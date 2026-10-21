//! Material 3 apps: install, update, open and remove the companion apps.
//! The essential ones (Settings, Files, Screenshots) come with the end4-pC
//! shell and can be updated or repaired here; the optional ones are
//! installed from here.

use crate::app::App;
use crate::material_apps::install::{self, Event};
use crate::material_apps::release::{self, Latest, Release, Remote};
use crate::material_apps::{self as m3, Availability, Dirs, Installed, LocalState, MaterialApp, Method, APPS};
use material_3_ui::sys::arch;
use material_3_ui::sys::task::Task;
use material_3_ui::sys::{cmd, paths};
use material_3_ui::common;
use material_3_ui::widgets::{self, ButtonKind};
use crate::ui::{Action, Page, ToastKind};
use egui::{pos2, vec2, Color32, CornerRadius, FontId, Id, Rect, RichText, Sense, Ui};
use material_3_ui::icons::icons::*;
use std::collections::HashMap;
use std::time::Duration;

/// An install or removal running on a background thread.
struct Job {
    rx: flume::Receiver<Event>,
    stage: String,
    progress: Option<f32>,
}

#[derive(Default)]
pub struct State {
    local: Task<LocalState>,
    remote: Task<Remote>,
    jobs: HashMap<&'static str, Job>,
    confirm_remove: Option<&'static str>,
    was_focused: bool,
}

/// What a card or detail page shows for an app.
#[derive(Clone, Debug, PartialEq)]
enum Status {
    NotInstalled,
    Installed(Option<String>),
    Update { to: String },
    Package(Option<String>),
    Other,
}

impl Status {
    fn label(&self) -> String {
        match self {
            Status::NotInstalled => "Not installed".into(),
            Status::Installed(Some(v)) if v.starts_with("built") => "Installed, built from source".into(),
            Status::Installed(Some(v)) => format!("Installed {v}"),
            Status::Installed(None) => "Installed".into(),
            Status::Update { to } => format!("Update available: {to}"),
            Status::Package(_) => "Installed by package manager".into(),
            Status::Other => "Installed another way".into(),
        }
    }
}

/// The release to update to, for an app installed here that's out of date.
fn pending_update<'a>(installed: &Installed, latest: &'a Latest) -> Option<&'a Release> {
    match (installed, latest) {
        (Installed::Managed { version }, Latest::Found(r)) if r.tarball.is_some() && release::is_newer(&r.tag, version) => Some(r),
        _ => None,
    }
}

fn status(installed: &Installed, latest: &Latest) -> Status {
    if let Some(r) = pending_update(installed, latest) {
        return Status::Update { to: r.tag.clone() };
    }
    match installed {
        Installed::NotInstalled => Status::NotInstalled,
        Installed::Managed { .. } => Status::Installed(installed.version_label()),
        Installed::Package { version, .. } => Status::Package(version.clone()),
        Installed::Other { .. } => Status::Other,
    }
}

fn availability(app: &App, remote: &Remote, a: &MaterialApp) -> Availability {
    Availability {
        arch_based: app.os.is_arch_based(),
        aur_helper: app.config.aur_helper.resolve().is_some(),
        aur_published: remote.aur_published(a),
        release: match remote.latest(a) {
            Latest::Found(r) => Some(r.tarball.is_some() && std::env::consts::ARCH == "x86_64"),
            Latest::None => Some(false),
            Latest::Unknown => None,
        },
    }
}

fn preferred(app: &App, a: &MaterialApp) -> Method {
    app.config.material_app_methods.get(a.id).copied().unwrap_or_default()
}

fn binary_path(a: &MaterialApp, installed: &Installed, dirs: &Dirs) -> String {
    match installed {
        Installed::Managed { .. } => dirs.binary(a).to_string_lossy().into_owned(),
        Installed::Other { path } => path.to_string_lossy().into_owned(),
        _ => a.bin.to_string(),
    }
}

