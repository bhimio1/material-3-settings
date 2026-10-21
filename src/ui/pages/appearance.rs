//! Appearance: the wallpaper and its colors (through the end-4 shell), GTK
//! theme, icons, cursor, fonts and the light/dark preference, written to
//! GSettings and GTK's settings.ini.

use crate::app::App;
use material_3_ui::theme::ColorScheme;
use crate::end4::{self, Look, WallChange};
use crate::sys::gtk::{self, Appearance, Choices, Field};
use material_3_ui::sys::task::Task;
use material_3_ui::sys::paths;
use material_3_ui::thumbs::{self, Thumbnails};
use material_3_ui::common::{self, card, row};
use material_3_ui::widgets::{self, ButtonKind};
use crate::ui::{Action, Actions, Page, ToastKind};
use egui::{pos2, vec2, CornerRadius, FontFamily, FontId, Frame, Margin, Rect, RichText, Sense, Ui};
use material_3_ui::icons::icons::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

const CURSOR_SIZES: [i32; 6] = [16, 24, 32, 40, 48, 64];
const PREVIEW_TEXT: &str = "The quick brown fox jumps over the lazy dog 0123456789";

enum Preview {
    Loading,
    /// Family name and the frame it was registered on (usable after it).
    Ready(FontFamily, u64),
    Unavailable,
}

/// A family and its font file contents (bytes, face index), if found.
type FontLoad = (String, Option<(Vec<u8>, u32)>);

/// Accent colors offered when the colors don't come from the wallpaper.
pub const ACCENTS: &[&str] = &[
    "#4285F4", "#6750A4", "#B3261E", "#E8710A", "#1E8E3E", "#00897B", "#C2185B", "#795548",
    "#607D8B",
];

/// Wallpapers shown as thumbnails (the rest are a "Choose…" away).
const MAX_WALLPAPERS: usize = 9;

/// What the wallpaper card shows, read in the background.
#[derive(Clone, Debug, Default)]
struct Walls {
    /// The shell's `switchwall.sh`; without it there's no wallpaper card.
    script: Option<PathBuf>,
    look: Look,
    choices: Vec<PathBuf>,
    /// When each choice was last modified (for the thumbnail cache).
    modified: HashMap<PathBuf, SystemTime>,
}

fn read_walls() -> Walls {
    let home = paths::home();
    let look = end4::read_look(&home);
    let default = end4::shell_file(&home, "assets/images/default_wallpaper.png");
    let choices = end4::wallpaper_choices(look.wallpaper.as_deref(), default.as_deref(), &end4::wallpapers_dir(), MAX_WALLPAPERS);
    let modified = choices
        .iter()
        .filter_map(|p| Some((p.clone(), std::fs::metadata(p).and_then(|m| m.modified()).ok()?)))
        .collect();
    Walls { script: end4::switchwall(&home), look, choices, modified }
}

#[derive(Default)]
pub struct State {
    data: Task<Appearance>,
    choices: Task<Choices>,
    op: Task<Result<(), String>>,
    /// Edited copy shown in the controls.
    local: Option<Appearance>,
    previews: HashMap<String, Preview>,
    preview_task: Task<FontLoad>,
    walls: Task<Walls>,
    /// Wallpaper thumbnails, decoded in the background (made on first use).
    thumbs: Option<Thumbnails>,
    /// A `switchwall.sh` run, and what it's doing.
    wall_op: Task<Result<(), String>>,
    wall_op_label: &'static str,
    /// Chosen here; shown as selected until config.json says otherwise.
    picked: Option<PathBuf>,
    /// The accent chosen here (`Some(None)` = back to the wallpaper).
    picked_accent: Option<Option<String>>,
}

