//! Appearance: GTK theme, icons, cursor, fonts and the light/dark
//! preference, written to GSettings and GTK's settings.ini.

use crate::app::App;
use crate::sys::gtk::{self, Appearance, Choices, Field};
use crate::sys::task::Task;
use crate::ui::common::{self, card, row};
use crate::ui::widgets::{self, ButtonKind};
use crate::ui::{Action, Page, ToastKind};
use egui::{CornerRadius, FontFamily, FontId, Frame, Margin, RichText, Ui};
use egui_material_icons::icons::*;
use std::collections::HashMap;

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

#[derive(Default)]
pub struct State {
    data: Task<Appearance>,
    choices: Task<Choices>,
    op: Task<Result<(), String>>,
    /// Edited copy shown in the controls.
    local: Option<Appearance>,
    previews: HashMap<String, Preview>,
    preview_task: Task<FontLoad>,
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
    st.op.spawn(ctx, move || {
        gtk::apply(field, &value, cursor.as_ref().map(|(t, s)| (t.as_str(), *s)))
    });
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

pub fn show(ui: &mut Ui, app: &mut App) {
    let ctx = ui.ctx().clone();
    let installed = app.installed.clone();
    let st = &mut app.pages.appearance;
    let actions = &mut app.actions;

    st.data.ensure(&ctx, gtk::read);
    st.choices.ensure(&ctx, gtk::choices);
    if let Some(result) = st.op.take() {
        if let Err(e) = result {
            actions.push(Action::Toast(format!("Couldn't apply: {e}"), ToastKind::Error));
        }
        st.data.spawn(&ctx, gtk::read);
    }
    // Take fresh values unless an edit is still being applied.
    if !st.op.running() && !st.data.running() {
        if let Some(d) = &st.data.value {
            if st.local.as_ref() != Some(d) {
                st.local = Some(d.clone());
            }
        }
    }

    common::page(
        ui,
        ICON_PALETTE,
        "Appearance",
        "Themes, icons, cursor and fonts for GTK apps. Saved to GSettings and ~/.config/gtk-3.0 / gtk-4.0/settings.ini.",
        |ui| {
            common::require(
                ui,
                actions,
                &installed,
                "gsettings",
                "glib2",
                "Without it, only settings.ini is updated and GTK 4 apps may ignore the change.",
            );
            let Some(mut local) = st.local.clone() else {
                common::loading(ui, "Reading current settings…");
                return;
            };
            let choices = st.choices.value.clone().unwrap_or_default();
            let loading_lists = st.choices.running();
            let mut changed: Option<Field> = None;

            card(ui, "Style", |ui| {
                row(ui, "Color scheme", "Apps that support it switch between light and dark.", |ui| {
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
                        changed = Some(Field::ColorScheme);
                    }
                });
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