/// Everything a click can ask for, carried out after drawing.
enum Op {
    Install(&'static MaterialApp, Method),
    UpdateRelease(&'static MaterialApp, Release),
    UpdatePackage(String),
    Open(&'static MaterialApp),
    Remove(&'static MaterialApp),
    EnableService(&'static MaterialApp),
}

fn spawn_job(st: &mut State, ctx: &egui::Context, a: &'static MaterialApp, stage: &str, work: impl FnOnce(&flume::Sender<Event>) -> Result<String, String> + Send + 'static) {
    let (tx, rx) = flume::unbounded();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let result = work(&tx);
        let _ = tx.send(Event::Done(result));
        ctx.request_repaint();
    });
    st.jobs.insert(a.id, Job { rx, stage: stage.into(), progress: None });
}

fn run_op(app: &mut App, op: Op) {
    let ctx = app.ctx.clone();
    let dirs = Dirs::from_env();
    let remote = app.pages.material_apps.remote.value.clone().unwrap_or_default();
    let local = app.pages.material_apps.local.value.clone().unwrap_or_default();
    match op {
        Op::Install(a, method) => match method {
            Method::Aur => {
                app.actions.push(Action::Install { packages: vec![remote.aur_package(a)] });
            }
            Method::Source => {
                app.actions.push(Action::RunInTerminal(install::source_script(a, &dirs)));
            }
            Method::Release | Method::Auto => match remote.latest(a).clone() {
                Latest::Found(r) if r.tarball.is_some() => {
                    let st = &mut app.pages.material_apps;
                    spawn_job(st, &ctx, a, "Starting…", move |tx| install::install_release(a, &dirs, &r, tx));
                }
                _ => app.toast(format!("{} has no release to download yet", a.name), ToastKind::Error),
            },
        },
        Op::UpdateRelease(a, r) => {
            let st = &mut app.pages.material_apps;
            spawn_job(st, &ctx, a, "Starting…", move |tx| install::install_release(a, &dirs, &r, tx));
        }
        Op::UpdatePackage(package) => {
            let cmd = arch::install_command(&[&package], app.config.aur_helper);
            app.actions.push(Action::RunInTerminal(cmd));
        }
        Op::Open(a) => {
            let program = binary_path(a, local.get(a), &dirs);
            if let Err(e) = cmd::spawn_detached(&program, &[]) {
                app.toast(e, ToastKind::Error);
            }
        }
        Op::Remove(a) => match local.get(a).clone() {
            Installed::Package { name, .. } => {
                app.actions.push(Action::RunInTerminal(install::package_remove_script(a, &name)));
            }
            Installed::Managed { .. } => {
                let st = &mut app.pages.material_apps;
                spawn_job(st, &ctx, a, "Removing…", move |_| install::remove_local(a, &dirs));
            }
            _ => {}
        },
        Op::EnableService(a) => {
            if let Some(service) = a.service {
                let st = &mut app.pages.material_apps;
                spawn_job(st, &ctx, a, "Starting the background service…", move |_| {
                    cmd::run("systemctl", &["--user", "daemon-reload"]).ok();
                    cmd::run("systemctl", &["--user", "enable", "--now", service])
                        .map(|_| format!("{}'s background service is running", a.name))
                });
            }
        }
    }
}

fn refresh_local(st: &mut State, ctx: &egui::Context) {
    st.local.spawn(ctx, || m3::read_local(&Dirs::from_env()));
}

/// Poll background work and refresh what's stale. Shared by both views
/// and the Overview.
pub fn poll(app: &mut App) {
    let ctx = app.ctx.clone();
    let cache = Dirs::from_env().cache.join("releases");
    let st = &mut app.pages.material_apps;
    st.local.ensure(&ctx, || m3::read_local(&Dirs::from_env()));
    {
        let cache = cache.clone();
        st.remote.ensure(&ctx, move || release::check(&cache, false));
    }
    if st.remote.finished_at.is_some_and(|t| t.elapsed() > release::MAX_AGE) && !st.remote.running() {
        st.remote.spawn(&ctx, move || release::check(&cache, false));
    }

    // Installs and removals done in a terminal show up when the window gets
    // focus back.
    let focused = ctx.input(|i| i.focused);
    if focused && !st.was_focused {
        refresh_local(st, &ctx);
        app.actions.push(Action::RefreshPackages);
    }
    st.was_focused = focused;

    let mut finished = Vec::new();
    for (id, job) in st.jobs.iter_mut() {
        for ev in job.rx.try_iter() {
            match ev {
                Event::Stage(s) => {
                    job.stage = s;
                    job.progress = None;
                }
                Event::Progress(f) => job.progress = Some(f),
                Event::Done(r) => finished.push((*id, r)),
            }
        }
    }
    let any_finished = !finished.is_empty();
    for (id, result) in finished {
        st.jobs.remove(id);
        match result {
            Ok(msg) => app.actions.push(Action::Toast(msg, ToastKind::Success)),
            Err(e) => app.actions.push(Action::Toast(e, ToastKind::Error)),
        }
    }
    if any_finished {
        refresh_local(&mut app.pages.material_apps, &ctx);
    }
    if !app.pages.material_apps.jobs.is_empty() {
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

/// The main button for an app: Install, Update or Open.
fn primary_button(ui: &mut Ui, app: &App, a: &'static MaterialApp, local: &LocalState, remote: &Remote, ops: &mut Vec<Op>) {
    let installed = local.get(a);
    let latest = remote.latest(a);
    if let Some(r) = pending_update(installed, latest) {
        if widgets::button(ui, ButtonKind::Filled, Some(ICON_UPDATE), "Update").clicked() {
            ops.push(Op::UpdateRelease(a, r.clone()));
        }
    } else if installed.is_installed() {
        if widgets::button(ui, ButtonKind::Tonal, Some(ICON_OPEN_IN_NEW), "Open").clicked() {
            ops.push(Op::Open(a));
        }
    } else {
        let method = m3::choose(preferred(app, a), availability(app, remote, a));
        // Until the first check is back, the method isn't known.
        let busy = remote.latest.is_empty();
        let resp = widgets::button_enabled(ui, ButtonKind::Filled, Some(ICON_DOWNLOAD), "Install", !busy)
            .on_hover_text(install_hint(method));
        if resp.clicked() {
            ops.push(Op::Install(a, method));
        }
    }
}

fn install_hint(method: Method) -> &'static str {
    match method {
        Method::Aur => "Installs the AUR package in a terminal",
        Method::Release => "Downloads the latest release into ~/.local/bin",
        Method::Source => "Builds it from source in a terminal (needs cargo and git)",
        Method::Auto => "",
    }
}

fn status_chip(ui: &Ui, pos: egui::Pos2, status: &Status, max_w: f32) -> Rect {
    let p = widgets::palette(ui);
    let (fill, fg, stroke) = match status {
        Status::NotInstalled => (Color32::TRANSPARENT, p.on_surface_variant, Some(p.outline_variant)),
        Status::Update { .. } => (p.tertiary_container, p.on_tertiary_container, None),
        Status::Installed(_) | Status::Package(_) => (p.secondary_container, p.on_secondary_container, None),
        Status::Other => (p.surface_container_highest, p.on_surface_variant, None),
    };
    let icon = match status {
        Status::NotInstalled => ICON_DOWNLOAD,
        Status::Update { .. } => ICON_UPDATE,
        Status::Package(_) => ICON_INVENTORY_2,
        _ => ICON_CHECK_CIRCLE,
    };
    let galley = widgets::truncated_galley(ui, &status.label(), FontId::proportional(12.0), fg, max_w - 36.0);
    let rect = Rect::from_min_size(pos, vec2(galley.size().x + 36.0, 26.0));
    ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    if let Some(s) = stroke {
        ui.painter().rect_stroke(rect, CornerRadius::same(8), egui::Stroke::new(1.0, s), egui::StrokeKind::Inside);
    }
    widgets::paint_icon(ui, icon, pos2(rect.left() + 16.0, rect.center().y), 15.0, fg);
    ui.painter().galley(pos2(rect.left() + 28.0, rect.center().y - galley.size().y / 2.0), galley, fg);
    rect
}

const CARD_W: f32 = 300.0;
const CARD_H: f32 = 136.0;
const GAP: f32 = 12.0;

/// What the cards read from.
struct View<'a> {
    app: &'a App,
    st: &'a State,
    local: &'a LocalState,
    remote: &'a Remote,
}

fn app_card(ui: &mut Ui, rect: Rect, v: &View, a: &'static MaterialApp, ops: &mut Vec<Op>) -> bool {
    let View { app, st, local, remote } = *v;
    let p = widgets::palette(ui);
    let installed = local.get(a);
    let (_, resp) = widgets::clickable_card(ui, rect.size(), p.surface_container, Id::unique(("m3-app-card", a.id)));
    let (circle, fg) = if installed.is_installed() {
        (p.primary_container, p.on_primary_container)
    } else {
        (p.surface_container_highest, p.on_surface_variant)
    };
    let c = pos2(rect.left() + 38.0, rect.top() + 38.0);
    ui.painter().circle_filled(c, 22.0, circle);
    widgets::paint_icon(ui, a.icon, c, 24.0, fg);
    let text_x = rect.left() + 72.0;
    let text_w = rect.right() - text_x - 16.0;
    let name = widgets::truncated_galley(ui, a.name, widgets::title_font(16.0), p.on_surface, text_w);
    ui.painter().galley(pos2(text_x, rect.top() + 18.0), name, p.on_surface);
    let mut job = egui::text::LayoutJob::single_section(
        a.summary.to_string(),
        egui::TextFormat::simple(FontId::proportional(12.5), p.on_surface_variant),
    );
    job.wrap = egui::text::TextWrapping {
        max_width: text_w,
        max_rows: 2,
        break_anywhere: false,
        overflow_character: Some('…'),
    };
    let summary = ui.painter().layout_job(job);
    ui.painter().galley(pos2(text_x, rect.top() + 42.0), summary, p.on_surface_variant);

    let bottom = Rect::from_min_max(pos2(rect.left() + 16.0, rect.bottom() - 52.0), pos2(rect.right() - 16.0, rect.bottom() - 12.0));
    if let Some(job) = st.jobs.get(a.id) {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bottom).layout(egui::Layout::top_down(egui::Align::Min)));
        child.label(RichText::new(&job.stage).size(12.0).color(p.on_surface_variant));
        child.add_space(6.0);
        widgets::linear_progress(&mut child, job.progress, bottom.width());
    } else {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bottom).layout(egui::Layout::right_to_left(egui::Align::Center)));
        primary_button(&mut child, app, a, local, remote, ops);
        let button_w = child.min_rect().width();
        let status = status(installed, remote.latest(a));
        status_chip(ui, pos2(bottom.left(), bottom.center().y - 13.0), &status, bottom.width() - button_w - 8.0);
    }
    resp.clicked()
}

