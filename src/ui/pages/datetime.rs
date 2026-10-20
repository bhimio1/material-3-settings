//! Date & time: clock, time zone, network time, and locale.

use crate::ui::common::{dialog_buttons, modal};
use crate::app::App;
use crate::sys::task::Task;
use crate::sys::time::{self, LocaleStatus, TimeStatus};
use crate::ui::common::{self, card, row};
use crate::ui::widgets::{self, ButtonKind};
use crate::ui::{toast, Action, Page, ToastKind};
use egui::{RichText, Ui};
use egui_material_icons::icons::*;
use std::time::Duration;

#[derive(Default)]
pub struct State {
    status: Task<Result<TimeStatus, String>>,
    zones: Task<Result<Vec<String>, String>>,
    locale: Task<Result<LocaleStatus, String>>,
    locales: Task<Vec<String>>,
    op: Task<Result<String, String>>,
    picker: bool,
    filter: String,
    show_locales: bool,
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let ctx = app.ctx.clone();
    let p = app.palette.clone();
    let st = &mut app.pages.datetime;
    let actions = &mut app.actions;
    let has_timedatectl = crate::sys::cmd::exists("timedatectl");

    if has_timedatectl {
        st.status.ensure(&ctx, time::load_status);
    }
    if crate::sys::cmd::exists("localectl") {
        st.locale.ensure(&ctx, time::load_locale);
    }
    if let Some(result) = st.op.take() {
        match result {
            Ok(msg) => toast(actions, msg, ToastKind::Success),
            Err(e) => toast(actions, e, ToastKind::Error),
        }
        st.status.spawn(&ctx, time::load_status);
    }
    ctx.request_repaint_after(Duration::from_millis(500));

