//! Default applications per file type, through `xdg-mime`.

use crate::app::App;
use crate::sys::mime::{self, DesktopEntry, CATEGORIES};
use crate::sys::task::Task;
use crate::ui::common::{self, card, row};
use crate::ui::widgets::{self, ButtonKind};
use crate::ui::{toast, ToastKind};
use egui::{RichText, Ui};
use egui_material_icons::{icons::*, MaterialIcon};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct State {
    apps: Task<Vec<DesktopEntry>>,
    defaults: Task<BTreeMap<&'static str, String>>,
    op: Task<Result<String, String>>,
}

fn load_defaults() -> BTreeMap<&'static str, String> {
    mime::load_defaults()
}

/// A picker row. Returns the chosen desktop ID when it changed.
fn picker(
    ui: &mut Ui,
    key: &str,
    icon: MaterialIcon,
    label: &str,
    desc: &str,
    current: Option<&String>,
    candidates: &[&DesktopEntry],
) -> Option<String> {
    let p = widgets::palette(ui);
    let mut chosen = None;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        widgets::icon(ui, icon, 24.0, p.primary);
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.set_max_width((ui.available_width() - 280.0).max(160.0));
            ui.label(RichText::new(label).size(14.5).color(p.on_surface));
            ui.label(RichText::new(desc).size(12.0).color(p.on_surface_variant));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let current_name = current
                .map(|id| {
                    candidates
                        .iter()
                        .find(|c| c.id == *id)
                        .map(|c| c.name.clone())
                        .unwrap_or_else(|| id.trim_end_matches(".desktop").to_string())
                })
                .unwrap_or_else(|| "Not set".into());
            if candidates.is_empty() {
                ui.label(RichText::new("No installed app handles this").color(p.on_surface_variant));
                return;
            }
            egui::ComboBox::from_id_salt(("default-app", key))
                .width(250.0)
                .selected_text(current_name)
                .show_ui(ui, |ui| {
                    for c in candidates {
                        let selected = current == Some(&c.id);
                        let text = if c.no_display {
                            format!("{} (hidden)", c.name)
                        } else {
                            c.name.clone()
                        };
                        if ui
                            .selectable_label(selected, text)
                            .on_hover_text(&c.id)
                            .clicked()
                            && !selected
                        {
                            chosen = Some(c.id.clone());
                        }
                    }
                });
        });
    });
    ui.add_space(6.0);
    chosen
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let ctx = app.ctx.clone();
    let st = &mut app.pages.default_apps;
    let actions = &mut app.actions;
    let has_xdg_mime = crate::sys::cmd::exists("xdg-mime");

    st.apps.ensure(&ctx, mime::scan);
    if has_xdg_mime {
        st.defaults.ensure(&ctx, load_defaults);
    }
    if let Some(result) = st.op.take() {
        match result {
            Ok(msg) => toast(actions, msg, ToastKind::Success),
            Err(e) => toast(actions, e, ToastKind::Error),
        }
        st.defaults.spawn(&ctx, load_defaults);
    }

    let installed = app.installed.clone();
    common::page(ui, ICON_APPS, "Default apps", "Which app opens links, folders and each kind of file.", |ui| {
        if !common::require(ui, actions, &installed, "xdg-mime", "xdg-utils", "It reads and sets default applications.") {
            return;
        }
        let (Some(apps), Some(defaults)) = (&st.apps.value, &st.defaults.value) else {
            card(ui, "", |ui| common::loading(ui, "Looking for installed apps…"));
            return;
        };
        card(ui, "Defaults", |ui| {
            for (i, cat) in CATEGORIES.iter().enumerate() {
                if i > 0 {
                    ui.separator();
                }
                let candidates = mime::candidates(apps, cat);
                let desc = cat.types.join(", ");
                if let Some(id) = picker(ui, cat.key, cat.icon, cat.label, &desc, defaults.get(cat.key), &candidates) {
                    let types: Vec<&'static str> = cat.types.to_vec();
                    let label = cat.label;
                    let name = candidates.iter().find(|c| c.id == id).map(|c| c.name.clone()).unwrap_or_default();
                    st.op.spawn(&ctx, move || mime::set_default(&id, &types).map(|_| format!("{label}: {name}")));
                }
            }
        });

        card(ui, "Terminal", |ui| {
            let terms = mime::terminals(apps);
            let desc = format!(
                "Used by apps that launch a terminal through xdg-terminal-exec{}. Saved in {}.",
                if crate::sys::cmd::exists("xdg-terminal-exec") { "" } else { " (not installed)" },
                crate::sys::paths::tilde(&mime::terminals_list_path())
            );
            if let Some(id) = picker(ui, "terminal", ICON_TERMINAL, "Terminal emulator", &desc, defaults.get("terminal"), &terms) {
                st.op.spawn(&ctx, move || mime::set_default_terminal(&id).map(|_| format!("Default terminal: {id}")));
            }
            ui.separator();
            row(ui, "Terminal for this app", "Package installs and other commands from Settings open in the terminal chosen in Preferences.", |ui| {
                if widgets::button(ui, ButtonKind::Text, Some(ICON_SETTINGS), "Preferences").clicked() {
                    actions.push(crate::ui::Action::Goto(crate::ui::Page::Preferences));
                }
            });
        });

        let count = apps.len();
        let busy = st.apps.running() || st.defaults.running();
        let mut refresh = false;
        card(ui, "", |ui| {
            row(ui, "Refresh", &format!("{count} applications found."), |ui| {
                refresh = common::refresh_button(ui, busy);
            });
        });
        if refresh {
            st.apps.spawn(&ctx, mime::scan);
            st.defaults.spawn(&ctx, load_defaults);
        }
    });
}