fn grid(ui: &mut Ui, apps: &[&'static MaterialApp], mut card: impl FnMut(&mut Ui, Rect, &'static MaterialApp)) {
    let width = ui.available_width();
    let cols = (((width + GAP) / (CARD_W + GAP)).floor() as usize).max(1);
    let w = (width - GAP * (cols as f32 - 1.0)) / cols as f32;
    let rows = apps.len().div_ceil(cols);
    let (area, _) = ui.allocate_exact_size(vec2(width, rows as f32 * (CARD_H + GAP) - GAP), Sense::hover());
    for (i, a) in apps.iter().enumerate() {
        let (r, c) = (i / cols, i % cols);
        let rect = Rect::from_min_size(pos2(area.left() + c as f32 * (w + GAP), area.top() + r as f32 * (CARD_H + GAP)), vec2(w, CARD_H));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        card(&mut child, rect, a);
    }
}

fn section(ui: &mut Ui, title: &str, text: &str) {
    let p = widgets::palette(ui);
    ui.add_space(24.0);
    ui.label(widgets::title_text(title, 16.0).color(p.on_surface));
    ui.add(egui::Label::new(RichText::new(text).size(13.0).color(p.on_surface_variant)).wrap());
    ui.add_space(10.0);
}

/// A banner offering to start a service that isn't enabled yet.
fn service_banner(ui: &mut Ui, a: &'static MaterialApp, local: &LocalState, busy: bool, ops: &mut Vec<Op>) {
    let Some(service) = a.service else { return };
    if !local.get(a).is_installed() || local.services_enabled.contains(service) {
        return;
    }
    let text = format!("{} works best with its background service running: it keeps events synced offline, sends reminders and feeds the shell.", a.name);
    common::banner(ui, ICON_SCHEDULE_SEND, &text, false, |ui| {
        if widgets::button_enabled(ui, ButtonKind::Filled, None, "Start in background (recommended)", !busy).clicked() {
            ops.push(Op::EnableService(a));
        }
    });
}

