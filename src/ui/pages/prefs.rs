//! Preferences for this app itself: theme, terminal, AUR helper, backups.

use crate::app::App;
use crate::config::{ColorScheme, Config, ThemeSource};
use crate::sys::arch::AurHelper;
use crate::sys::{cmd, paths, term};
use crate::theme::palette::{parse_hex, to_hex};
use crate::ui::common::{self, card, row};
use crate::ui::widgets::{self, segmented, switch, ButtonKind};
use crate::ui::Action;
use egui::{vec2, Color32, RichText, Ui};
use egui_material_icons::icons::*;

const SEED_PRESETS: &[&str] = &[
    "#4285F4", "#6750A4", "#B3261E", "#E8710A", "#1E8E3E", "#00897B", "#C2185B", "#795548",
    "#607D8B",
];

#[derive(Default)]
pub struct State {
    seed_text: Option<String>,
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let p = app.palette.clone();
    let theme_file = app.theme_file();
    let config = &mut app.config;
    let st = &mut app.pages.prefs;
    let mut actions = Vec::new();
    let mut changed = false;

    common::page(ui, ICON_SETTINGS, "Preferences", "How this app looks and behaves. Saved to config.toml.", |ui| {
        card(ui, "Appearance", |ui| {
            let source_desc = match (&config.theme_source, &theme_file) {
                (ThemeSource::Auto, Some(f)) => format!("Following {} (updates live with your wallpaper).", paths::tilde(f)),
                (ThemeSource::Auto, None) => "No end-4 or file manager theme found, so the seed color below is used.".to_string(),
                (ThemeSource::Seed, _) => "Using a palette generated from the seed color below.".to_string(),
            };
            row(ui, "Color source", &source_desc, |ui| {
                changed |= segmented(ui, &mut config.theme_source, &[
                    (ThemeSource::Auto, ICON_WALLPAPER, "Auto"),
                    (ThemeSource::Seed, ICON_PALETTE, "Seed"),
                ]);
            });
            ui.separator();
            let seed_mode = theme_file.is_none();
            ui.add_enabled_ui(seed_mode, |ui| {
                row(ui, "Color scheme", if seed_mode { "Light, dark, or follow the system." } else { "The theme file decides light or dark." }, |ui| {
                    changed |= segmented(ui, &mut config.color_scheme, &[
                        (ColorScheme::System, ICON_BRIGHTNESS_AUTO, "System"),
                        (ColorScheme::Light, ICON_LIGHT_MODE, "Light"),
                        (ColorScheme::Dark, ICON_DARK_MODE, "Dark"),
                    ]);
                });
                ui.separator();
                row(ui, "Seed color", "Every color in the interface is derived from this one.", |ui| {
                    let text = st.seed_text.get_or_insert_with(|| config.seed_color.clone());
                    let resp = ui.add(egui::TextEdit::singleline(text).desired_width(92.0).char_limit(7));
                    if resp.changed() {
                        if let Some(c) = parse_hex(text) {
                            config.seed_color = to_hex(c);
                            changed = true;
                        }
                    }
                    let mut color = parse_hex(&config.seed_color).unwrap_or(Color32::from_rgb(0x42, 0x85, 0xF4));
                    if ui.color_edit_button_srgba(&mut color).changed() {
                        config.seed_color = to_hex(color);
                        *text = config.seed_color.clone();
                        changed = true;
                    }
                    for hex in SEED_PRESETS.iter().rev() {
                        let c = parse_hex(hex).expect("valid preset");
                        let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), egui::Sense::click());
                        ui.painter().circle_filled(rect.center(), 11.0, c);
                        if config.seed_color.eq_ignore_ascii_case(hex) {
                            ui.painter().circle_stroke(rect.center(), 12.5, egui::Stroke::new(2.0, p.on_surface));
                        }
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            config.seed_color = hex.to_string();
                            *text = hex.to_string();
                            changed = true;
                        }
                    }
                });
            });
            ui.separator();
            row(ui, "Filled icons", "Use filled Material Symbols instead of outlined ones.", |ui| {
                changed |= switch(ui, &mut config.filled_icons).changed();
            });
            ui.separator();
            row(ui, "Interface scale", "Size of everything in the window.", |ui| {
                let r = ui.add(egui::Slider::new(&mut config.ui_scale, 0.75..=1.75).step_by(0.05).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
                // Apply on release so the slider doesn't jump under the cursor.
                if r.drag_stopped() || (r.changed() && !r.dragged()) {
                    changed = true;
                }
            });
        });

