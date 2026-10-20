//! Power: profiles, battery, brightness, idle, and session actions.

use crate::app::App;
use crate::sys::power::{self, Backlight, PowerSupplies, Profiles};
use crate::sys::scx;
use crate::sys::task::Task;
use crate::ui::common::{dialog_buttons, modal};
use crate::ui::common::{self, card, row};
use crate::ui::widgets::{self, ButtonKind};
use crate::ui::{toast, Action, Page, ToastKind};
use egui::{vec2, RichText, Ui};
use egui_material_icons::{icons::*, MaterialIcon};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SessionAction {
    Suspend,
    Hibernate,
    Reboot,
    PowerOff,
}

impl SessionAction {
    fn label(self) -> &'static str {
        match self {
            SessionAction::Suspend => "Suspend",
            SessionAction::Hibernate => "Hibernate",
            SessionAction::Reboot => "Restart",
            SessionAction::PowerOff => "Shut down",
        }
    }

    fn icon(self) -> MaterialIcon {
        match self {
            SessionAction::Suspend => ICON_BEDTIME,
            SessionAction::Hibernate => ICON_ACUTE,
            SessionAction::Reboot => ICON_RESTART_ALT,
            SessionAction::PowerOff => ICON_POWER_SETTINGS_NEW,
        }
    }

    fn verb(self) -> &'static str {
        match self {
            SessionAction::Suspend => "suspend",
            SessionAction::Hibernate => "hibernate",
            SessionAction::Reboot => "reboot",
            SessionAction::PowerOff => "poweroff",
        }
    }
}

#[derive(Default)]
pub struct State {
    profiles: Task<Result<Profiles, String>>,
    supplies: Task<PowerSupplies>,
    brightness: Task<Result<Option<Backlight>, String>>,
    /// Slider value while dragging, applied on release.
    bright_drag: Option<f32>,
    /// A profile/brightness/threshold change in flight.
    op: Task<Result<String, String>>,
    confirm: Option<SessionAction>,
    threshold: Option<u8>,
    swap: Option<bool>,
    /// sched-ext: what's running and which schedulers exist.
    scx: Task<Result<(scx::Current, Vec<String>), String>>,
    /// Scheduler and mode picked in the card, before applying.
    scx_pick: Option<(String, String)>,
}

fn load_scx() -> Result<(scx::Current, Vec<String>), String> {
    Ok((scx::current()?, scx::list()?))
}

fn battery_icon(capacity: u8, charging: bool) -> MaterialIcon {
    if charging {
        return ICON_BATTERY_CHARGING_FULL;
    }
    match capacity {
        0..=10 => ICON_BATTERY_ALERT,
        11..=30 => ICON_BATTERY_2_BAR,
        31..=50 => ICON_BATTERY_3_BAR,
        51..=70 => ICON_BATTERY_4_BAR,
        71..=90 => ICON_BATTERY_6_BAR,
        _ => ICON_BATTERY_FULL,
    }
}