pub fn show_list(ui: &mut Ui, app: &mut App) {
    poll(app);
    let p = app.palette.clone();
    let local = app.pages.material_apps.local.value.clone().unwrap_or_default();
    let remote = app.pages.material_apps.remote.value.clone().unwrap_or_default();
    let checking = app.pages.material_apps.remote.running();
    let mut ops = Vec::new();
    let mut goto = None;
    let mut force_check = false;

    let updates = APPS.iter().filter(|a| pending_update(local.get(a), remote.latest(a)).is_some()).count();
    let st = &app.pages.material_apps;
    let view = View { app, st, local: &local, remote: &remote };

    common::page(ui, ICON_WIDGETS, "Material 3 apps", "Apps made to match this desktop. They share its look and take their colors from your wallpaper.", |ui| {
        ui.add_space(4.0);
        let size = vec2(ui.available_width(), 32.0);
        ui.allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Center), |ui| {
            let text = if checking {
                "Checking for updates…".to_string()
            } else if updates > 0 {
                format!("{updates} update{} available", if updates == 1 { "" } else { "s" })
            } else if remote.latest.is_empty() {
                String::new()
            } else {
                "Everything is up to date".to_string()
            };
            ui.label(RichText::new(text).color(p.on_surface_variant));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if common::refresh_button(ui, checking) {
                    force_check = true;
                }
            });
        });
        if let Some(problem) = &remote.problem {
            common::banner(ui, ICON_CLOUD_OFF, problem, false, |_| {});
        }
        for a in APPS.iter().filter(|a| a.service.is_some()) {
            service_banner(ui, a, &local, st.jobs.contains_key(a.id), &mut ops);
        }

        let essential: Vec<&'static MaterialApp> = APPS.iter().filter(|a| a.essential).collect();
        let optional: Vec<&'static MaterialApp> = APPS.iter().filter(|a| !a.essential).collect();
        section(ui, "Installed with the shell", "The shell depends on these, so it installs them itself. You can update or repair them here, but not remove them.");
        grid(ui, &essential, |ui, rect, a| {
            if app_card(ui, rect, &view, a, &mut ops) {
                goto = Some(a.id);
            }
        });
        section(ui, "Optional", "Install the ones you want. Each card opens a page with what the app needs and how it's installed.");
        grid(ui, &optional, |ui, rect, a| {
            if app_card(ui, rect, &view, a, &mut ops) {
                goto = Some(a.id);
            }
        });
    });

    if force_check {
        let cache = Dirs::from_env().cache.join("releases");
        let ctx = app.ctx.clone();
        app.pages.material_apps.remote.spawn(&ctx, move || release::check(&cache, true));
        refresh_local(&mut app.pages.material_apps, &ctx);
    }
    if let Some(id) = goto {
        app.actions.push(Action::Goto(Page::MaterialApp(id.to_string())));
    }
    for op in ops {
        run_op(app, op);
    }
}