/// A combo box with a filter field, for long lists (fonts, themes).
fn search_combo(ui: &mut Ui, id: &str, value: &mut String, options: &[String], width: f32) -> bool {
    let mut changed = false;
    let filter_id = egui::Id::unique((id, "filter"));
    let options = with_current(options, value);
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .height(340.0)
        .selected_text(if value.is_empty() { "—".to_string() } else { value.clone() })
        .show_ui(ui, |ui| {
            let mut filter: String = ui.data_mut(|d| d.get_temp(filter_id).unwrap_or_default());
            let r = ui.add(
                egui::TextEdit::singleline(&mut filter)
                    .hint_text("Search…")
                    .desired_width(f32::INFINITY),
            );
            if !r.has_focus() && filter.is_empty() {
                r.request_focus();
            }
            ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
            ui.separator();
            let f = filter.to_lowercase();
            let mut shown = 0;
            for opt in options.iter().filter(|o| o.to_lowercase().contains(&f)) {
                shown += 1;
                if shown > 400 {
                    ui.label(RichText::new("Keep typing to narrow the list…").weak());
                    break;
                }
                if ui.selectable_label(value == opt, opt).clicked() && value != opt {
                    *value = opt.clone();
                    changed = true;
                    ui.data_mut(|d| d.remove::<String>(filter_id));
                    ui.close();
                }
            }
            if shown == 0 {
                ui.label(RichText::new("No matches").weak());
            }
        });
    changed
}

fn with_current(list: &[String], current: &str) -> Vec<String> {
    let mut v = list.to_vec();
    if !current.is_empty() && !v.iter().any(|x| x == current) {
        v.insert(0, current.to_string());
    }
    v
}

fn apply(st: &mut State, ctx: &egui::Context, field: Field, local: &Appearance) {
    let value = local.get(field);
    let cursor = matches!(field, Field::CursorTheme | Field::CursorSize)
        .then(|| (local.cursor_theme.clone(), local.cursor_size));
    // On end-4, light/dark also recolors the shell and everything themed
    // from the wallpaper.
    let shell_mode = match value.as_str() {
        "prefer-dark" => Some(true),
        "prefer-light" => Some(false),
        _ => None,
    }
    .filter(|_| field == Field::ColorScheme && shell_colors(st));
    st.op.spawn(ctx, move || {
        let shell = match shell_mode {
            Some(dark) => end4::run_switchwall(&paths::home(), &WallChange::Mode { dark }),
            None => Ok(()),
        };
        let gtk = gtk::apply(field, &value, cursor.as_ref().map(|(t, s)| (t.as_str(), *s)));
        shell.and(gtk)
    });
}

/// The end-4 shell's color scripts are installed.
fn shell_colors(st: &State) -> bool {
    st.walls.value.as_ref().is_some_and(|w| w.script.is_some())
}

/// For the welcome: whether the wallpaper controls show (once known).
pub fn has_shell_colors(app: &App) -> bool {
    shell_colors(&app.pages.appearance)
}

/// Load font files for previews in the background, one at a time.
fn pump_previews(st: &mut State, ctx: &egui::Context, wanted: &[String]) {
    if st.preview_task.poll() {
        if let Some((family, data)) = st.preview_task.value.take() {
            let preview = match data {
                Some((bytes, index)) => {
                    let name = format!("m3s-preview:{family}");
                    let fam = FontFamily::Name(name.clone().into());
                    let mut fd = egui::FontData::from_owned(bytes);
                    fd.index = index;
                    ctx.add_font(egui::epaint::text::FontInsert::new(
                        &name,
                        fd,
                        vec![egui::epaint::text::InsertFontFamily {
                            family: fam.clone(),
                            priority: egui::epaint::text::FontPriority::Highest,
                        }],
                    ));
                    Preview::Ready(fam, ctx.cumulative_frame_nr())
                }
                None => Preview::Unavailable,
            };
            st.previews.insert(family, preview);
        }
    }
    if st.preview_task.running() {
        return;
    }
    if let Some(family) = wanted.iter().find(|f| !st.previews.contains_key(*f)).cloned() {
        st.previews.insert(family.clone(), Preview::Loading);
        st.preview_task.spawn(ctx, move || {
            let data = gtk::font_file(&family)
                .and_then(|(path, index)| std::fs::read(path).ok().map(|b| (b, index)));
            (family, data)
        });
    }
}

fn preview_line(ui: &mut Ui, st: &State, label: &str, family: &str, size: f32) {
    let p = widgets::palette(ui);
    let font = match st.previews.get(family) {
        Some(Preview::Ready(fam, frame)) if ui.ctx().cumulative_frame_nr() > *frame => {
            Some(FontId::new(size * 1.3, fam.clone()))
        }
        _ => None,
    };
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(92.0, 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(92.0);
                ui.label(RichText::new(label).size(12.0).color(p.on_surface_variant));
            },
        );
        match font {
            Some(font) => {
                ui.add(egui::Label::new(RichText::new(PREVIEW_TEXT).font(font).color(p.on_surface)).truncate());
            }
            None => {
                let text = match st.previews.get(family) {
                    Some(Preview::Unavailable) => format!("{family} (no preview available)"),
                    _ => format!("{family}…"),
                };
                ui.label(RichText::new(text).italics().color(p.on_surface_variant));
            }
        }
    });
}

