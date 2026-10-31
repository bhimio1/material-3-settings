//! Shell integration: add the Material 3 changes to the stock shells
//! (end-4's ii, pctrade's end4-pC, iNiR, Dux Shell) instead of using a fork.

use crate::app::App;
use crate::inject::{self, State as InjectState, Status, Target};
use crate::keybinds;
use crate::ui::{toast, ToastKind};
use egui::{RichText, Ui};
use material_3_ui::common::{self, card};
use material_3_ui::icons::icons::*;
use material_3_ui::sys::{paths, task::Task};
use material_3_ui::widgets::{self, ButtonKind};

#[derive(Default)]
pub struct State {
    status: Task<Vec<Status>>,
    keys: Task<Option<keybinds::Plan>>,
    job: Task<Result<String, String>>,
    job_label: String,
}

/// Keybinds only make sense on Hyprland.
fn load_keys() -> Option<keybinds::Plan> {
    let home = paths::home();
    let hypr = home.join(".config/hypr");
    (hypr.join("hyprland.lua").is_file() || hypr.join("hyprland.conf").is_file())
        .then(|| keybinds::plan(&home, &keybinds::live_keys(), keybinds::have_games(&home)))
}

fn load() -> Vec<Status> {
    let home = paths::home();
    Target::ALL.into_iter().map(|t| inject::status(&home, t)).collect()
}

/// Apply to one shell, keep the watcher in step, and check the shell
/// survived it.
pub fn apply_one(t: Target) -> Result<String, String> {
    let home = paths::home();
    let was_running = inject::running_shells().iter().any(|s| s == t.id());
    let report = inject::apply(&home, t)?;
    inject::watcher::sync(&home)?;
    inject::verify_running(&home, t, was_running)?;
    Ok(report.summary(t))
}

pub fn remove_one(t: Target) -> Result<String, String> {
    let home = paths::home();
    let msg = inject::remove(&home, t)?;
    inject::watcher::sync(&home)?;
    Ok(msg)
}

fn description(t: Target) -> &'static str {
    match t {
        Target::Ii | Target::End4Pc => "Adds a small m3 folder to the shell and a few marked lines to its files. Re-added automatically after the shell updates.",
        Target::Inir => "Adds a small m3 folder to the shell and a few marked lines to its files. `inir update` replaces them; they're put back within seconds.",
        Target::Dux => "Installs a Dux Shell plugin. Nothing of the shell itself is changed, so updates never affect it.",
    }
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let ctx = app.ctx.clone();
    let p = app.palette.clone();
    let actions = &mut app.actions;
    let st = &mut app.pages.shells;

    st.status.ensure(&ctx, load);
    st.keys.ensure(&ctx, load_keys);
    if let Some(result) = st.job.take() {
        match result {
            Ok(msg) => toast(actions, msg, ToastKind::Success),
            Err(e) => toast(actions, format!("{} failed: {e}", st.job_label), ToastKind::Error),
        }
        st.status.spawn(&ctx, load);
        st.keys.spawn(&ctx, load_keys);
    }
    let backups = app.config.backups;
    let busy = st.job.running();

    common::page(
        ui,
        ICON_EXTENSION,
        "Shell integration",
        "Material 3 Settings adds the Material 3 apps' integration to the shell you already use: the game library, Game Mode, calendar events in the sidebar, screenshots in Material 3 Screenshots and the first-run welcome. Your shell keeps updating from its own developers.",
        |ui| {
            let Some(list) = st.status.value.clone() else {
                common::loading(ui, "Looking for shells…");
                return;
            };
            let installed: Vec<&Status> = list.iter().filter(|s| s.state != InjectState::NotInstalled).collect();
            if installed.is_empty() {
                common::banner(ui, ICON_INFO, "None of the supported shells is installed: end-4's illogical-impulse, pctrade's end4-pC, iNiR or Dux Shell.", false, |_| {});
            }
            for s in installed {
                let t = s.target;
                card(ui, t.label(), |ui| {
                    let (state, on) = match &s.state {
                        InjectState::Applied { .. } => ("Added", true),
                        InjectState::NeedsReapply => ("Needs adding again", false),
                        _ => ("Not added", false),
                    };
                    common::row(ui, state, description(t), |ui| {
                        ui.add_enabled_ui(!busy, |ui| {
                            let added = s.state != InjectState::Clean;
                            if added && widgets::button(ui, ButtonKind::Text, Some(ICON_DELETE), "Remove").clicked() {
                                st.job_label = format!("Removing from {}", t.label());
                                st.job.spawn(&ctx, move || remove_one(t));
                            }
                            let (icon, label) = if added { (ICON_SYNC, "Update") } else { (ICON_ADD, "Add") };
                            if widgets::button(ui, if on { ButtonKind::Tonal } else { ButtonKind::Filled }, Some(icon), label).clicked() {
                                st.job_label = format!("Adding to {}", t.label());
                                st.job.spawn(&ctx, move || apply_one(t));
                            }
                        });
                    });
                    if let Some(r) = &s.record {
                        if !r.skipped.is_empty() {
                            ui.add_space(4.0);
                            for sk in &r.skipped {
                                ui.horizontal(|ui| {
                                    widgets::icon(ui, ICON_WARNING, 16.0, p.error);
                                    ui.label(
                                        RichText::new(format!("{} isn't available with this version of the shell: {}", sk.feature, sk.reason))
                                            .size(12.5)
                                            .color(p.on_surface_variant),
                                    );
                                });
                            }
                        }
                    }
                });
            }
            if let Some(Some(plan)) = st.keys.value.clone() {
                card(ui, "Keyboard shortcuts", |ui| {
                    let free = plan.binds.iter().filter(|(_, f)| *f).count();
                    let desc = if plan.added > 0 {
                        format!("{} Material 3 keybinds added to your Hyprland config.", plan.added)
                    } else {
                        "Hyprland keybinds for the Material 3 apps. Keys you've already bound are left alone.".to_string()
                    };
                    common::row(ui, if plan.added > 0 { "Added" } else { "Not added" }, &desc, |ui| {
                        ui.add_enabled_ui(!busy, |ui| {
                            if plan.added > 0 && widgets::button(ui, ButtonKind::Text, Some(ICON_DELETE), "Remove").clicked() {
                                st.job_label = "Removing the keybinds".into();
                                st.job.spawn(&ctx, move || keybinds::remove(&paths::home(), backups));
                            }
                            if (plan.added == 0 || free > plan.added)
                                && widgets::button(ui, if plan.added > 0 { ButtonKind::Tonal } else { ButtonKind::Filled }, Some(ICON_ADD), "Add").clicked()
                            {
                                st.job_label = "Adding the keybinds".into();
                                st.job.spawn(&ctx, move || keybinds::apply(&paths::home(), backups));
                            }
                        });
                    });
                    ui.add_space(4.0);
                    for (s, free) in &plan.binds {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(crate::welcome::pretty_keys(s.key)).size(12.5).strong());
                            let note = if *free { s.description.to_string() } else { format!("{} (key already in use)", s.description) };
                            ui.label(RichText::new(note).size(12.5).color(p.on_surface_variant));
                        });
                    }
                });
            }
            if busy {
                ui.add_space(8.0);
                ui.label(RichText::new(format!("{}…", st.job_label)).color(p.on_surface_variant));
            }
            ui.add_space(12.0);
            if common::refresh_button(ui, st.status.running()) {
                st.status.spawn(&ctx, load);
                st.keys.spawn(&ctx, load_keys);
            }
        },
    );
}