fn profile_icon(name: &str) -> MaterialIcon {
    match name {
        "power-saver" => ICON_ECO,
        "performance" => ICON_SPEED,
        _ => ICON_BALANCE,
    }
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let ctx = app.ctx.clone();
    let p = app.palette.clone();
    let st = &mut app.pages.power;
    let actions = &mut app.actions;

    st.supplies.ensure(&ctx, power::read_supplies);
    if st
        .supplies
        .finished_at
        .is_some_and(|t| t.elapsed() > Duration::from_secs(10))
        && !st.supplies.running()
    {
        st.supplies.spawn(&ctx, power::read_supplies);
    }
    ctx.request_repaint_after(Duration::from_secs(10));
    let has_ppd = crate::sys::cmd::exists("powerprofilesctl");
    if has_ppd {
        st.profiles.ensure(&ctx, power::load_profiles);
    }
    let has_brightnessctl = crate::sys::cmd::exists("brightnessctl");
    if has_brightnessctl {
        st.brightness.ensure(&ctx, power::load_brightness);
    }
    let has_scx = scx::available();
    if has_scx {
        st.scx.ensure(&ctx, load_scx);
    }
    let swap = *st.swap.get_or_insert_with(power::has_swap);
    if let Some(result) = st.op.take() {
        match result {
            Ok(msg) if !msg.is_empty() => toast(actions, msg, ToastKind::Success),
            Ok(_) => {}
            Err(e) => toast(actions, e, ToastKind::Error),
        }
        if has_ppd {
            st.profiles.spawn(&ctx, power::load_profiles);
        }
        if has_brightnessctl {
            st.brightness.spawn(&ctx, power::load_brightness);
        }
        st.supplies.spawn(&ctx, power::read_supplies);
        if has_scx {
            st.scx.spawn(&ctx, load_scx);
        }
    }

    let installed = app.installed.clone();
    common::page(ui, ICON_BATTERY_CHARGING_FULL, "Power", "Power mode, battery, screen brightness and session.", |ui| {
        // ------------------------------------------------------ profiles
        card(ui, "Power mode", |ui| {
            if !common::require(ui, actions, &installed, "powerprofilesctl", "power-profiles-daemon", "It switches between power saving and performance.") {
                return;
            }
            match &st.profiles.value {
                None => common::loading(ui, "Reading power profiles…"),
                Some(Err(e)) => common::note(ui, &format!("power-profiles-daemon isn't responding: {e}. Enable it with `systemctl enable --now power-profiles-daemon`.")),
                Some(Ok(profiles)) => {
                    let order = ["power-saver", "balanced", "performance"];
                    let mut names: Vec<&str> = order.iter().copied().filter(|n| profiles.available.iter().any(|a| a == n)).collect();
                    names.extend(profiles.available.iter().map(String::as_str).filter(|n| !order.contains(n)));
                    let mut selected = names.iter().position(|n| *n == profiles.active).unwrap_or(usize::MAX);
                    let desc = match profiles.active.as_str() {
                        "power-saver" => "Longer battery life, lower performance.",
                        "performance" => "Maximum performance, more heat and power draw.",
                        _ => "Standard performance and power usage.",
                    };
                    row(ui, "Mode", desc, |ui| {
                        if st.op.running() {
                            ui.add(egui::Spinner::new().color(p.primary));
                        }
                        let options: Vec<(usize, MaterialIcon, &str)> = names.iter().enumerate()
                            .map(|(i, n)| (i, profile_icon(n), power::profile_label(n)))
                            .collect();
                        if widgets::segmented(ui, &mut selected, &options) {
                            if let Some(name) = names.get(selected) {
                                let name = name.to_string();
                                st.op.spawn(&ctx, move || power::set_profile(&name).map(|_| String::new()));
                            }
                        }
                    });
                }
            }
        });

        // ------------------------------------------------------ battery
        let supplies = st.supplies.value.clone().unwrap_or_default();
        if !supplies.batteries.is_empty() || supplies.ac_online.is_some() {
            card(ui, "Battery", |ui| {
                for (i, b) in supplies.batteries.iter().enumerate() {
                    if i > 0 {
                        ui.separator();
                    }
                    let cap = b.capacity.unwrap_or(0);
                    let charging = b.status == "Charging" || (b.status != "Discharging" && supplies.ac_online == Some(true));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let color = if cap <= 15 && !charging { p.error } else { p.primary };
                        widgets::icon(ui, battery_icon(cap, charging), 40.0, color);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(format!("{cap}%")).size(26.0).color(p.on_surface));
                            let mut status = match b.status.as_str() {
                                "Not charging" => "Plugged in, not charging".to_string(),
                                s => s.to_string(),
                            };
                            if let Some(h) = b.hours_left {
                                status.push_str(&match b.status.as_str() {
                                    "Charging" => format!(" · full in {}", power::format_hours(h)),
                                    _ => format!(" · {} left", power::format_hours(h)),
                                });
                            }
                            ui.label(RichText::new(status).color(p.on_surface_variant));
                        });
                    });
                    ui.add_space(6.0);
                    widgets::linear_progress(ui, Some(cap as f32 / 100.0), ui.available_width());
                    let mut facts = vec![("Device", if b.model.is_empty() { b.name.clone() } else { format!("{} ({})", b.model, b.name) })];
                    if let Some(h) = b.health {
                        facts.push(("Health", format!("{h:.0}% of design capacity")));
                    }
                    common::facts(ui, &format!("bat-{i}"), &facts);
                    if let Some(end) = b.end_threshold {
                        let value = st.threshold.get_or_insert(end);
                        row(ui, "Charge limit", "Stop charging at this level to slow battery wear. Changing it asks for your password; most firmware resets it at boot.", |ui| {
                            let dirty = *value != end;
                            if widgets::button_enabled(ui, ButtonKind::Tonal, None, "Apply", dirty && !st.op.running()).clicked() {
                                let v = *value;
                                let path = b.path.clone();
                                st.op.spawn(&ctx, move || power::set_threshold(&path, "charge_control_end_threshold", v).map(|_| format!("Charge limit set to {v}%")));
                            }
                            ui.add(egui::Slider::new(value, 50..=100).suffix("%"));
                        });
                    }
                }
                if let Some(online) = supplies.ac_online {
                    if !supplies.batteries.is_empty() {
                        ui.separator();
                    }
                    row(ui, "Power adapter", if online { "Connected" } else { "Not connected" }, |ui| {
                        widgets::icon(ui, if online { ICON_POWER } else { ICON_POWER_OFF }, 22.0, p.on_surface_variant);
                    });
                }
            });
        }

        // ---------------------------------------------------- brightness
        // ---------------------------------------------------- sched-ext
        // CachyOS ships scx_loader; elsewhere it's the scx-scheds package.
        if has_scx {
            card(ui, "CPU scheduler", |ui| {
                match &st.scx.value {
                    None => common::loading(ui, "Asking scx_loader…"),
                    Some(Err(e)) => common::note(ui, &format!("scx_loader isn't responding: {e}. Enable it with `systemctl enable --now scx_loader`.")),
                    Some(Ok((current, scheds))) => {
                        let busy = st.op.running() || st.scx.running();
                        row(ui, "Running now", &scx::describe(current), |ui| {
                            if current.sched.is_some() {
                                if widgets::button_enabled(ui, ButtonKind::Text, None, "Stop", !busy).clicked() {
                                    st.op.spawn(&ctx, scx::stop);
                                }
                                if widgets::button_enabled(ui, ButtonKind::Text, Some(ICON_RESTART_ALT), "Default", !busy)
                                    .on_hover_text("Restore the default scheduler from scx_loader's config")
                                    .clicked()
                                {
                                    st.op.spawn(&ctx, scx::restore);
                                }
                            }
                        });
                        ui.separator();
                        let (pick_sched, pick_mode) = st.scx_pick.get_or_insert_with(|| {
                            (
                                current.sched.clone().unwrap_or_else(|| "lavd".into()),
                                current.mode.clone().unwrap_or_else(|| "gaming".into()),
                            )
                        });
                        row(ui, "Switch scheduler", "sched-ext schedulers load without a reboot. For games, scx_lavd in Gaming mode is a good start.", |ui| {
                            let running = current.sched.is_some();
                            let same = current.sched.as_deref() == Some(pick_sched.as_str()) && current.mode.as_deref() == Some(pick_mode.as_str());
                            if widgets::button_enabled(ui, ButtonKind::Filled, None, "Apply", !busy && !same).clicked() {
                                let (sched, mode) = (pick_sched.clone(), pick_mode.clone());
                                st.op.spawn(&ctx, move || scx::switch(&sched, &mode, running));
                            }
                            egui::ComboBox::from_id_salt("scx-mode")
                                .width(130.0)
                                .selected_text(scx::MODES.iter().find(|(k, _)| *k == pick_mode.as_str()).map(|(_, l)| *l).unwrap_or(pick_mode.as_str()))
                                .show_ui(ui, |ui| {
                                    for (key, label) in scx::MODES {
                                        ui.selectable_value(pick_mode, key.to_string(), label);
                                    }
                                });
                            egui::ComboBox::from_id_salt("scx-sched")
                                .width(130.0)
                                .selected_text(format!("scx_{pick_sched}"))
                                .show_ui(ui, |ui| {
                                    for s in scheds {
                                        ui.selectable_value(pick_sched, s.clone(), format!("scx_{s}"));
                                    }
                                });
                        });
                    }
                }
            });
        }

        card(ui, "Screen", |ui| {
            if !common::require(ui, actions, &installed, "brightnessctl", "brightnessctl", "It changes the screen backlight.") {
                return;
            }
            match &st.brightness.value {
                None => common::loading(ui, "Reading brightness…"),
                Some(Err(e)) => common::note(ui, &format!("Couldn't read the backlight: {e}")),
                Some(Ok(None)) => common::note(ui, "No adjustable backlight found. External monitors usually need ddcutil instead."),
                Some(Ok(Some(b))) => {
                    let device = b.device.clone();
                    let current = b.percent as f32;
                    row(ui, "Brightness", &format!("Backlight device {device}."), |ui| {
                        let value = st.bright_drag.get_or_insert(current);
                        let r = ui.add(egui::Slider::new(value, 1.0..=100.0).suffix("%").integer());
                        let release = r.drag_stopped() || (r.changed() && !r.dragged());
                        if release {
                            let v = *value as u8;
                            st.bright_drag = None;
                            st.op.spawn(&ctx, move || power::set_brightness(&device, v).map(|_| String::new()));
                        } else if !r.dragged() && !r.has_focus() && !st.op.running() {
                            st.bright_drag = None;
                        }
                        widgets::icon(ui, ICON_BRIGHTNESS_6, 22.0, p.on_surface_variant);
                    });
                }
            }
        });

        // ---------------------------------------------------------- idle
        card(ui, "Idle and lock", |ui| {
            let has_hypridle = crate::sys::cmd::exists("hypridle");
            row(
                ui,
                "Screen dimming, locking and sleep",
                if has_hypridle {
                    "Handled by hypridle. Edit its timeouts and actions in hypridle.conf."
                } else {
                    "Hyprland uses hypridle to dim, lock and suspend after inactivity. It isn't installed."
                },
                |ui| {
                    if has_hypridle {
                        if widgets::button(ui, ButtonKind::Tonal, Some(ICON_EDIT), "Configure").clicked() {
                            actions.push(Action::Goto(Page::App("hypridle".into())));
                        }
                    } else if widgets::button(ui, ButtonKind::Filled, Some(ICON_DOWNLOAD), "Install").clicked() {
                        actions.push(Action::Install { packages: vec!["hypridle".into()] });
                    }
                },
            );
            ui.separator();
            row(ui, "Lock screen", "hyprlock's look and behaviour.", |ui| {
                if widgets::button(ui, ButtonKind::Text, Some(ICON_LOCK), "hyprlock").clicked() {
                    actions.push(Action::Goto(Page::App("hyprlock".into())));
                }
            });
        });

        // ------------------------------------------------------- session
        card(ui, "Session", |ui| {
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                let mut list = vec![SessionAction::Suspend];
                if swap {
                    list.push(SessionAction::Hibernate);
                }
                list.extend([SessionAction::Reboot, SessionAction::PowerOff]);
                for a in list {
                    let kind = if a == SessionAction::PowerOff { ButtonKind::Danger } else { ButtonKind::Outlined };
                    if widgets::button(ui, kind, Some(a.icon()), a.label()).clicked() {
                        st.confirm = Some(a);
                    }
                }
            });
            ui.add_space(10.0);
        });
    });

    if let Some(a) = st.confirm {
        let mut close = false;
        let r = modal(&ctx, &p, "session", a.icon(), &format!("{}?", a.label()), |ui| {
            ui.set_width(360.0);
            let text = match a {
                SessionAction::Suspend => "The computer will go to sleep. Unsaved work stays in memory.",
                SessionAction::Hibernate => "Memory is saved to swap and the computer turns off. Needs a resume hook and a large enough swap.",
                SessionAction::Reboot => "All apps will close. Save your work first.",
                SessionAction::PowerOff => "All apps will close and the computer turns off. Save your work first.",
            };
            ui.label(RichText::new(text).color(p.on_surface_variant));
            dialog_buttons(ui, |ui| {
                let kind = if matches!(a, SessionAction::PowerOff | SessionAction::Reboot) { ButtonKind::Danger } else { ButtonKind::Filled };
                if widgets::button(ui, kind, None, a.label()).clicked() {
                    let verb = a.verb();
                    st.op.spawn(&ctx, move || crate::sys::cmd::run("systemctl", &[verb]).map(|_| String::new()));
                    close = true;
                }
                if widgets::button(ui, ButtonKind::Text, None, "Cancel").clicked() {
                    close = true;
                }
            });
        });
        if close || r.should_close() {
            st.confirm = None;
        }
    }
}
