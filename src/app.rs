//! App state, theme, layout and action dispatch.

use crate::config::Config;
use crate::ui::{self, pages, Action, Actions, Page, ToastKind};
use egui::{CornerRadius, Frame, Margin};
use material_3_ui::keys::take_key;
use material_3_ui::sys::arch::{self, OsRelease};
use material_3_ui::sys::task::Task;
use material_3_ui::sys::{cmd, term};
use material_3_ui::theme::{M3Palette, ThemeManager};
use material_3_ui::toasts::{Toast, Toasts};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

pub struct App {
    pub ctx: egui::Context,
    /// The current palette (a copy of the theme manager's).
    pub palette: M3Palette,
    pub theme: ThemeManager,
    pub config: Config,
    pub page: Page,
    pub nav_query: String,
    pub toasts: Toasts<Action>,
    pub actions: Actions,
    pub os: OsRelease,
    /// Installed pacman packages (names only).
    pub installed: Arc<HashSet<String>>,
    installed_task: Task<HashSet<String>>,
    /// Per-page state.
    pub pages: pages::States,
    /// The welcome flow, while it's showing (instead of the pages).
    pub welcome: Option<ui::welcome::State>,
    /// A package install was started in a terminal; re-read the package
    /// list when the window gets focus back.
    pending_package_refresh: bool,
    was_focused: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, start: Option<Page>, welcome: bool) -> Self {
        let ctx = cc.egui_ctx.clone();
        let config = Config::load();
        // Installs the fonts and follows the desktop's palette file.
        let theme = ThemeManager::new(&ctx, &config.appearance);

        let mut installed_task = Task::default();
        installed_task.spawn(&ctx, arch::installed_packages);

        let mut app = Self {
            palette: theme.palette().clone(),
            theme,
            config,
            page: start.unwrap_or(Page::Overview),
            nav_query: String::new(),
            toasts: Toasts::default(),
            actions: Vec::new(),
            os: OsRelease::load(),
            installed: Arc::new(HashSet::new()),
            installed_task,
            pages: pages::States::default(),
            welcome: welcome.then(ui::welcome::State::default),
            pending_package_refresh: false,
            was_focused: true,
            ctx,
        };
        app.theme_error();
        app
    }

    pub fn toast(&mut self, text: impl Into<String>, kind: ToastKind) {
        self.toasts.push(Toast::new(text, kind));
    }

    // ------------------------------------------------------------------ theme

    /// The palette file in use, if any.
    pub fn theme_file(&self) -> Option<PathBuf> {
        self.theme.theme_file(&self.config.appearance)
    }

    pub fn apply_theme(&mut self) {
        self.theme.apply(&self.config.appearance);
        self.palette = self.theme.palette().clone();
        self.theme_error();
    }

    /// Show a palette file that couldn't be read.
    fn theme_error(&mut self) {
        if let Some(e) = self.theme.take_error() {
            self.toast(e, ToastKind::Error);
        }
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
        if self.theme.poll(&self.config.appearance) {
            self.palette = self.theme.palette().clone();
            self.theme_error();
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
        if self.welcome.is_none() && take_key(ui.ctx(), egui::Modifiers::COMMAND, egui::Key::Comma) {
            self.actions.push(Action::Goto(Page::Preferences));
        }
        self.layout(ui);
        let fired = self.toasts.show(&self.ctx, &self.palette, 20.0);
        self.actions.extend(fired);
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
