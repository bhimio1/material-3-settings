//! Login & boot: the Material 3 SDDM login screen and Plymouth boot
//! animation, both in the shell's colors (see `crate::login`).

use crate::app::App;
use crate::login::{self, Status};
use crate::ui::{toast, ToastKind};
use egui::Ui;
use material_3_ui::common::{self, card};
use material_3_ui::icons::icons::*;
use material_3_ui::sys::{paths, task::Task};
use material_3_ui::widgets::{self, ButtonKind};

#[derive(Default)]
pub struct State {
    status: Task<Status>,
    job: Task<Result<String, String>>,
    job_label: String,
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let ctx = app.ctx.clone();
    let p = app.palette.clone();
    let actions = &mut app.actions;
    let st = &mut app.pages.login;

    st.status.ensure(&ctx, login::status);
    if let Some(result) = st.job.take() {
        match result {
            Ok(msg) => toast(actions, msg, ToastKind::Success),
            Err(e) => toast(actions, format!("{} failed: {e}", st.job_label), ToastKind::Error),
        }
        st.status.spawn(&ctx, login::status);
    }
    let busy = st.job.running();

    common::page(
        ui,
        ICON_LOGIN,
        "Login & boot",
        "A Material 3 login screen and boot animation in your shell's colors. They follow whichever shell you've added the integration to (ii, end4-pC, iNiR or Dux Shell). Installing asks for your password once.",
        |ui| {
            let Some(s) = st.status.value.clone() else {
                common::loading(ui, "Checking…");
                return;
            };
            if login::scripts_dir().is_none() {
                common::banner(ui, ICON_WARNING, "The login and boot themes weren't found in /usr/share/m3-settings/login. Reinstall Material 3 Settings.", true, |_| {});
            }

            card(ui, "Login screen", |ui| {
                let desc = if !s.sddm_available {
                    "Needs SDDM (sudo pacman -S sddm).".to_string()
                } else if s.sddm_installed && s.sddm_selected() {
                    "Shows your wallpaper, colors, fonts and avatar, and updates by itself when they change.".to_string()
                } else if s.sddm_installed {
                    format!("Installed, but SDDM shows {}. Install again to select it.", s.sddm_current.as_deref().unwrap_or("its default theme"))
                } else {
                    "Matches the lock screen. Your previous theme comes back if you remove it.".to_string()
                };
                common::row(ui, if s.sddm_installed { "Installed" } else { "Not installed" }, &desc, |ui| {
                    ui.add_enabled_ui(!busy && s.sddm_available, |ui| {
                        if s.sddm_installed {
                            if widgets::button(ui, ButtonKind::Text, Some(ICON_DELETE), "Remove").clicked() {
                                st.job_label = "Removing the login screen".into();
                                st.job.spawn(&ctx, || login::remove_sddm(&paths::home()));
                            }
                            if widgets::button(ui, ButtonKind::Text, Some(ICON_SYNC), "Update now").clicked() {
                                st.job_label = "Updating the login screen".into();
                                st.job.spawn(&ctx, login::sync);
                            }
                        }
                        let (kind, label) = if s.sddm_installed && s.sddm_selected() { (ButtonKind::Tonal, "Reinstall") } else { (ButtonKind::Filled, "Install") };
                        if widgets::button(ui, kind, Some(ICON_DOWNLOAD), label).clicked() {
                            st.job_label = "Installing the login screen".into();
                            st.job.spawn(&ctx, || login::install_sddm(&paths::home()));
                        }
                    });
                });
                if let Some(dm) = &s.other_dm {
                    ui.horizontal(|ui| {
                        widgets::icon(ui, ICON_INFO, 16.0, p.on_surface_variant);
                        ui.label(egui::RichText::new(format!("Your login manager is {dm}; the theme shows once you switch to SDDM.")).size(12.5).color(p.on_surface_variant));
                    });
                }
            });

            card(ui, "Boot animation", |ui| {
                let desc = if !s.plymouth_available {
                    "Needs Plymouth (sudo pacman -S plymouth).".to_string()
                } else if s.plymouth_installed && s.plymouth_selected() {
                    "The illogical-impulse emblem drawing itself in. The colors are baked in at install time; refresh them after changing wallpaper.".to_string()
                } else {
                    "The illogical-impulse emblem drawing itself in, in your colors. Installing rebuilds the initramfs, which takes a minute.".to_string()
                };
                let state = match (s.plymouth_installed, s.plymouth_selected()) {
                    (true, true) => "Installed",
                    (true, false) => "Installed, not selected",
                    _ => "Not installed",
                };
                common::row(ui, state, &desc, |ui| {
                    ui.add_enabled_ui(!busy && s.plymouth_available, |ui| {
                        let (kind, icon, label) = if s.plymouth_installed && s.plymouth_selected() {
                            (ButtonKind::Tonal, ICON_PALETTE, "Refresh colors")
                        } else {
                            (ButtonKind::Filled, ICON_DOWNLOAD, "Install")
                        };
                        if widgets::button(ui, kind, Some(icon), label).clicked() {
                            st.job_label = "Installing the boot animation".into();
                            st.job.spawn(&ctx, login::install_plymouth);
                        }
                    });
                });
            });

            if busy {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(format!("{}…", st.job_label)).color(p.on_surface_variant));
            }
            ui.add_space(12.0);
            if common::refresh_button(ui, st.status.running()) {
                st.status.spawn(&ctx, login::status);
            }
        },
    );
}