pub fn show_app(ui: &mut Ui, app: &mut App, id: &str) {
    let Some(a) = m3::find(id) else {
        app.actions.push(Action::Goto(Page::MaterialApps));
        return;
    };
    poll(app);
    let p = app.palette.clone();
    let dirs = Dirs::from_env();
    let local = app.pages.material_apps.local.value.clone().unwrap_or_default();
    let remote = app.pages.material_apps.remote.value.clone().unwrap_or_default();
    let installed = local.get(a).clone();
    let latest = remote.latest(a).clone();
    let av = availability(app, &remote, a);
    let mut method_pref = preferred(app, a);
    let method = m3::choose(method_pref, av);
    let mut ops = Vec::new();
    let mut actions = Vec::new();
    let mut confirm = app.pages.material_apps.confirm_remove;
    let st = &app.pages.material_apps;
    let busy = st.jobs.contains_key(a.id);
    let app_ref: &App = app;

    egui::ScrollArea::vertical().id_salt(("m3-app", a.id)).auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(32, 24)).show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(common::PAGE_WIDTH));
            // ------------------------------------------------------ header
            ui.horizontal(|ui| {
                if widgets::icon_button(ui, ICON_ARROW_BACK, "All Material 3 apps", true).clicked() {
                    actions.push(Action::Goto(Page::MaterialApps));
                }
                ui.add_space(4.0);
                let (rect, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
                let (circle, fg) = if installed.is_installed() {
                    (p.primary_container, p.on_primary_container)
                } else {
                    (p.surface_container_highest, p.on_surface_variant)
                };
                ui.painter().circle_filled(rect.center(), 28.0, circle);
                widgets::paint_icon(ui, a.icon, rect.center(), 30.0, fg);
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(a.name).size(28.0).color(p.on_surface));
                    ui.label(RichText::new(a.summary).size(14.0).color(p.on_surface_variant));
                });
            });

            // ------------------------------------------------------ buttons
            ui.add_space(16.0);
            if let Some(job) = st.jobs.get(a.id) {
                ui.label(RichText::new(&job.stage).color(p.on_surface_variant));
                ui.add_space(6.0);
                widgets::linear_progress(ui, job.progress, ui.available_width().min(420.0));
                ui.add_space(8.0);
            } else {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    primary_button(ui, app_ref, a, &local, &remote, &mut ops);
                    if pending_update(&installed, &latest).is_some()
                        && widgets::button(ui, ButtonKind::Tonal, Some(ICON_OPEN_IN_NEW), "Open").clicked()
                    {
                        ops.push(Op::Open(a));
                    }
                    if let Installed::Package { name, .. } = &installed {
                        if widgets::button(ui, ButtonKind::Outlined, Some(ICON_SYSTEM_UPDATE_ALT), "Update package")
                            .on_hover_text("Checks the AUR for a newer version, in a terminal")
                            .clicked()
                        {
                            ops.push(Op::UpdatePackage(name.clone()));
                        }
                    }
                    let removable = !a.essential && matches!(installed, Installed::Managed { .. } | Installed::Package { .. });
                    if removable && widgets::button(ui, ButtonKind::Text, Some(ICON_DELETE), "Remove").clicked() {
                        confirm = Some(a.id);
                    }
                    if widgets::button(ui, ButtonKind::Text, Some(ICON_CODE), "Project page").clicked() {
                        actions.push(Action::OpenUrl(a.project_url()));
                    }
                });
            }
            service_banner(ui, a, &local, busy, &mut ops);
            if a.essential {
                common::banner(ui, ICON_LOCK, "Installed with the shell. You can update or repair it here, but not remove it.", false, |_| {});
            }

            // ------------------------------------------------------ about
            common::card(ui, "About", |ui| common::note(ui, a.description));

            // ------------------------------------------------------ needs
            if !a.needs.is_empty() {
                common::card(ui, "What it needs", |ui| {
                    for need in a.needs {
                        let met = local.need_met(need);
                        let what = if need.programs == [need.package] {
                            need.package.to_string()
                        } else {
                            format!("{} · package {}", need.programs.join(" or "), need.package)
                        };
                        let desc = format!(
                            "{what}{}",
                            if need.required { "" } else { " (optional)" }
                        );
                        common::row(ui, need.purpose, &desc, |ui| {
                            if met {
                                widgets::icon(ui, ICON_CHECK_CIRCLE, 22.0, p.primary);
                            } else if widgets::button(ui, if need.required { ButtonKind::Filled } else { ButtonKind::Tonal }, Some(ICON_DOWNLOAD), "Install").clicked() {
                                actions.push(Action::Install { packages: vec![need.package.to_string()] });
                            }
                        });
                    }
                });
            }

            // ------------------------------------------------------ install
            common::card(ui, "Installation", |ui| {
                let status = status(&installed, &latest);
                let (how, location) = match &installed {
                    Installed::NotInstalled => (
                        format!("{}{}", method.label(), if method_pref == Method::Auto { " (chosen automatically)" } else { "" }),
                        String::new(),
                    ),
                    Installed::Managed { version } if version.starts_with("source-") => {
                        ("Built from source".to_string(), paths::tilde(&dirs.binary(a)))
                    }
                    Installed::Managed { .. } => ("GitHub release".to_string(), paths::tilde(&dirs.binary(a))),
                    Installed::Package { name, .. } => (format!("Package {name}. Updated with system updates."), format!("/usr/bin/{}", a.bin)),
                    Installed::Other { path } => ("Installed outside Settings, so it isn't updated here".to_string(), paths::tilde(path)),
                };
                let mut facts = vec![("Status", status.label()), ("Method", how)];
                if let Some(v) = installed.version_label() {
                    facts.push(("Version", v));
                }
                facts.push((
                    "Latest release",
                    match &latest {
                        Latest::Found(r) => r.tag.clone(),
                        Latest::None => "None yet".into(),
                        Latest::Unknown => "Not checked yet".into(),
                    },
                ));
                facts.push((
                    "AUR",
                    match av.aur_published {
                        Some(true) => remote.aur_package(a),
                        Some(false) => "Not published".into(),
                        None => "Not checked".into(),
                    },
                ));
                if !location.is_empty() {
                    facts.push(("Location", location));
                }
                common::facts(ui, &format!("m3-app-facts-{}", a.id), &facts);
            });

            // ------------------------------------------------------ advanced
            common::card(ui, "Advanced", |ui| {
                common::row(ui, "Install method", "Automatic uses the AUR package when it's published and paru or yay is installed, then the latest GitHub release, then builds from source.", |_| {});
                ui.horizontal_wrapped(|ui| {
                    let options = [
                        (Method::Auto, ICON_AUTO_AWESOME, "Automatic"),
                        (Method::Aur, ICON_INVENTORY_2, "AUR"),
                        (Method::Release, ICON_DOWNLOAD, "Release"),
                        (Method::Source, ICON_BUILD, "Source"),
                    ];
                    widgets::segmented(ui, &mut method_pref, &options);
                });
                ui.add_space(8.0);
                if method_pref != Method::Auto && method != method_pref {
                    common::note(ui, &format!("{} isn't possible here right now, so {} is used.", method_pref.label(), method.label()));
                }
                if installed.is_installed() && !matches!(installed, Installed::Package { .. }) {
                    common::row(ui, "Reinstall", "Install again with the method above, replacing what's there. Use this to repair an install.", |ui| {
                        if widgets::button_enabled(ui, ButtonKind::Outlined, Some(ICON_BUILD), "Reinstall", !busy).clicked() {
                            ops.push(Op::Install(a, method));
                        }
                    });
                }
                common::row(ui, "Data", "Where the shell's installer and this page record installed versions.", |ui| {
                    if widgets::button(ui, ButtonKind::Text, Some(ICON_FOLDER_OPEN), "Open").clicked() {
                        actions.push(Action::OpenPath(dirs.state.clone()));
                    }
                });
            });
        });
    });

    // ---------------------------------------------------------- confirm
    if confirm == Some(a.id) {
        let what = match &installed {
            Installed::Package { name, .. } => format!("This removes the {name} package with pacman, in a terminal."),
            _ => format!("This deletes {} and its desktop entry. Your settings and your own files stay.", paths::tilde(&dirs.binary(a))),
        };
        let resp = common::modal(&app.ctx, &p, "m3-remove", ICON_DELETE, &format!("Remove {}?", a.name), |ui| {
            ui.set_max_width(380.0);
            ui.add(egui::Label::new(RichText::new(&what).color(p.on_surface_variant)).wrap());
            let mut choice = None;
            common::dialog_buttons(ui, |ui| {
                if widgets::button(ui, ButtonKind::Danger, None, "Remove").clicked() {
                    choice = Some(true);
                }
                if widgets::button(ui, ButtonKind::Text, None, "Cancel").clicked() {
                    choice = Some(false);
                }
            });
            choice
        });
        match resp.inner {
            Some(true) => {
                ops.push(Op::Remove(a));
                confirm = None;
            }
            Some(false) => confirm = None,
            None if resp.should_close() => confirm = None,
            None => {}
        }
    }
    app.pages.material_apps.confirm_remove = confirm;

    if method_pref != preferred(app, a) {
        if method_pref == Method::Auto {
            app.config.material_app_methods.remove(a.id);
        } else {
            app.config.material_app_methods.insert(a.id.to_string(), method_pref);
        }
        app.actions.push(Action::ConfigChanged);
    }
    app.actions.extend(actions);
    for op in ops {
        run_op(app, op);
    }
}

