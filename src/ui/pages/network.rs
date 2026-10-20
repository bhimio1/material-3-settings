//! Network: Wi-Fi on/off and networks, wired/VPN connections, airplane mode
//! and the hostname, through NetworkManager's `nmcli`.

use crate::ui::common::{empty_hint, list_row};
use crate::app::App;
use crate::sys::net::{self, Wifi};
use crate::sys::{cmd, task::Task};
use crate::ui::common::{self, card, row};
use crate::ui::widgets::{self, ButtonKind};
use crate::ui::{Action, ToastKind};
use egui::{RichText, Ui};
use egui_material_icons::{icons::*, MaterialIcon};
use std::time::Duration;

const REFRESH: Duration = Duration::from_secs(15);

#[derive(Default)]
pub struct State {
    data: Task<Result<net::State, String>>,
    wifi: Task<Result<Vec<Wifi>, String>>,
    /// (what is busy, result, success message).
    op: Task<(String, Result<(), String>, String)>,
    busy: Option<String>,
    /// SSID whose password field is open.
    password_for: Option<String>,
    password: String,
    hostname: Option<String>,
}

fn signal_icon(signal: u8) -> MaterialIcon {
    match signal {
        75.. => ICON_SIGNAL_WIFI_4_BAR,
        50..=74 => ICON_NETWORK_WIFI_3_BAR,
        25..=49 => ICON_NETWORK_WIFI_2_BAR,
        _ => ICON_NETWORK_WIFI_1_BAR,
    }
}

fn kind_icon(kind: &str) -> MaterialIcon {
    match kind {
        "802-3-ethernet" | "ethernet" => ICON_LAN,
        "vpn" | "wireguard" => ICON_VPN_KEY,
        "802-11-wireless" | "wifi" => ICON_WIFI,
        _ => ICON_SETTINGS_ETHERNET,
    }
}

fn kind_label(kind: &str) -> &str {
    match kind {
        "802-3-ethernet" | "ethernet" => "Wired",
        "vpn" => "VPN",
        "wireguard" => "WireGuard",
        "bridge" => "Bridge",
        "bond" => "Bond",
        "tun" => "Tunnel",
        "gsm" => "Mobile broadband",
        other => other,
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

fn refresh(st: &mut State, ctx: &egui::Context, rescan: bool) {
    st.data.spawn(ctx, net::read);
    st.wifi.spawn(ctx, move || net::scan(rescan));
}

fn wifi_card(ui: &mut Ui, st: &mut State, ctx: &egui::Context, state: &net::State) {
    let p = widgets::palette(ui);
    card(ui, "Wi-Fi", |ui| {
        row(ui, "Wi-Fi", if state.has_wifi_device { "Find and connect to wireless networks." } else { "No wireless adapter found." }, |ui| {
            let mut on = state.wifi_enabled;
            if widgets::switch(ui, &mut on).changed() && st.busy.is_none() {
                run_op(st, ctx, "radio", "", move || net::set_wifi(on));
            }
            if st.busy.as_deref() == Some("radio") {
                ui.add(egui::Spinner::new());
            }
        });
        if !state.wifi_enabled {
            return;
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(widgets::title_text("Networks", 13.0).color(p.on_surface_variant));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if common::refresh_button(ui, st.wifi.running()) {
                    st.wifi.spawn(ctx, || net::scan(true));
                }
            });
        });
        let list = match &st.wifi.value {
            None => {
                common::loading(ui, "Scanning…");
                return;
            }
            Some(Err(e)) => {
                empty_hint(ui, &format!("Couldn't scan: {e}"));
                return;
            }
            Some(Ok(list)) => list.clone(),
        };
        if list.is_empty() {
            empty_hint(ui, "No networks in range");
        }
        for (i, w) in list.iter().enumerate() {
            if i > 0 {
                ui.separator();
            }
            let saved = state.saved_wifi(&w.ssid).is_some();
            let busy_here = st.busy.as_deref() == Some(w.ssid.as_str());
            let mut status = Vec::new();
            if w.in_use {
                status.push("Connected".to_string());
            } else if saved {
                status.push("Saved".to_string());
            }
            status.push(if w.secured() { w.security.clone() } else { "Open".into() });
            status.push(format!("{}%", w.signal));
            let resp = list_row(ui, signal_icon(w.signal), w.in_use, &w.ssid, &status.join(" · "), |ui| {
                if busy_here {
                    ui.add(egui::Spinner::new());
                    return;
                }
                if saved && widgets::icon_button(ui, ICON_DELETE, "Forget network", st.busy.is_none()).clicked() {
                    let ssid = w.ssid.clone();
                    run_op(st, ctx, &w.ssid, "Network forgotten", move || net::forget(&ssid));
                }
                if w.in_use {
                    if widgets::button_enabled(ui, ButtonKind::Text, None, "Disconnect", st.busy.is_none()).clicked() {
                        let ssid = w.ssid.clone();
                        run_op(st, ctx, &w.ssid, "", move || net::disconnect(&ssid));
                    }
                } else if widgets::button_enabled(ui, ButtonKind::Tonal, None, "Connect", st.busy.is_none()).clicked() {
                    connect(st, ctx, w, saved);
                }
                if w.secured() {
                    widgets::icon(ui, ICON_LOCK, 16.0, widgets::palette(ui).on_surface_variant);
                }
            });
            if resp.double_clicked() && !w.in_use && st.busy.is_none() {
                connect(st, ctx, w, saved);
            }
            if st.password_for.as_deref() == Some(w.ssid.as_str()) {
                password_row(ui, st, ctx, &w.ssid);
            }
        }
    });
}