fn font_row(ui: &mut Ui, id: &str, title: &str, desc: &str, value: &mut String, fonts: &[String]) -> bool {
    let (mut family, mut size) = gtk::split_font(value);
    let mut changed = false;
    row(ui, title, desc, |ui| {
        let r = ui.add(
            egui::DragValue::new(&mut size)
                .range(6.0..=36.0)
                .speed(0.1)
                .fixed_decimals(1)
                .suffix(" pt"),
        );
        if r.drag_stopped() || (r.changed() && !r.dragged()) {
            changed = true;
        }
        changed |= search_combo(ui, id, &mut family, fonts, 200.0);
    });
    if changed {
        *value = gtk::join_font(&family, size);
    }
    changed
}

/// Poll background work and keep the edited copy fresh. Shared with the
/// welcome flow.
fn sync(st: &mut State, ctx: &egui::Context, actions: &mut Actions) {
    st.data.ensure(ctx, gtk::read);
    st.choices.ensure(ctx, gtk::choices);
    st.walls.ensure(ctx, read_walls);
    // Pick up wallpaper changes made elsewhere (the shell's own selector, or
    // the shell setting its default at first login).
    if !st.walls.running() && !st.wall_op.running() && st.walls.finished_at.is_some_and(|t| t.elapsed() > std::time::Duration::from_secs(3)) {
        st.walls.spawn(ctx, read_walls);
    }
    if st.walls.finished_at.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_secs(3));
    }
    if let Some(result) = st.op.take() {
        if let Err(e) = result {
            actions.push(Action::Toast(format!("Couldn't apply: {e}"), ToastKind::Error));
        }
        st.data.spawn(ctx, gtk::read);
    }
    if let Some(result) = st.wall_op.take() {
        if let Err(e) = result {
            actions.push(Action::Toast(format!("Couldn't change the wallpaper: {e}"), ToastKind::Error));
            st.picked = None;
            st.picked_accent = None;
        }
        st.walls.spawn(ctx, read_walls);
        st.data.spawn(ctx, gtk::read);
    }
    // Choices made here stand until config.json agrees with them.
    if let Some(look) = st.walls.value.as_ref().map(|w| &w.look) {
        if st.picked.is_some() && st.picked == look.wallpaper {
            st.picked = None;
        }
        if st.picked_accent.as_ref().is_some_and(|a| *a == look.accent) {
            st.picked_accent = None;
        }
    }
    // Take fresh values unless an edit is still being applied.
    if !st.op.running() && !st.data.running() {
        if let Some(d) = &st.data.value {
            if st.local.as_ref() != Some(d) {
                st.local = Some(d.clone());
            }
        }
    }
}

/// The light/dark row. Returns true when the value changed.
fn scheme_row(ui: &mut Ui, local: &mut Appearance, shell: bool) -> bool {
    let mut changed = false;
    let desc = if shell {
        "The shell, its widgets and apps that support it switch between light and dark."
    } else {
        "Apps that support it switch between light and dark."
    };
    row(ui, "Color scheme", desc, |ui| {
        let mut scheme = match local.color_scheme.as_str() {
            "prefer-dark" => 2u8,
            "prefer-light" => 1,
            _ => 0,
        };
        if widgets::segmented(ui, &mut scheme, &[
            (0, ICON_BRIGHTNESS_AUTO, "Default"),
            (1, ICON_LIGHT_MODE, "Light"),
            (2, ICON_DARK_MODE, "Dark"),
        ]) {
            local.color_scheme = ["default", "prefer-light", "prefer-dark"][scheme as usize].into();
            changed = true;
        }
    });
    changed
}

fn run_wall_op(st: &mut State, ctx: &egui::Context, label: &'static str, change: WallChange) {
    st.wall_op_label = label;
    st.wall_op.spawn(ctx, move || end4::run_switchwall(&paths::home(), &change));
}