    let installed = app.installed.clone();
    common::page(ui, ICON_SCHEDULE, "Date & time", "Clock, time zone and language settings of the system.", |ui| {
        let now = chrono::Local::now();
        ui.add_space(16.0);
        egui::Frame::new()
            .fill(p.primary_container)
            .corner_radius(egui::CornerRadius::same(24))
            .inner_margin(egui::Margin::symmetric(24, 18))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(now.format("%H:%M:%S").to_string()).size(48.0).color(p.on_primary_container));
                let tz = match &st.status.value {
                    Some(Ok(s)) => format!(" · {}", s.timezone),
                    _ => String::new(),
                };
                ui.label(
                    RichText::new(format!("{}{tz} (UTC{})", now.format("%A, %B %-d, %Y"), now.format("%:z")))
                        .size(15.0)
                        .color(p.on_primary_container),
                );
            });

        card(ui, "Time", |ui| {
            if !common::require(ui, actions, &installed, "timedatectl", "systemd", "") {
                return;
            }
            match &st.status.value {
                None => common::loading(ui, "Reading time settings…"),
                Some(Err(e)) => common::note(ui, &format!("Couldn't read time settings: {e}")),
                Some(Ok(s)) => {
                    row(ui, "Time zone", &s.timezone, |ui| {
                        if widgets::button(ui, ButtonKind::Tonal, Some(ICON_PUBLIC), "Change").clicked() {
                            st.picker = true;
                            st.filter.clear();
                            if st.zones.value.is_none() && !st.zones.running() {
                                st.zones.spawn(&ctx, time::list_timezones);
                            }
                        }
                    });
                    ui.separator();
                    let desc = if !s.can_ntp {
                        "No NTP service is installed (systemd-timesyncd ships with systemd)."
                    } else if s.ntp && s.ntp_synced {
                        "Synchronized with a time server."
                    } else if s.ntp {
                        "On, waiting for the first synchronization."
                    } else {
                        "Off. The clock is only kept by the hardware clock."
                    };
                    row(ui, "Set time automatically", desc, |ui| {
                        let mut on = s.ntp;
                        if st.op.running() {
                            ui.add(egui::Spinner::new().color(p.primary));
                        } else if ui.add_enabled_ui(s.can_ntp, |ui| widgets::switch(ui, &mut on)).inner.changed() {
                            st.op.spawn(&ctx, move || {
                                time::set_ntp(on).map(|_| if on { "Network time on".into() } else { "Network time off".into() })
                            });
                        }
                    });
                    if s.local_rtc {
                        ui.separator();
                        row(ui, "Hardware clock in local time", "Usual when dual-booting Windows. Linux prefers UTC: `timedatectl set-local-rtc 0`.", |_| {});
                    }
                    ui.separator();
                    row(ui, "Clock format in the bar", "The 12/24-hour format of the end-4 shell's clock is set on its page.", |ui| {
                        if widgets::button(ui, ButtonKind::Text, Some(ICON_ARROW_FORWARD), "end4-pC / ii").clicked() {
                            actions.push(Action::Goto(Page::End4));
                        }
                    });
                }
            }
        });

        card(ui, "Language and region", |ui| match &st.locale.value {
            None => common::loading(ui, "Reading locale…"),
            Some(Err(e)) => common::note(ui, &format!("Couldn't run localectl: {e}")),
            Some(Ok(l)) => {
                let layout = if l.x11_variant.trim_matches(',').is_empty() {
                    l.x11_layout.clone()
                } else {
                    format!("{} ({})", l.x11_layout, l.x11_variant)
                };
                common::facts(ui, "locale", &[
                    ("System locale", l.locale.clone()),
                    ("Console keymap", l.keymap.clone()),
                    ("X11 keyboard", if layout.is_empty() { "—".into() } else { layout }),
                ]);
                common::note(ui, "Hyprland's keyboard layout is set under Hyprland → Input. New locales are enabled in /etc/locale.gen and built with `locale-gen`.");
                ui.horizontal(|ui| {
                    if widgets::button(ui, ButtonKind::Outlined, Some(ICON_TRANSLATE), if st.show_locales { "Hide installed locales" } else { "Installed locales" }).clicked() {
                        st.show_locales = !st.show_locales;
                        if st.show_locales {
                            st.locales.spawn(&ctx, time::available_locales);
                        }
                    }
                    if widgets::button(ui, ButtonKind::Text, Some(ICON_EDIT), "Edit locale.gen").clicked() {
                        actions.push(Action::RunInTerminal("sudoedit /etc/locale.gen && sudo locale-gen".into()));
                    }
                });
                if st.show_locales {
                    st.locales.poll();
                    match &st.locales.value {
                        None => common::loading(ui, "Listing locales…"),
                        Some(list) => {
                            ui.add_space(6.0);
                            ui.label(RichText::new(list.join("  ·  ")).size(12.5).color(p.on_surface_variant));
                        }
                    }
                }
                ui.add_space(8.0);
            }
        });
    });

    if st.picker {
        st.zones.poll();
        let mut close = false;
        let current = match &st.status.value {
            Some(Ok(s)) => s.timezone.clone(),
            _ => String::new(),
        };
        let r = modal(&ctx, &p, "timezone", ICON_PUBLIC, "Time zone", |ui| {
            ui.set_width(420.0);
            let edit = ui.add(
                egui::TextEdit::singleline(&mut st.filter)
                    .hint_text("Search, e.g. Berlin or America/")
                    .desired_width(f32::INFINITY),
            );
            if !edit.has_focus() && st.filter.is_empty() {
                edit.request_focus();
            }
            ui.add_space(8.0);
            match &st.zones.value {
                None => common::loading(ui, "Loading time zones…"),
                Some(Err(e)) => {
                    ui.label(RichText::new(e).color(p.error));
                }
                Some(Ok(zones)) => {
                    let q = st.filter.to_lowercase().replace(' ', "_");
                    let matches: Vec<&String> = zones.iter().filter(|z| z.to_lowercase().contains(&q)).collect();
                    egui::ScrollArea::vertical().max_height(360.0).auto_shrink([false, true]).show_rows(ui, 30.0, matches.len(), |ui, range| {
                        for z in &matches[range] {
                            let resp = ui.add_sized([ui.available_width(), 30.0], egui::Button::selectable(**z == current, z.as_str()));
                            if resp.clicked() {
                                let tz = (*z).clone();
                                st.op.spawn(&ctx, move || time::set_timezone(&tz).map(|_| format!("Time zone set to {tz}")));
                                close = true;
                            }
                        }
                    });
                    if matches.is_empty() {
                        ui.label(RichText::new("No matching time zone").color(p.on_surface_variant));
                    }
                }
            }
            dialog_buttons(ui, |ui| {
                if widgets::button(ui, ButtonKind::Text, None, "Cancel").clicked() {
                    close = true;
                }
            });
        });
        if close || r.should_close() {
            st.picker = false;
        }
    }
}