fn connect(st: &mut State, ctx: &egui::Context, w: &Wifi, saved: bool) {
    if w.secured() && !saved {
        st.password_for = Some(w.ssid.clone());
        st.password.clear();
        return;
    }
    let ssid = w.ssid.clone();
    run_op(st, ctx, &w.ssid, &format!("Connected to {}", w.ssid), move || net::connect(&ssid, None, saved));
}

fn password_row(ui: &mut Ui, st: &mut State, ctx: &egui::Context, ssid: &str) {
    ui.horizontal(|ui| {
        ui.add_space(54.0);
        let r = ui.add(
            egui::TextEdit::singleline(&mut st.password)
                .password(true)
                .hint_text("Password")
                .desired_width(240.0),
        );
        if st.password.is_empty() && !r.has_focus() {
            r.request_focus();
        }
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let ok = st.password.len() >= 8 || st.password.len() == 5 || st.password.len() == 13;
        let clicked = widgets::button_enabled(ui, ButtonKind::Filled, None, "Connect", ok).clicked();
        if (clicked || enter) && ok {
            let pw = std::mem::take(&mut st.password);
            let ssid_owned = ssid.to_string();
            st.password_for = None;
            run_op(st, ctx, ssid, &format!("Connected to {ssid}"), move || {
                net::connect(&ssid_owned, Some(&pw), false)
            });
        }
        if widgets::button(ui, ButtonKind::Text, None, "Cancel").clicked()
            || ui.input(|i| i.key_pressed(egui::Key::Escape))
        {
            st.password_for = None;
            st.password.clear();
        }
    });
    ui.add_space(6.0);
}