/// For the Overview: how many apps are installed, and updates waiting.
pub fn summary(app: &App) -> Option<(usize, usize)> {
    let st = &app.pages.material_apps;
    let local = st.local.value.as_ref()?;
    let remote = st.remote.value.clone().unwrap_or_default();
    let installed = APPS.iter().filter(|a| local.get(a).is_installed()).count();
    let updates = APPS.iter().filter(|a| pending_update(local.get(a), remote.latest(a)).is_some()).count();
    Some((installed, updates))
}

/// How a set of installs is carried out: AUR packages in one terminal,
/// source builds one after another in another, releases downloaded here.
#[derive(Default)]
struct Batch {
    aur: Vec<&'static MaterialApp>,
    source: Vec<&'static MaterialApp>,
    release: Vec<&'static MaterialApp>,
}

fn batch(items: &[(&'static MaterialApp, Method)]) -> Batch {
    let mut b = Batch::default();
    for (a, method) in items {
        match method {
            Method::Aur => b.aur.push(a),
            Method::Source => b.source.push(a),
            Method::Release | Method::Auto => b.release.push(a),
        }
    }
    b
}

/// The release check is back, so install methods are known.
pub fn ready(app: &App) -> bool {
    app.pages.material_apps.remote.value.as_ref().is_some_and(|r| !r.latest.is_empty())
}

pub fn is_installed(app: &App, a: &MaterialApp) -> bool {
    app.pages.material_apps.local.value.as_ref().is_some_and(|l| l.get(a).is_installed())
}

/// A download or removal in progress: its stage and progress.
pub fn job(app: &App, a: &MaterialApp) -> Option<(String, Option<f32>)> {
    app.pages.material_apps.jobs.get(a.id).map(|j| (j.stage.clone(), j.progress))
}

/// Install several apps the way each card's Install button would.
pub fn install_many(app: &mut App, apps: &[&'static MaterialApp]) {
    let remote = app.pages.material_apps.remote.value.clone().unwrap_or_default();
    let items: Vec<(&'static MaterialApp, Method)> = apps
        .iter()
        .map(|a| (*a, m3::choose(preferred(app, a), availability(app, &remote, a))))
        .collect();
    let b = batch(&items);
    if !b.aur.is_empty() {
        app.actions.push(Action::Install { packages: b.aur.iter().map(|a| remote.aur_package(a)).collect() });
    }
    if !b.source.is_empty() {
        let dirs = Dirs::from_env();
        let script = b.source.iter().map(|a| install::source_script(a, &dirs)).collect::<Vec<_>>().join("; ");
        app.actions.push(Action::RunInTerminal(script));
    }
    for a in b.release {
        run_op(app, Op::Install(a, Method::Release));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses() {
        let found = Latest::Found(Release {
            tag: "v0.2.0".into(),
            tarball: Some("https://x/a.tar.gz".into()),
            ..Default::default()
        });
        let managed = |v: &str| Installed::Managed { version: v.into() };
        assert_eq!(status(&managed("v0.1.0"), &found), Status::Update { to: "v0.2.0".into() });
        assert_eq!(status(&managed("v0.2.0"), &found), Status::Installed(Some("v0.2.0".into())));
        assert_eq!(status(&managed("source-abc"), &found), Status::Update { to: "v0.2.0".into() });
        assert_eq!(status(&managed("source-abc"), &Latest::None).label(), "Installed, built from source");
        assert_eq!(status(&Installed::NotInstalled, &found), Status::NotInstalled);
        let pkg = Installed::Package { name: "material-3-music-bin".into(), version: Some("0.1.0-1".into()) };
        // Packages are updated by pacman, never from a release.
        assert_eq!(status(&pkg, &found).label(), "Installed by package manager");
        // A release without a Linux build isn't offered as an update.
        let no_build = Latest::Found(Release { tag: "v0.3.0".into(), ..Default::default() });
        assert!(pending_update(&managed("v0.1.0"), &no_build).is_none());
    }

    #[test]
    fn batches_by_method() {
        let app = |id: &str| m3::find(id).unwrap();
        let b = batch(&[
            (app("music"), Method::Aur),
            (app("games"), Method::Release),
            (app("text"), Method::Source),
            (app("viewer"), Method::Aur),
            (app("monitor"), Method::Source),
        ]);
        let ids = |v: &[&MaterialApp]| v.iter().map(|a| a.id).collect::<Vec<_>>();
        assert_eq!(ids(&b.aur), vec!["music", "viewer"]);
        assert_eq!(ids(&b.release), vec!["games"]);
        assert_eq!(ids(&b.source), vec!["text", "monitor"]);
        let empty = batch(&[]);
        assert!(empty.aur.is_empty() && empty.source.is_empty() && empty.release.is_empty());
    }
}
