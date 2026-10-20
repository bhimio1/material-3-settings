//! Bluetooth: adapter power and visibility, paired devices, and pairing new
//! ones, through `bluetoothctl`.

use crate::ui::common::{empty_hint, list_row};
use crate::app::App;
use crate::sys::bt::{self, Device};
use crate::sys::task::Task;
use crate::ui::common::{self, card, row};
use crate::ui::widgets::{self, ButtonKind};
use crate::ui::{Action, ToastKind};
use egui::Ui;
use egui_material_icons::{icons::*, MaterialIcon};
use std::time::Duration;

const REFRESH: Duration = Duration::from_secs(10);
const SCAN_SECONDS: u32 = 10;

#[derive(Default)]
pub struct State {
    data: Task<Result<bt::State, String>>,
    /// (busy key, result, success message).
    op: Task<(String, Result<(), String>, String)>,
    busy: Option<String>,
    scan: Task<Result<(), String>>,
}

fn device_icon(d: &Device) -> MaterialIcon {
    let i = d.icon.as_str();
    if i.starts_with("audio-headset") || i.starts_with("audio-headphones") {
        ICON_HEADPHONES
    } else if i.starts_with("audio") {
        ICON_SPEAKER
    } else if i == "input-mouse" {
        ICON_MOUSE
    } else if i == "input-keyboard" {
        ICON_KEYBOARD
    } else if i == "input-gaming" {
        ICON_SPORTS_ESPORTS
    } else if i.starts_with("phone") {
        ICON_SMARTPHONE
    } else if i == "computer" {
        ICON_COMPUTER
    } else if i.contains("watch") {
        ICON_WATCH
    } else if i == "printer" {
        ICON_PRINT
    } else if i.starts_with("video") {
        ICON_TV
    } else {
        ICON_DEVICES_OTHER
    }
}

fn run_op(
    st: &mut State,
    ctx: &egui::Context,
    key: &str,
    ok: &str,
    f: impl FnOnce() -> Result<(), String> + Send + 'static,
) {
    st.busy = Some(key.to_string());
    let key = key.to_string();
    let ok = ok.to_string();
    st.op.spawn(ctx, move || (key, f(), ok));
}