/// Fill `rect` with `tex`, cropped to its aspect ratio like CSS `cover`.
fn paint_cover(ui: &Ui, tex: &egui::TextureHandle, rect: Rect, radius: u8) {
    let [w, h] = tex.size();
    let (img, target) = (w as f32 / h.max(1) as f32, rect.width() / rect.height());
    let uv = if img > target {
        let cut = (1.0 - target / img) / 2.0;
        Rect::from_min_max(pos2(cut, 0.0), pos2(1.0 - cut, 1.0))
    } else {
        let cut = (1.0 - img / target) / 2.0;
        Rect::from_min_max(pos2(0.0, cut), pos2(1.0, 1.0 - cut))
    };
    egui::Image::new((tex.id(), rect.size()))
        .uv(uv)
        .corner_radius(CornerRadius::same(radius))
        .paint_at(ui, rect);
}

/// Wallpaper thumbnails, a "Choose…" tile, and where the colors come from.
fn wallpaper_body(ui: &mut Ui, st: &mut State, ctx: &egui::Context) {
    let p = widgets::palette(ui);
    let Some(walls) = st.walls.value.clone() else {
        common::loading(ui, "Looking for wallpapers…");
        return;
    };
    let busy = st.wall_op.running();
    let current = st.picked.clone().or(walls.look.wallpaper.clone());

    ui.add_space(12.0);
    let width = ui.available_width();
    let cols = if width >= 700.0 { 5 } else if width >= 520.0 { 4 } else { 3 };
    let gap = 10.0;
    let tile_w = (width - gap * (cols as f32 - 1.0)) / cols as f32;
    let tile_h = (tile_w * 9.0 / 16.0).round();
    let count = walls.choices.len() + 1;
    let rows = count.div_ceil(cols);
    let (area, _) = ui.allocate_exact_size(vec2(width, rows as f32 * (tile_h + gap) - gap), Sense::hover());
    let mut chosen = None;
    let mut pick = false;
    for i in 0..count {
        let (r, c) = (i / cols, i % cols);
        let rect = Rect::from_min_size(pos2(area.left() + c as f32 * (tile_w + gap), area.top() + r as f32 * (tile_h + gap)), vec2(tile_w, tile_h));
        let id = ui.make_persistent_id(("wallpaper", i));
        let resp = ui.interact(rect, id, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let Some(path) = walls.choices.get(i) else {
            // The last tile asks the shell's script for a file.
            ui.painter().rect_filled(rect, CornerRadius::same(12), widgets::state_layer(p.surface_container_highest, p.on_surface, &resp));
            widgets::paint_icon(ui, ICON_ADD_PHOTO_ALTERNATE, rect.center() - vec2(0.0, 10.0), 26.0, p.on_surface_variant);
            ui.painter().text(rect.center() + vec2(0.0, 16.0), egui::Align2::CENTER_CENTER, "Choose…", widgets::title_font(13.0), p.on_surface_variant);
            if resp.on_hover_text("Pick any image on your computer").clicked() && !busy {
                pick = true;
            }
            continue;
        };
        let mtime = walls.modified.get(path).copied().unwrap_or(SystemTime::UNIX_EPOCH);
        let thumbs = st.thumbs.get_or_insert_with(|| Thumbnails::new(ctx, |p| thumbs::load_image(p, 480)));
        let tex = thumbs.get(path, mtime);
        match tex {
            Some(tex) => paint_cover(ui, &tex, rect, 12),
            None if !thumbs.loading(path) => {
                ui.painter().rect_filled(rect, CornerRadius::same(12), p.surface_container_highest);
                widgets::paint_icon(ui, ICON_BROKEN_IMAGE, rect.center(), 24.0, p.on_surface_variant);
            }
            None => {
                ui.painter().rect_filled(rect, CornerRadius::same(12), p.surface_container_highest);
                let spot = Rect::from_center_size(rect.center(), vec2(20.0, 20.0));
                ui.new_child(egui::UiBuilder::new().max_rect(spot)).add(egui::Spinner::new().color(p.primary));
            }
        }
        if resp.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(12), p.on_surface.gamma_multiply(0.08));
        }
        if current.as_ref() == Some(path) {
            ui.painter().rect_stroke(rect.expand(1.0), CornerRadius::same(13), egui::Stroke::new(3.0, p.primary), egui::StrokeKind::Outside);
            let badge = pos2(rect.right() - 16.0, rect.top() + 16.0);
            ui.painter().circle_filled(badge, 11.0, p.primary);
            widgets::paint_icon(ui, ICON_CHECK, badge, 16.0, p.on_primary);
        }
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if resp.on_hover_text(name).clicked() && !busy && current.as_ref() != Some(path) {
            chosen = Some(path.clone());
        }
    }
    if let Some(path) = chosen {
        st.picked = Some(path.clone());
        run_wall_op(st, ctx, "Setting the wallpaper and its colors…", WallChange::Image(path));
    } else if pick {
        run_wall_op(st, ctx, "Waiting for a file…", WallChange::Pick);
    }
    ui.add_space(8.0);
    ui.label(RichText::new(format!("From {}, plus the shell's default.", paths::tilde(&end4::wallpapers_dir()))).size(12.0).color(p.on_surface_variant));

    ui.add_space(4.0);
    ui.separator();
    let accent = st.picked_accent.clone().unwrap_or(walls.look.accent.clone());
    let mut from_wallpaper = accent.is_none();
    row(ui, "Colors", if from_wallpaper { "Taken from the wallpaper, for the shell and every app themed with it." } else { "Built around one accent color instead of the wallpaper." }, |ui| {
        if widgets::segmented(ui, &mut from_wallpaper, &[
            (true, ICON_WALLPAPER, "Wallpaper"),
            (false, ICON_PALETTE, "Accent"),
        ]) && !busy {
            let next = if from_wallpaper { None } else { Some(ACCENTS[1].to_string()) };
            st.picked_accent = Some(next.clone());
            run_wall_op(st, ctx, "Updating colors…", WallChange::Accent(next));
        }
    });
    if let Some(hex) = &accent {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for preset in ACCENTS {
                let c = material_3_ui::theme::palette::parse_hex(preset).expect("valid preset");
                let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
                ui.painter().circle_filled(rect.center(), 13.0, c);
                if hex.eq_ignore_ascii_case(preset) {
                    ui.painter().circle_stroke(rect.center(), 14.5, egui::Stroke::new(2.0, p.on_surface));
                }
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !busy && !hex.eq_ignore_ascii_case(preset) {
                    st.picked_accent = Some(Some(preset.to_string()));
                    run_wall_op(st, ctx, "Updating colors…", WallChange::Accent(Some(preset.to_string())));
                }
            }
        });
        ui.add_space(8.0);
    }
    if busy {
        common::loading(ui, st.wall_op_label);
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
}

