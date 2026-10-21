//! App state, theme, layout and action dispatch.

use crate::config::{ColorScheme, Config, ThemeSource};
use crate::sys::arch::{self, OsRelease};
use crate::sys::task::Task;
use crate::sys::{cmd, term};
use crate::theme::{sources, style, M3Palette};
use crate::ui::toasts::{Toast, Toasts};
use crate::ui::{self, pages, widgets, Action, Actions, Page, ToastKind};
use egui::{CornerRadius, Frame, Margin};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub struct App {
    pub ctx: egui::Context,
    pub palette: M3Palette,
    pub config: Config,
    pub page: Page,
    pub nav_query: String,
    pub toasts: Toasts,
    pub actions: Actions,
    pub os: OsRelease,
    /// Installed pacman packages (names only).
    pub installed: Arc<HashSet<String>>,
    installed_task: Task<HashSet<String>>,
    /// Per-page state.
    pub pages: pages::States,
    /// The welcome flow, while it's showing (instead of the pages).
    pub welcome: Option<ui::welcome::State>,
    system_dark: Option<bool>,
    /// A package install was started in a terminal; re-read the package
    /// list when the window gets focus back.
    pending_package_refresh: bool,
    was_focused: bool,
    _theme_watcher: Option<notify::RecommendedWatcher>,
    theme_events: flume::Receiver<PathBuf>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, start: Option<Page>, welcome: bool) -> Self {
        let ctx = cc.egui_ctx.clone();
        style::install_fonts(&ctx);

        // Watch the palette files so the theme follows wallpaper changes.
        let (tx, theme_events) = flume::unbounded();
        let theme_watcher = {
            use notify::Watcher;
            let ctx = ctx.clone();
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(ev) = res {
                    if ev.kind.is_create() || ev.kind.is_modify() {
                        for p in ev.paths {
                            let _ = tx.send(p);
                        }
                        ctx.request_repaint_after(Duration::from_millis(150));
                    }
                }
            })
            .ok()
            .map(|mut w| {
                for file in [sources::end4_colors_path(), sources::file_manager_theme_path()]
                    .into_iter()
                    .flatten()
                {
                    if let Some(dir) = file.parent() {
                        let _ = w.watch(dir, notify::RecursiveMode::NonRecursive);
                    }
                }
                w
            })
        };

        let mut installed_task = Task::default();
        installed_task.spawn(&ctx, arch::installed_packages);

        let mut app = Self {
            palette: M3Palette::from_seed(crate::theme::palette::DEFAULT_SEED, true),
            config: Config::load(),
            page: start.unwrap_or(Page::Overview),
            nav_query: String::new(),
            toasts: Toasts::default(),
            actions: Vec::new(),
            os: OsRelease::load(),
            installed: Arc::new(HashSet::new()),
            installed_task,
            pages: pages::States::default(),
            welcome: welcome.then(ui::welcome::State::default),
            system_dark: ctx.system_theme().map(|t| t == egui::Theme::Dark),
            pending_package_refresh: false,
            was_focused: true,
            _theme_watcher: theme_watcher,
            theme_events,
            ctx,
        };
        app.apply_theme();
        app
    }

    pub fn toast(&mut self, text: impl Into<String>, kind: ToastKind) {
        self.toasts.push(Toast::new(text, kind));
    }

    // ------------------------------------------------------------------ theme

    fn wants_dark(&self) -> bool {
        match self.config.color_scheme {
            ColorScheme::Light => false,
            ColorScheme::Dark => true,
            ColorScheme::System => self.system_dark.unwrap_or(true),
        }
    }

    /// The palette file in use, if any.
    pub fn theme_file(&self) -> Option<PathBuf> {
        match self.config.theme_source {
            ThemeSource::Auto => sources::auto_theme_path(),
            ThemeSource::Seed => None,
        }
    }

    pub fn apply_theme(&mut self) {
        let dark = self.wants_dark();
        let palette = match self.theme_file() {
            Some(path) => match M3Palette::from_theme_json(&path) {
                Ok(p) => p,
                Err(e) => {
                    self.toast(format!("Couldn't read {}: {e}", path.display()), ToastKind::Error);
                    M3Palette::from_seed(&self.config.seed_color, dark)
                }
            },
            None => M3Palette::from_seed(&self.config.seed_color, dark),
        };
        style::apply(&self.ctx, &palette);
        widgets::set_palette(&self.ctx, palette.clone());
        widgets::set_filled_icons(self.config.filled_icons);
        let scale = self.config.ui_scale.clamp(0.6, 2.5);
        if (self.ctx.zoom_factor() - scale).abs() > f32::EPSILON {
            self.ctx.set_zoom_factor(scale);
        }
        self.palette = palette;
    }

    fn poll_background(&mut self) {
        if self.installed_task.poll() {
            if let Some(set) = self.installed_task.value.take() {
                self.installed = Arc::new(set);
            }
        }
        let focused = self.ctx.input(|i| i.focused);
        if focused && !self.was_focused && self.pending_package_refresh {
            self.pending_package_refresh = false;
            self.installed_task.spawn(&self.ctx, arch::installed_packages);
        }
        self.was_focused = focused;
        let changed: Vec<PathBuf> = self.theme_events.try_iter().collect();
        if let Some(theme) = self.theme_file() {
            if changed.contains(&theme) {
                self.apply_theme();
            }
        }
        let system_dark = self.ctx.system_theme().map(|t| t == egui::Theme::Dark);
        if system_dark != self.system_dark {
            self.system_dark = system_dark;
            if self.config.color_scheme == ColorScheme::System && self.theme_file().is_none() {
                self.apply_theme();
            }
        }
    }

    // ---------------------------------------------------------------- actions

    pub fn run_in_terminal(&mut self, script: &str) {
        match term::run(&self.config.terminal, script) {
            Ok(t) => self.toast(format!("Running in {t}"), ToastKind::Info),
            Err(e) => self.toast(e, ToastKind::Error),
        }
    }

    fn process(&mut self, action: Action) {
        match action {
            Action::Goto(page) => {
                self.page = page;
                self.nav_query.clear();
            }
            Action::Toast(text, kind) => self.toast(text, kind),
            Action::Install { packages } => {
                let refs: Vec<&str> = packages.iter().map(String::as_str).collect();
                let cmd = arch::install_command(&refs, self.config.aur_helper);
                self.run_in_terminal(&cmd);
                // Pick up the new packages once the install is likely done.
                self.pending_package_refresh = true;
            }
            Action::RunInTerminal(script) => self.run_in_terminal(&script),
            Action::OpenPath(path) => {
                if let Err(e) = cmd::spawn_detached("xdg-open", &[&path.to_string_lossy()]) {
                    self.toast(e, ToastKind::Error);
                }
            }
            Action::OpenUrl(url) => {
                if let Err(e) = cmd::spawn_detached("xdg-open", &[&url]) {
                    self.toast(e, ToastKind::Error);
                }
            }
            Action::RefreshPackages => {
                self.installed_task.spawn(&self.ctx, arch::installed_packages);
            }
            Action::ConfigChanged => {
                if let Err(e) = self.config.save() {
                    self.toast(format!("Couldn't save preferences: {e}"), ToastKind::Error);
                }
                self.apply_theme();
            }
            Action::StartWelcome => self.welcome = Some(ui::welcome::State::default()),
            Action::FinishWelcome { open_settings } => {
                self.finish_welcome();
                self.welcome = None;
                if open_settings {
                    self.page = Page::Overview;
                } else {
                    self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    /// Record that the welcome was seen: in the shell's marker (so it
    /// doesn't greet again, unless asked to) and in our config.
    fn finish_welcome(&mut self) {
        let Some(w) = self.welcome.as_mut() else { return };
        if w.flow.finished {
            return;
        }
        w.flow.finished = true;
        let show = w.flow.show_next_time;
        if let Err(e) = crate::welcome::set_show_next_time(&crate::welcome::marker_path(), show) {
            self.toast(format!("Couldn't update the first-run marker: {e}"), ToastKind::Error);
        }
        self.config.welcome_done = true;
        if let Err(e) = self.config.save() {
            self.toast(format!("Couldn't save preferences: {e}"), ToastKind::Error);
        }
    }

    fn process_actions(&mut self) {
        while !self.actions.is_empty() {
            let batch = std::mem::take(&mut self.actions);
            for action in batch {
                self.process(action);
            }
        }
    }

    fn layout(&mut self, ui: &mut egui::Ui) {
        let p = self.palette.clone();
        let surface = Frame::new().fill(p.surface);

        if self.welcome.is_some() {
            egui::CentralPanel::default()
                .frame(surface.inner_margin(Margin::same(12)))
                .show(ui, |ui| {
                    Frame::new()
                        .fill(p.surface_container_low)
                        .corner_radius(CornerRadius::same(24))
                        .show(ui, |ui| {
                            ui.set_min_size(ui.available_size());
                            ui::welcome::show(ui, self);
                        });
                });
            return;
        }

        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(256.0)
            .size_range(210.0..=340.0)
            .show_separator_line(false)
            .frame(surface.inner_margin(Margin {
                left: 12,
                right: 4,
                top: 12,
                bottom: 12,
            }))
            .show(ui, |ui| ui::sidebar::show(ui, self));

        egui::CentralPanel::default()
            .frame(surface.inner_margin(Margin {
                left: 8,
                right: 12,
                top: 12,
                bottom: 12,
            }))
            .show(ui, |ui| {
                Frame::new()
                    .fill(p.surface_container_low)
                    .corner_radius(CornerRadius::same(24))
                    .show(ui, |ui| {
                        ui.set_min_size(ui.available_size());
                        pages::show(ui, self);
                    });
            });
    }
}

impl eframe::App for App {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_background();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.welcome.is_none() && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Comma)) {
            self.actions.push(Action::Goto(Page::Preferences));
        }
        self.layout(ui);
        ui::toasts::show(&self.ctx.clone(), self);
        self.process_actions();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.palette.surface.to_normalized_gamma_f32()
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Closing the window counts as seeing the welcome, like the shell's.
        self.finish_welcome();
        let _ = self.config.save();
    }
}