        card(ui, "Behaviour", |ui| {
            let picked = term::pick(&config.terminal);
            let desc = match &picked {
                Some(t) if config.terminal.trim().is_empty() => format!("Package installs and other commands open in a terminal. Automatic: {t}."),
                Some(t) => format!("Package installs and other commands open in {t}."),
                None => "No terminal found. Install one (kitty, foot, alacritty…) or type its command.".to_string(),
            };
            row(ui, "Terminal", &desc, |ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut config.terminal).hint_text("Automatic").desired_width(140.0));
                if r.lost_focus() {
                    changed = true;
                }
                let installed: Vec<&str> = term::known().filter(|t| cmd::exists(t)).collect();
                egui::ComboBox::from_id_salt("pref-terminal")
                    .width(120.0)
                    .selected_text("Choose")
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(config.terminal.is_empty(), "Automatic").clicked() {
                            config.terminal.clear();
                            changed = true;
                        }
                        for t in &installed {
                            if ui.selectable_label(config.terminal == *t, *t).clicked() {
                                config.terminal = t.to_string();
                                changed = true;
                            }
                        }
                        if installed.is_empty() {
                            ui.label(RichText::new("No known terminal installed").color(p.on_surface_variant));
                        }
                    });
            });
            ui.separator();
            let resolved = config.aur_helper.resolve();
            row(ui, "AUR helper", &match resolved {
                Some(h) => format!("AUR packages and system updates go through {h}."),
                None => "No AUR helper in use. AUR-only apps can't be installed from here.".to_string(),
            }, |ui| {
                changed |= segmented(ui, &mut config.aur_helper, &[
                    (AurHelper::Auto, ICON_AUTO_AWESOME, "Auto"),
                    (AurHelper::Paru, ICON_PACKAGE_2, "paru"),
                    (AurHelper::Yay, ICON_PACKAGE_2, "yay"),
                    (AurHelper::None, ICON_BLOCK, "None"),
                ]);
            });
        });

        card(ui, "Config files", |ui| {
            let backups = paths::state_dir().join("backups");
            row(ui, "Back up before saving", &format!("Keep the 20 most recent versions of each file in {}.", paths::tilde(&backups)), |ui| {
                if widgets::button(ui, ButtonKind::Text, Some(ICON_FOLDER_OPEN), "Open folder").clicked() {
                    let _ = std::fs::create_dir_all(&backups);
                    actions.push(Action::OpenPath(backups.clone()));
                }
                changed |= switch(ui, &mut config.backups).changed();
            });
            ui.separator();
            row(ui, "Editor font size", "Text size in the config file editor.", |ui| {
                let r = ui.add(egui::Slider::new(&mut config.editor_font_size, 9.0..=24.0).step_by(0.5).suffix(" pt"));
                if r.drag_stopped() || (r.changed() && !r.dragged()) {
                    changed = true;
                }
            });
            ui.separator();
            let path = Config::path().unwrap_or_default();
            row(ui, "Preferences file", &paths::tilde(&path), |ui| {
                if widgets::button(ui, ButtonKind::Text, Some(ICON_FOLDER_OPEN), "Show").clicked() {
                    if let Some(dir) = path.parent() {
                        actions.push(Action::OpenPath(dir.to_path_buf()));
                    }
                }
            });
        });

        ui.add_space(24.0);
        ui.label(RichText::new(format!("Material 3 Settings {} · Ctrl+, opens this page", env!("CARGO_PKG_VERSION"))).size(12.0).color(p.on_surface_variant));
    });

    if changed {
        actions.push(Action::ConfigChanged);
    }
    app.actions.extend(actions);
}