fn connections_card(ui: &mut Ui, st: &mut State, ctx: &egui::Context, state: &net::State) {
    let others: Vec<_> = state.connections.iter().filter(|c| !c.is_wifi()).cloned().collect();
    if others.is_empty() {
        return;
    }
    card(ui, "Wired & VPN", |ui| {
        for (i, c) in others.iter().enumerate() {
            if i > 0 {
                ui.separator();
            }
            let sub = if c.active {
                format!("{} · Connected on {}", kind_label(&c.kind), c.device)
            } else {
                kind_label(&c.kind).to_string()
            };
            let key = format!("con:{}", c.name);
            list_row(ui, kind_icon(&c.kind), c.active, &c.name, &sub, |ui| {
                if st.busy.as_deref() == Some(key.as_str()) {
                    ui.add(egui::Spinner::new());
                    return;
                }
                let name = c.name.clone();
                if c.active {
                    if widgets::button_enabled(ui, ButtonKind::Text, None, "Disconnect", st.busy.is_none()).clicked() {
                        run_op(st, ctx, &key, "", move || net::disconnect(&name));
                    }
                } else if widgets::button_enabled(ui, ButtonKind::Tonal, None, "Connect", st.busy.is_none()).clicked() {
                    run_op(st, ctx, &key, &format!("Connected to {}", c.name), move || {
                        cmd::run("nmcli", &["connection", "up", "id", &name]).map(|_| ())
                    });
                }
            });
        }
    });
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let ctx = ui.ctx().clone();
    let installed = app.installed.clone();
    let st = &mut app.pages.network;
    let actions = &mut app.actions;

    if let Some((_, result, ok)) = st.op.take() {
        st.busy = None;
        match result {
            Ok(()) if !ok.is_empty() => actions.push(Action::Toast(ok, ToastKind::Success)),
            Ok(()) => {}
            Err(e) => actions.push(Action::Toast(e, ToastKind::Error)),
        }
        refresh(st, &ctx, false);
    }

    common::page(ui, ICON_WIFI, "Network", "Wi-Fi, wired connections and VPNs, managed by NetworkManager.", |ui| {
        if !common::require(ui, actions, &installed, "nmcli", "networkmanager", "") {
            return;
        }
        if st.data.value.is_none() && !st.data.running() {
            refresh(st, &ctx, false);
        }
        st.data.poll();
        st.wifi.poll();
        if st.data.finished_at.is_some_and(|t| t.elapsed() > REFRESH)
            && !st.data.running()
            && st.busy.is_none()
            && st.password_for.is_none()
        {
            refresh(st, &ctx, false);
        }
        ctx.request_repaint_after(REFRESH);

        let state = match st.data.value.clone() {
            None => {
                common::loading(ui, "Asking NetworkManager…");
                return;
            }
            Some(Err(e)) => {
                common::banner(ui, ICON_ERROR, &format!("NetworkManager isn't responding: {e}"), true, |ui| {
                    if widgets::button(ui, ButtonKind::Tonal, None, "Start it").clicked() {
                        actions.push(Action::RunInTerminal("sudo systemctl enable --now NetworkManager".into()));
                    }
                });
                return;
            }
            Some(Ok(s)) => s,
        };

        let p = widgets::palette(ui);
        let active: Vec<_> = state.active().cloned().collect();
        ui.add_space(8.0);
        let summary = if active.is_empty() {
            "Not connected".to_string()
        } else {
            active.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
        };
        ui.horizontal(|ui| {
            widgets::icon(ui, if active.is_empty() { ICON_WIFI_OFF } else { ICON_CHECK_CIRCLE }, 18.0, if active.is_empty() { p.on_surface_variant } else { p.primary });
            ui.label(RichText::new(summary).color(p.on_surface));
        });

        if state.airplane {
            common::banner(ui, ICON_AIRPLANEMODE_ACTIVE, "Airplane mode is on. Wireless radios are off.", false, |_| {});
        }

        wifi_card(ui, st, &ctx, &state);
        connections_card(ui, st, &ctx, &state);

        card(ui, "Other", |ui| {
            row(ui, "Airplane mode", "Turn off all wireless radios managed by NetworkManager.", |ui| {
                let mut on = state.airplane;
                if widgets::switch(ui, &mut on).changed() && st.busy.is_none() {
                    run_op(st, &ctx, "airplane", "", move || net::set_airplane(on));
                }
            });
            ui.separator();
            row(ui, "Device name", "How this computer appears on the network.", |ui| {
                let text = st.hostname.get_or_insert_with(|| state.hostname.clone());
                let valid = net::valid_hostname(text.trim());
                let differs = text.trim() != state.hostname;
                if widgets::button_enabled(ui, ButtonKind::Tonal, None, "Rename", valid && differs && st.busy.is_none()).clicked() {
                    let name = text.trim().to_string();
                    st.hostname = None;
                    run_op(st, &ctx, "hostname", &format!("Renamed to {name}"), move || net::set_hostname(&name));
                }
                if let Some(text) = st.hostname.as_mut() {
                    ui.add(egui::TextEdit::singleline(text).desired_width(180.0));
                }
            });
            ui.separator();
            row(ui, "Advanced settings", "IP addresses, DNS, proxies, VPN and hotspot profiles.", |ui| {
                if cmd::exists("nm-connection-editor") {
                    if widgets::button(ui, ButtonKind::Tonal, Some(ICON_OPEN_IN_NEW), "Open editor").clicked() {
                        if let Err(e) = cmd::spawn_detached("nm-connection-editor", &[]) {
                            actions.push(Action::Toast(e, ToastKind::Error));
                        }
                    }
                } else if widgets::button(ui, ButtonKind::Outlined, Some(ICON_DOWNLOAD), "Install editor").clicked() {
                    actions.push(Action::Install { packages: vec!["nm-connection-editor".into()], aur: false });
                }
                if cmd::exists("nmtui")
                    && widgets::button(ui, ButtonKind::Text, Some(ICON_TERMINAL), "nmtui").clicked()
                {
                    actions.push(Action::RunInTerminal("nmtui".into()));
                }
            });
        });
    });
}
