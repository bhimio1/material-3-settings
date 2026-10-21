//! The first-run welcome (`m3-settings --welcome`): a full-window flow of
//! steps with Back/Next and a progress indicator, in place of the sidebar
//! and pages. Each step reuses a page's own controls.

use crate::app::App;
use crate::hypr::{self, Bind};
use crate::material_apps::{MaterialApp, APPS};
use crate::sys::{cmd, paths, task::Task};
use crate::ui::common::{self, card};
use crate::ui::pages::{appearance, default_apps, material_apps};
use crate::ui::widgets::{self, ButtonKind};
use crate::ui::{Action, ToastKind};
use crate::welcome::{self, Flow, KeyHint, Step};
use egui::{pos2, vec2, Align2, CornerRadius, Rect, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder};
use egui_material_icons::{icons::*, MaterialIcon};

/// Widest the step content gets.
const COLUMN: f32 = 760.0;
const HEADER_H: f32 = 108.0;
const FOOTER_H: f32 = 76.0;

/// Default apps worth choosing on day one.
const DEFAULTS: &[&str] = &["browser", "files", "terminal", "text", "image", "video"];

#[derive(Default)]
pub struct State {
    pub flow: Flow,
    binds: Task<Vec<Bind>>,
    /// end4-pC is installed (`None` until checked).
    end4pc: Option<bool>,
}

fn step_icon(step: Step) -> MaterialIcon {
    match step {
        Step::Hello => ICON_WAVING_HAND,
        Step::Look => ICON_PALETTE,
        Step::Apps => ICON_WIDGETS,
        Step::Defaults => ICON_APPS,
        Step::Keys => ICON_KEYBOARD,
        Step::Done => ICON_TASK_ALT,
    }
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let Some(mut st) = app.welcome.take() else { return };
    let ctx = app.ctx.clone();
    // Start the release check early so install methods are known by the
    // Apps step.
    material_apps::poll(app);
    st.binds.ensure(&ctx, || if hypr::running() { hypr::binds().unwrap_or_default() } else { Vec::new() });
    let end4pc = *st.end4pc.get_or_insert_with(|| paths::config_home().join("quickshell/end4-pC").is_dir());

    let full = ui.max_rect();
    let header = Rect::from_min_size(full.min, vec2(full.width(), HEADER_H));
    let footer = Rect::from_min_max(pos2(full.left(), full.bottom() - FOOTER_H), full.max);
    let body = Rect::from_min_max(pos2(full.left(), header.bottom()), pos2(full.right(), footer.top()));
    ui.allocate_rect(full, Sense::hover());

    header_bar(&mut ui.new_child(UiBuilder::new().max_rect(header)), &mut st.flow);

    let step = st.flow.step;
    let mut body_ui = ui.new_child(UiBuilder::new().max_rect(body));
    egui::ScrollArea::vertical()
        .id_salt(("welcome", step.index()))
        .auto_shrink([false, false])
        .show(&mut body_ui, |ui| {
            column(ui, |ui| match step {
                Step::Hello => hello(ui, app, end4pc),
                Step::Look => look(ui, app),
                Step::Apps => apps(ui, app, &mut st.flow),
                Step::Defaults => defaults(ui, app),
                Step::Keys => keys(ui, app, st.binds.value.as_deref().unwrap_or_default()),
                Step::Done => done(ui, app, &mut st.flow),
            });
        });

    footer_bar(&mut ui.new_child(UiBuilder::new().max_rect(footer)), &mut st.flow);
    app.welcome = Some(st);
}

/// A centered column with page-like margins.
fn column(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let w = ui.available_width();
    let col = COLUMN.min(w - 64.0).max(200.0);
    ui.horizontal_top(|ui| {
        ui.add_space(((w - col) / 2.0).max(0.0));
        ui.vertical(|ui| {
            ui.set_width(col);
            ui.add_space(8.0);
            add(ui);
            ui.add_space(24.0);
        });
    });
}

/// Step heading: icon, title and a line of explanation.
fn heading(ui: &mut Ui, step: Step, title: &str, subtitle: &str) {
    let p = widgets::palette(ui);
    ui.horizontal(|ui| {
        widgets::icon(ui, step_icon(step), 30.0, p.primary);
        ui.label(RichText::new(title).size(28.0).color(p.on_surface));
    });
    ui.add(egui::Label::new(RichText::new(subtitle).color(p.on_surface_variant)).wrap());
}