fn device_list(ui: &mut Ui, st: &mut State, ctx: &egui::Context, devices: &[Device], paired: bool) {
    for (i, d) in devices.iter().enumerate() {
        if i > 0 {
            ui.separator();
        }
        let sub = if paired {
            match (d.connected, d.battery) {
                (true, Some(b)) => format!("Connected · {b}% battery"),
                (true, None) => "Connected".into(),
                (false, _) => "Not connected".into(),
            }
        } else {
            d.address.clone()
        };
        let resp = list_row(ui, device_icon(d), d.connected, &d.name, &sub, |ui| {
            if st.busy.as_deref() == Some(d.address.as_str()) {
                ui.add(egui::Spinner::new());
                return;
            }
            let free = st.busy.is_none();
            let addr = d.address.clone();
            if !paired {
                if widgets::button_enabled(ui, ButtonKind::Tonal, None, "Pair", free).clicked() {
                    run_op(st, ctx, &d.address, &format!("Paired with {}", d.name), move || bt::pair(&addr));
                }
                return;
            }
            if widgets::icon_button(ui, ICON_DELETE, "Forget device", free).clicked() {
                run_op(st, ctx, &d.address, &format!("Removed {}", d.name), move || bt::remove(&addr));
                return;
            }
            let addr = d.address.clone();
            if d.connected {
                if widgets::button_enabled(ui, ButtonKind::Text, None, "Disconnect", free).clicked() {
                    run_op(st, ctx, &d.address, "", move || bt::disconnect(&addr));
                }
            } else if widgets::button_enabled(ui, ButtonKind::Tonal, None, "Connect", free).clicked() {
                run_op(st, ctx, &d.address, &format!("Connected to {}", d.name), move || bt::connect(&addr));
            }
        });
        if paired {
            resp.on_hover_text(&d.address);
        }
    }
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let ctx = ui.ctx().clone();
    let installed = app.installed.clone();
    let st = &mut app.pages.bluetooth;
    let actions = &mut app.actions;

    if let Some((_, result, ok)) = st.op.take() {
        st.busy = None;
        match result {
            Ok(()) if !ok.is_empty() => actions.push(Action::Toast(ok, ToastKind::Success)),
            Ok(()) => {}
            Err(e) => actions.push(Action::Toast(e, ToastKind::Error)),
        }
        st.data.spawn(&ctx, bt::read);
    }
    if let Some(result) = st.scan.take() {
        if let Err(e) = result {
            actions.push(Action::Toast(format!("Scan failed: {e}"), ToastKind::Error));
        }
        st.data.spawn(&ctx, bt::read);
    }

    common::page(ui, ICON_BLUETOOTH, "Bluetooth", "Headphones, keyboards, mice and other wireless devices.", |ui| {
        if !common::require(ui, actions, &installed, "bluetoothctl", "bluez-utils", "") {
            return;
        }
        st.data.ensure(&ctx, bt::read);
        if st.data.finished_at.is_some_and(|t| t.elapsed() > REFRESH)
            && !st.data.running()
            && st.busy.is_none()
            && !st.scan.running()
        {
            st.data.spawn(&ctx, bt::read);
        }
        ctx.request_repaint_after(REFRESH);

        let state = match st.data.value.clone() {
            None => {
                common::loading(ui, "Looking for Bluetooth adapters…");
                return;
            }
            Some(Err(e)) => {
                common::banner(ui, ICON_ERROR, &e, true, |_| {});
                return;
            }
            Some(Ok(s)) => s,
        };

        if !state.service_active {
            common::banner(ui, ICON_BLUETOOTH_DISABLED, "The Bluetooth service isn't running.", false, |ui| {
                if widgets::button(ui, ButtonKind::Filled, None, "Enable").clicked() {
                    actions.push(Action::RunInTerminal("sudo systemctl enable --now bluetooth".into()));
                }
                if common::refresh_button(ui, st.data.running()) {
                    st.data.spawn(&ctx, bt::read);
                }
            });
        }
        let Some(controller) = state.controller.clone() else {
            if state.service_active {
                common::banner(ui, ICON_BLUETOOTH_DISABLED, "No Bluetooth adapter found. If it's a USB dongle, check that it's plugged in, and that it isn't blocked (rfkill).", false, |ui| {
                    if common::refresh_button(ui, st.data.running()) {
                        st.data.spawn(&ctx, bt::read);
                    }
                });
            }
            return;
        };

        card(ui, "", |ui| {
            row(ui, "Bluetooth", &format!("Adapter {} ({})", controller.alias, controller.address), |ui| {
                let mut on = controller.powered;
                if st.busy.as_deref() == Some("power") {
                    ui.add(egui::Spinner::new());
                } else if widgets::switch(ui, &mut on).changed() && st.busy.is_none() {
                    run_op(st, &ctx, "power", "", move || bt::set_powered(on));
                }
            });
            if controller.powered {
                ui.separator();
                row(ui, "Visible to other devices", &format!("Shown as “{}” while this is on.", controller.alias), |ui| {
                    let mut on = controller.discoverable;
                    if widgets::switch(ui, &mut on).changed() && st.busy.is_none() {
                        run_op(st, &ctx, "discoverable", "", move || bt::set_discoverable(on));
                    }
                });
            }
        });
        if !controller.powered {
            return;
        }

        let paired: Vec<Device> = state.paired().cloned().collect();
        card(ui, "My devices", |ui| {
            ui.add_space(4.0);
            if paired.is_empty() {
                empty_hint(ui, "No paired devices yet");
            }
            device_list(ui, st, &ctx, &paired, true);
            ui.add_space(4.0);
        });

        let nearby: Vec<Device> = state.available().cloned().collect();
        card(ui, "Nearby devices", |ui| {
            row(ui, "Find devices", "Put the device in pairing mode first.", |ui| {
                if st.scan.running() {
                    ui.add(egui::Spinner::new());
                    ui.label("Searching…");
                } else if widgets::button_enabled(ui, ButtonKind::Tonal, Some(ICON_BLUETOOTH_SEARCHING), "Search", st.busy.is_none()).clicked() {
                    st.scan.spawn(&ctx, || bt::scan(SCAN_SECONDS));
                }
            });
            if !nearby.is_empty() {
                ui.separator();
                device_list(ui, st, &ctx, &nearby, false);
            }
            ui.add_space(4.0);
        });
    });
}