/// Wallpaper, light/dark and colors in one card, for the welcome flow. On
/// a desktop without the end-4 shell, light/dark and this app's own colors.
pub fn show_look(ui: &mut Ui, app: &mut App) {
    let ctx = app.ctx.clone();
    let theme_file = app.theme_file();
    let st = &mut app.pages.appearance;
    let actions = &mut app.actions;
    sync(st, &ctx, actions);
    if st.walls.value.is_none() {
        common::loading(ui, "Reading current settings…");
        return;
    }
    let shell = shell_colors(st);
    let mut changed = false;
    card(ui, if shell { "Wallpaper" } else { "Style" }, |ui| {
        if shell {
            wallpaper_body(ui, st, &ctx);
            ui.separator();
        }
        match st.local.as_mut() {
            Some(local) => changed = scheme_row(ui, local, shell),
            None => common::loading(ui, "Reading current settings…"),
        }
        if !shell {
            ui.separator();
            if super::prefs::color_rows(ui, &mut app.config, &mut app.pages.prefs, theme_file.as_ref(), false) {
                actions.push(Action::ConfigChanged);
            }
        }
    });
    if changed {
        if let Some(local) = st.local.clone() {
            apply(st, &ctx, Field::ColorScheme, &local);
            // Without the shell's colors, this app follows the choice too.
            if !shell {
                app.config.appearance.scheme = match local.color_scheme.as_str() {
                    "prefer-dark" => ColorScheme::Dark,
                    "prefer-light" => ColorScheme::Light,
                    _ => ColorScheme::System,
                };
                actions.push(Action::ConfigChanged);
            }
        }
    }
    if st.op.running() {
        common::loading(ui, "Applying…");
    }
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let ctx = ui.ctx().clone();
    let installed = app.installed.clone();
    let st = &mut app.pages.appearance;
    let actions = &mut app.actions;
    sync(st, &ctx, actions);
    let shell = shell_colors(st);

    common::page(
        ui,
        ICON_PALETTE,
        "Appearance",
        "Wallpaper, themes, icons, cursor and fonts. GTK settings are saved to GSettings and ~/.config/gtk-3.0 / gtk-4.0/settings.ini.",
        |ui| {
            crate::ui::require(
                ui,
                actions,
                &installed,
                "gsettings",
                "glib2",
                "Without it, only settings.ini is updated and GTK 4 apps may ignore the change.",
            );
            if shell {
                card(ui, "Wallpaper", |ui| wallpaper_body(ui, st, &ctx));
            }
            let Some(mut local) = st.local.clone() else {
                common::loading(ui, "Reading current settings…");
                return;
            };
            let choices = st.choices.value.clone().unwrap_or_default();
            let loading_lists = st.choices.running();
            let mut changed: Option<Field> = None;

            card(ui, "Style", |ui| {
                if scheme_row(ui, &mut local, shell) {
                    changed = Some(Field::ColorScheme);
                }
                ui.separator();
                row(ui, "GTK theme", "Widget style for GTK 3 apps (GTK 4/libadwaita apps mostly follow the color scheme).", |ui| {
                    if loading_lists {
                        ui.add(egui::Spinner::new());
                    }
                    if search_combo(ui, "gtk-theme", &mut local.gtk_theme, &choices.gtk_themes, 220.0) {
                        changed = Some(Field::GtkTheme);
                    }
                });
                ui.separator();
                row(ui, "Icon theme", "Application and folder icons.", |ui| {
                    if search_combo(ui, "icon-theme", &mut local.icon_theme, &choices.icon_themes, 220.0) {
                        changed = Some(Field::IconTheme);
                    }
                });
            });

            card(ui, "Cursor", |ui| {
                row(ui, "Cursor theme", "Also applied to Hyprland right away.", |ui| {
                    if search_combo(ui, "cursor-theme", &mut local.cursor_theme, &choices.cursor_themes, 220.0) {
                        changed = Some(Field::CursorTheme);
                    }
                });
                ui.separator();
                row(ui, "Cursor size", "In pixels at 1× scale.", |ui| {
                    let mut size = local.cursor_size;
                    egui::ComboBox::from_id_salt("cursor-size")
                        .width(90.0)
                        .selected_text(format!("{size} px"))
                        .show_ui(ui, |ui| {
                            for s in CURSOR_SIZES {
                                ui.selectable_value(&mut size, s, format!("{s} px"));
                            }
                        });
                    if size != local.cursor_size {
                        local.cursor_size = size;
                        changed = Some(Field::CursorSize);
                    }
                });
            });

            card(ui, "Fonts", |ui| {
                if font_row(ui, "ui-font", "Interface font", "Menus, buttons and window text.", &mut local.font, &choices.fonts) {
                    changed = Some(Field::Font);
                }
                ui.separator();
                if font_row(ui, "mono-font", "Monospace font", "Terminals and code in GTK apps.", &mut local.monospace_font, &choices.fonts) {
                    changed = Some(Field::MonospaceFont);
                }
                ui.separator();
                let p = widgets::palette(ui);
                ui.add_space(8.0);
                Frame::new()
                    .fill(p.surface_container_highest)
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let (ui_family, ui_size) = gtk::split_font(&local.font);
                        let (mono_family, mono_size) = gtk::split_font(&local.monospace_font);
                        preview_line(ui, st, "Interface", &ui_family, ui_size);
                        ui.add_space(4.0);
                        preview_line(ui, st, "Monospace", &mono_family, mono_size);
                        pump_previews(st, &ctx, &[ui_family, mono_family]);
                    });
                ui.add_space(8.0);
            });

            card(ui, "Qt apps", |ui| {
                row(ui, "Qt and KDE apps", "Qt apps use their own settings: qt6ct / qt5ct for fonts and icons, Kvantum for the widget style.", |ui| {
                    if widgets::button(ui, ButtonKind::Tonal, Some(ICON_TUNE), "Kvantum").clicked() {
                        actions.push(Action::Goto(Page::App("kvantum".into())));
                    }
                    if widgets::button(ui, ButtonKind::Tonal, Some(ICON_TUNE), "qt6ct").clicked() {
                        actions.push(Action::Goto(Page::App("qt6ct".into())));
                    }
                });
            });

            if st.op.running() {
                common::loading(ui, "Applying…");
            }
            if let Some(field) = changed {
                apply(st, &ctx, field, &local);
            }
            st.local = Some(local);
        },
    );
}