// ------------------------------------------------------------ header / footer

fn header_bar(ui: &mut Ui, flow: &mut Flow) {
    let p = widgets::palette(ui);
    let rect = ui.max_rect();
    // Title on the left, "Skip setup" on the right.
    let top = Rect::from_min_size(rect.min + vec2(24.0, 14.0), vec2(rect.width() - 48.0, 36.0));
    let mut row = ui.new_child(UiBuilder::new().max_rect(top).layout(egui::Layout::left_to_right(egui::Align::Center)));
    widgets::icon(&mut row, ICON_SETTINGS, 20.0, p.primary);
    row.label(widgets::title_text("Welcome", 15.0).color(p.on_surface));
    row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if flow.step != Step::Done && widgets::button(ui, ButtonKind::Text, Some(ICON_SKIP_NEXT), "Skip setup").clicked() {
            flow.skip_all();
        }
    });

    // The stepper: a numbered circle per step, joined by lines.
    let w = 640.0f32.min(rect.width() - 64.0);
    let left = rect.center().x - w / 2.0;
    let y = rect.top() + 66.0;
    let n = Step::ALL.len();
    let gap = w / (n - 1) as f32;
    let current = flow.step.index();
    for i in 0..n - 1 {
        let a = pos2(left + i as f32 * gap + 16.0, y);
        let b = pos2(left + (i + 1) as f32 * gap - 16.0, y);
        let color = if i < current { p.primary } else { p.outline_variant };
        ui.painter().line_segment([a, b], Stroke::new(2.0, color));
    }
    for (i, step) in Step::ALL.iter().enumerate() {
        let c = pos2(left + i as f32 * gap, y);
        let hit = Rect::from_center_size(c + vec2(0.0, 10.0), vec2(gap.min(72.0), 44.0));
        // Steps already passed can be revisited.
        let resp = ui.interact(hit, ui.make_persistent_id(("welcome-step", i)), Sense::click());
        let clickable = i < current;
        if clickable && resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if clickable && resp.clicked() {
            flow.step = *step;
        }
        let (fill, fg) = if i <= current { (p.primary, p.on_primary) } else { (p.surface_container_highest, p.on_surface_variant) };
        ui.painter().circle_filled(c, 13.0, fill);
        if i < current {
            widgets::paint_icon(ui, ICON_CHECK, c, 16.0, fg);
        } else {
            ui.painter().text(c, Align2::CENTER_CENTER, (i + 1).to_string(), widgets::title_font(12.5), fg);
        }
        let label_color = if i == current { p.on_surface } else { p.on_surface_variant };
        ui.painter().text(c + vec2(0.0, 26.0), Align2::CENTER_CENTER, step.title(), widgets::title_font(12.0), label_color);
    }
}

fn footer_bar(ui: &mut Ui, flow: &mut Flow) {
    let p = widgets::palette(ui);
    let rect = ui.max_rect();
    let w = (COLUMN + 32.0).min(rect.width() - 32.0);
    let inner = Rect::from_center_size(rect.center(), vec2(w, 40.0));
    ui.painter().line_segment(
        [pos2(rect.left() + 24.0, rect.top()), pos2(rect.right() - 24.0, rect.top())],
        Stroke::new(1.0, p.outline_variant.gamma_multiply(0.6)),
    );
    let mut row = ui.new_child(UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)));
    if flow.can_go_back() && widgets::button(&mut row, ButtonKind::Text, Some(ICON_ARROW_BACK), "Back").clicked() {
        flow.back();
    }
    let step_text = format!("Step {} of {}", flow.step.index() + 1, Step::ALL.len());
    row.painter().text(inner.center(), Align2::CENTER_CENTER, step_text, egui::FontId::proportional(12.5), p.on_surface_variant);
    row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        match flow.step {
            Step::Done => {}
            Step::Hello => {
                if widgets::button(ui, ButtonKind::Filled, Some(ICON_ARROW_FORWARD), "Get started").clicked() {
                    flow.next();
                }
            }
            _ => {
                if widgets::button(ui, ButtonKind::Filled, Some(ICON_ARROW_FORWARD), "Next").clicked() {
                    flow.next();
                }
            }
        }
    });
}

// ---------------------------------------------------------------------- steps

fn hero(ui: &mut Ui, icon: MaterialIcon, fill: egui::Color32, fg: egui::Color32, title: &str, text: &str) {
    let p = widgets::palette(ui);
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(96.0, 96.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 48.0, fill);
        widgets::paint_icon(ui, icon, rect.center(), 48.0, fg);
        ui.add_space(16.0);
        ui.label(RichText::new(title).size(32.0).color(p.on_surface));
        ui.add_space(6.0);
        let w = ui.available_width().min(600.0);
        ui.allocate_ui_with_layout(vec2(w, 0.0), egui::Layout::top_down(egui::Align::Center), |ui| {
            ui.set_width(w);
            ui.add(egui::Label::new(RichText::new(text).size(15.0).color(p.on_surface_variant)).wrap());
        });
    });
}

fn hello(ui: &mut Ui, app: &mut App, end4pc: bool) {
    let p = app.palette.clone();
    let text = if end4pc {
        "This is end4-pC: Hyprland with end-4's illogical-impulse shell, in Material You. Its colors come from your wallpaper, and so do those of the apps made for it."
    } else {
        "This is Material 3 Settings, the settings app for this desktop. A few quick steps set up the look, apps and shortcuts."
    };
    hero(ui, ICON_WAVING_HAND, p.primary_container, p.on_primary_container, "Welcome to your desktop", text);
    ui.add_space(28.0);

    let tiles: [(MaterialIcon, &str, &str, (egui::Color32, egui::Color32)); 3] = [
        (ICON_WALLPAPER, "Colors from your wallpaper", "Pick a wallpaper and the whole desktop follows it.", (p.primary_container, p.on_primary_container)),
        (ICON_WIDGETS, "Material 3 apps", "Music, games, a browser and more, made to match.", (p.secondary_container, p.on_secondary_container)),
        (ICON_KEYBOARD, "Made for the keyboard", "Super opens search.\nSuper + / lists every shortcut.", (p.tertiary_container, p.on_tertiary_container)),
    ];
    let w = ui.available_width();
    let gap = 12.0;
    let cols = if w >= 560.0 { 3 } else { 1 };
    let tile_w = (w - gap * (cols as f32 - 1.0)) / cols as f32;
    let tile_h = 132.0;
    let rows = tiles.len().div_ceil(cols);
    let (area, _) = ui.allocate_exact_size(vec2(w, rows as f32 * (tile_h + gap) - gap), Sense::hover());
    for (i, (icon, title, text, (circle, fg))) in tiles.iter().enumerate() {
        let (r, c) = (i / cols, i % cols);
        let rect = Rect::from_min_size(pos2(area.left() + c as f32 * (tile_w + gap), area.top() + r as f32 * (tile_h + gap)), vec2(tile_w, tile_h));
        ui.painter().rect_filled(rect, CornerRadius::same(20), p.surface_container);
        let ic = pos2(rect.left() + 36.0, rect.top() + 36.0);
        ui.painter().circle_filled(ic, 20.0, *circle);
        widgets::paint_icon(ui, *icon, ic, 22.0, *fg);
        let tw = rect.width() - 36.0;
        let t = widgets::truncated_galley(ui, title, widgets::title_font(15.0), p.on_surface, tw);
        ui.painter().galley(pos2(rect.left() + 18.0, rect.top() + 66.0), t, p.on_surface);
        let mut job = egui::text::LayoutJob::single_section(text.to_string(), egui::TextFormat::simple(egui::FontId::proportional(12.5), p.on_surface_variant));
        job.wrap = egui::text::TextWrapping { max_width: tw, max_rows: 2, break_anywhere: false, overflow_character: Some('…') };
        let g = ui.painter().layout_job(job);
        ui.painter().galley(pos2(rect.left() + 18.0, rect.top() + 88.0), g, p.on_surface_variant);
    }
    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("It takes about a minute. Every step can be skipped, and everything can be changed later in Settings.").size(13.0).color(p.on_surface_variant));
    });
}

fn look(ui: &mut Ui, app: &mut App) {
    let text = if appearance::has_shell_colors(app) {
        "Pick a wallpaper, light or dark, and where the colors come from. Changes apply right away."
    } else {
        "Light or dark, and where this app's colors come from. Changes apply right away."
    };
    heading(ui, Step::Look, "Make it yours", text);
    appearance::show_look(ui, app);
}

/// A small tonal pill (`Included`, `Installed`).
fn pill(ui: &mut Ui, icon: MaterialIcon, text: &str) -> egui::Response {
    let p = widgets::palette(ui);
    let galley = ui.painter().layout_no_wrap(text.to_string(), widgets::title_font(12.0), p.on_secondary_container);
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 36.0, 26.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(8), p.secondary_container);
    widgets::paint_icon(ui, icon, pos2(rect.left() + 16.0, rect.center().y), 15.0, p.on_secondary_container);
    ui.painter().galley(pos2(rect.left() + 28.0, rect.center().y - galley.size().y / 2.0), galley, p.on_secondary_container);
    resp
}

fn apps(ui: &mut Ui, app: &mut App, flow: &mut Flow) {
    let p = app.palette.clone();
    heading(ui, Step::Apps, "Material 3 apps", "Apps made for this desktop. They share its look and take their colors from your wallpaper. Tick the ones you want.");

    card(ui, "Included", |ui| {
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for a in APPS.iter().filter(|a| a.essential) {
                pill(ui, a.icon, a.name).on_hover_text(a.summary);
            }
        });
        common::note(ui, "The shell depends on these, so it installs and updates them itself.");
    });

    let optional: Vec<&'static MaterialApp> = APPS.iter().filter(|a| !a.essential).collect();
    let ready = material_apps::ready(app);
    // Ticks only count for apps that still need installing.
    flow.selected.retain(|id| optional.iter().any(|a| a.id == *id && !material_apps::is_installed(app, a)));
    let mut install = false;
    card(ui, "Optional", |ui| {
        for (i, a) in optional.iter().enumerate() {
            if i > 0 {
                ui.separator();
            }
            let installed = material_apps::is_installed(app, a);
            let job = material_apps::job(app, a);
            let selected = flow.selected.contains(a.id);
            let mut toggled = false;
            let resp = common::list_row(ui, a.icon, installed || selected, a.name, a.summary, |ui| {
                if let Some((stage, progress)) = &job {
                    ui.allocate_ui_with_layout(vec2(180.0, 34.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(180.0);
                        ui.add(egui::Label::new(RichText::new(stage).size(12.0).color(p.on_surface_variant)).truncate());
                        ui.add_space(6.0);
                        widgets::linear_progress(ui, *progress, 180.0);
                    });
                } else if installed {
                    pill(ui, ICON_CHECK_CIRCLE, "Installed");
                } else if widgets::checkbox(ui, selected).clicked() {
                    toggled = true;
                }
            });
            if !installed && job.is_none() && (toggled || resp.clicked()) {
                flow.toggle(a.id);
            }
        }
        ui.separator();
        let n = flow.selected.len();
        common::row(
            ui,
            if ready { "Install the ones you ticked" } else { "Checking what's available…" },
            "Each app is installed the best way available: its AUR package, the release download or a build from source. Downloads show their progress here; the others open in a terminal.",
            |ui| {
                let label = if n > 0 { format!("Install selected ({n})") } else { "Install selected".to_string() };
                if !ready {
                    ui.add(egui::Spinner::new().color(p.primary));
                }
                if widgets::button_enabled(ui, ButtonKind::Filled, Some(ICON_DOWNLOAD), &label, ready && n > 0).clicked() {
                    install = true;
                }
            },
        );
    });
    if install {
        let chosen: Vec<&'static MaterialApp> = optional.iter().filter(|a| flow.selected.contains(a.id)).copied().collect();
        flow.selected.clear();
        material_apps::install_many(app, &chosen);
        let n = chosen.len();
        app.actions.push(Action::Toast(format!("Installing {n} app{}", if n == 1 { "" } else { "s" }), ToastKind::Info));
    }
    ui.add_space(12.0);
    ui.label(RichText::new("Settings → Material 3 apps has every app's details, updates and removal.").size(12.5).color(p.on_surface_variant));
}

fn defaults(ui: &mut Ui, app: &mut App) {
    let p = app.palette.clone();
    heading(ui, Step::Defaults, "Default apps", "Which apps open links, folders and files. Pick from what's installed.");
    default_apps::compact(ui, app, DEFAULTS);
    ui.add_space(12.0);
    ui.label(RichText::new("Settings → Default apps has the rest: email, music, PDFs and archives.").size(12.5).color(p.on_surface_variant));
}

/// Keys drawn as keycaps: `Super` `+` `/`.
fn keycaps(ui: &mut Ui, keys: &str) {
    let p = widgets::palette(ui);
    for (i, key) in keys.split(" + ").enumerate() {
        if i > 0 {
            ui.label(RichText::new("+").size(12.0).color(p.on_surface_variant));
        }
        let galley = ui.painter().layout_no_wrap(key.to_string(), widgets::title_font(12.5), p.on_surface);
        let (rect, _) = ui.allocate_exact_size(vec2((galley.size().x + 16.0).max(28.0), 26.0), Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::same(6), p.surface_container_highest);
        ui.painter().rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, p.outline_variant), StrokeKind::Inside);
        ui.painter().galley(rect.center() - galley.size() / 2.0, galley, p.on_surface);
    }
}

fn keys(ui: &mut Ui, app: &mut App, live: &[Bind]) {
    let p = app.palette.clone();
    heading(ui, Step::Keys, "Keyboard shortcuts", "The ones you'll use every day. Super is the key with the Windows logo.");
    let sheet: Vec<KeyHint> = welcome::cheat_sheet(live);
    let from_live = welcome::live_count(live) > 0;
    card(ui, "", |ui| {
        let w = ui.available_width();
        let cols = if w >= 600.0 { 2 } else { 1 };
        let col_w = (w - 24.0 * (cols as f32 - 1.0)) / cols as f32;
        let rows = sheet.len().div_ceil(cols);
        ui.add_space(8.0);
        egui::Grid::new("welcome-keys").num_columns(cols).spacing([24.0, 0.0]).show(ui, |ui| {
            for r in 0..rows {
                for c in 0..cols {
                    // Fill down the first column, then the second.
                    let Some(hint) = sheet.get(c * rows + r) else { continue };
                    ui.allocate_ui_with_layout(vec2(col_w, 40.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.set_min_size(vec2(col_w, 40.0));
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(RichText::new(hint.label).size(13.5).color(p.on_surface));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Right-to-left: lay the keys out reversed.
                            let mut reversed: Vec<&str> = hint.keys.split(" + ").collect();
                            reversed.reverse();
                            keycaps(ui, &reversed.join(" + "));
                        });
                    });
                }
                ui.end_row();
            }
        });
        ui.add_space(8.0);
    });
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        let note = if from_live { "Read from your Hyprland keybinds." } else { "end-4's default keybinds. Yours may differ if you changed them." };
        ui.label(RichText::new(note).size(12.5).color(p.on_surface_variant));
        if hypr::running() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::button(ui, ButtonKind::Tonal, Some(ICON_KEYBOARD_ALT), "Show every shortcut").clicked() {
                    if let Err(e) = cmd::run("hyprctl", &["dispatch", "global", "quickshell:cheatsheetToggle"]) {
                        app.actions.push(Action::Toast(e, ToastKind::Error));
                    }
                }
            });
        }
    });
}

fn done(ui: &mut Ui, app: &mut App, flow: &mut Flow) {
    let p = app.palette.clone();
    let settings_keys = welcome::cheat_sheet(&[]).into_iter().find(|h| h.label == "Settings").map(|h| h.keys).unwrap_or_default();
    hero(
        ui,
        ICON_TASK_ALT,
        p.tertiary_container,
        p.on_tertiary_container,
        "You're all set",
        &format!("All of this is in Settings too, along with sound, network, displays and more.\nPress {settings_keys} to open it any time."),
    );
    ui.add_space(12.0);
    card(ui, "", |ui| {
        common::row(ui, "Show this at next login", "Greet you with these steps again the next time the shell starts.", |ui| {
            widgets::switch(ui, &mut flow.show_next_time);
        });
    });
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        // Center a fixed-width row of two buttons.
        let w = 300.0;
        ui.allocate_ui_with_layout(vec2(w, 40.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(w);
            if widgets::button(ui, ButtonKind::Tonal, Some(ICON_CLOSE), "Close").clicked() {
                app.actions.push(Action::FinishWelcome { open_settings: false });
            }
            ui.add_space(8.0);
            if widgets::button(ui, ButtonKind::Filled, Some(ICON_SETTINGS), "Open Settings").clicked() {
                app.actions.push(Action::FinishWelcome { open_settings: true });
            }
        });
    });
}
