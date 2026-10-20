//! Sound: output and input devices, volume and mute, through `pactl`.

use crate::app::App;
use crate::sys::audio::{self, Dir};
use crate::sys::{cmd, task::Task};
use crate::ui::common;
use crate::ui::widgets::{self, ButtonKind};
use crate::ui::{Action, Actions, ToastKind};
use egui::{pos2, vec2, Align2, CornerRadius, RichText, Sense, Ui, UiBuilder};
use egui_material_icons::{icons::*, MaterialIcon};
use std::time::{Duration, Instant};

/// Minimum gap between volume commands while a slider is dragged.
const VOLUME_INTERVAL: Duration = Duration::from_millis(80);
const REFRESH: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct State {
    data: Task<Result<audio::State, String>>,
    op: Task<Result<(), String>>,
    volume_op: Task<Result<(), String>>,
    /// Slider values being dragged, per direction (device name, percent).
    editing: [Option<(String, f32)>; 2],
    /// Latest value not yet sent.
    pending: [Option<(String, u32)>; 2],
    last_sent: Option<Instant>,
}

fn slot(dir: Dir) -> usize {
    match dir {
        Dir::Output => 0,
        Dir::Input => 1,
    }
}

/// A list row: leading icon in a tonal circle, title and subtitle, and
/// trailing controls. The whole row is clickable.
pub(crate) fn list_row(
    ui: &mut Ui,
    icon: MaterialIcon,
    active: bool,
    title: &str,
    subtitle: &str,
    trailing: impl FnOnce(&mut Ui),
) -> egui::Response {
    let p = widgets::palette(ui);
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 58.0), Sense::click());
    if response.hovered() {
        let alpha = if response.is_pointer_button_down_on() { 0.10 } else { 0.06 };
        ui.painter().rect_filled(
            rect.expand2(vec2(8.0, 0.0)),
            CornerRadius::same(14),
            p.on_surface.gamma_multiply(alpha),
        );
    }
    let (circle, fg) = if active {
        (p.primary_container, p.on_primary_container)
    } else {
        (p.surface_container_highest, p.on_surface_variant)
    };
    let c = pos2(rect.left() + 20.0, rect.center().y);
    ui.painter().circle_filled(c, 20.0, circle);
    widgets::paint_icon(ui, icon, c, 22.0, fg);

    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(0.0, 4.0)))
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    trailing(&mut child);
    let trailing_w = child.min_rect().width();

    let text_left = rect.left() + 54.0;
    let max_w = (rect.right() - trailing_w - 16.0 - text_left).max(40.0);
    let title_g = widgets::truncated_galley(ui, title, widgets::title_font(14.5), p.on_surface, max_w);
    let (ty, sy) = if subtitle.is_empty() {
        (rect.center().y - title_g.size().y / 2.0, 0.0)
    } else {
        (rect.center().y - title_g.size().y - 1.0, rect.center().y + 1.0)
    };
    ui.painter().galley(pos2(text_left, ty), title_g, p.on_surface);
    if !subtitle.is_empty() {
        let sub = widgets::truncated_galley(
            ui,
            subtitle,
            egui::FontId::proportional(12.5),
            if active { p.primary } else { p.on_surface_variant },
            max_w,
        );
        ui.painter().galley(pos2(text_left, sy), sub, p.on_surface_variant);
    }
    response
}

/// A centered hint inside an empty list.
pub(crate) fn empty_hint(ui: &mut Ui, text: &str) {
    let p = widgets::palette(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(13.0),
        p.on_surface_variant,
    );
}

fn device_icon(dev: &audio::Device, dir: Dir) -> MaterialIcon {
    let i = dev.icon.as_str();
    let n = dev.name.to_lowercase();
    if i.contains("headset") || i.contains("headphone") || n.contains("headset") {
        if dir == Dir::Input {
            ICON_HEADSET_MIC
        } else {
            ICON_HEADPHONES
        }
    } else if n.contains("hdmi") || n.contains("displayport") || i.contains("video") {
        ICON_TV
    } else if dir == Dir::Input {
        ICON_MIC
    } else {
        ICON_SPEAKER
    }
}

fn refresh(st: &mut State, ctx: &egui::Context) {
    st.data.spawn(ctx, audio::read);
}

fn start_op(
    st: &mut State,
    ctx: &egui::Context,
    f: impl FnOnce() -> Result<(), String> + Send + 'static,
) {
    st.op.spawn(ctx, f);
}

fn device_card(ui: &mut Ui, st: &mut State, ctx: &egui::Context, dir: Dir, state: &audio::State) {
    let p = widgets::palette(ui);
    let (title, empty) = match dir {
        Dir::Output => ("Output", "No output devices"),
        Dir::Input => ("Input", "No input devices"),
    };
    let devices = state.devices(dir).to_vec();
    let default = state.default_name(dir).to_string();
    let busy = st.op.running();
    common::card(ui, title, |ui| {
        ui.add_space(4.0);
        if devices.is_empty() {
            empty_hint(ui, empty);
        }
        for (i, dev) in devices.iter().enumerate() {
            if i > 0 {
                ui.separator();
            }
            let is_default = dev.name == default;
            let subtitle = if is_default {
                if dev.muted {
                    "Default · muted".to_string()
                } else {
                    format!("Default · {}%", dev.volume)
                }
            } else {
                String::new()
            };
            let resp = list_row(ui, device_icon(dev, dir), is_default, &dev.description, &subtitle, |ui| {
                let icon = if is_default {
                    ICON_RADIO_BUTTON_CHECKED
                } else {
                    ICON_RADIO_BUTTON_UNCHECKED
                };
                widgets::icon(ui, icon, 22.0, if is_default { p.primary } else { p.on_surface_variant });
            })
            .on_hover_text(&dev.name);
            if resp.clicked() && !is_default && !busy {
                let name = dev.name.clone();
                start_op(st, ctx, move || audio::set_default(dir, &name));
            }
            if is_default {
                volume_row(ui, st, ctx, dir, dev);
            }
        }
        ui.add_space(4.0);
    });
}

fn volume_row(ui: &mut Ui, st: &mut State, ctx: &egui::Context, dir: Dir, dev: &audio::Device) {
    let p = widgets::palette(ui);
    let s = slot(dir);
    ui.horizontal(|ui| {
        ui.add_space(44.0);
        let mute_icon = match (dir, dev.muted) {
            (Dir::Output, true) => ICON_VOLUME_OFF,
            (Dir::Output, false) => ICON_VOLUME_UP,
            (Dir::Input, true) => ICON_MIC_OFF,
            (Dir::Input, false) => ICON_MIC,
        };
        let tip = if dev.muted { "Unmute" } else { "Mute" };
        if widgets::icon_button_sized(ui, mute_icon, tip, !st.op.running(), 36.0, dev.muted).clicked() {
            let name = dev.name.clone();
            let muted = !dev.muted;
            start_op(st, ctx, move || audio::set_mute(dir, &name, muted));
        }
        let mut value = match &st.editing[s] {
            Some((name, v)) if *name == dev.name => *v,
            _ => dev.volume as f32,
        };
        ui.spacing_mut().slider_width = (ui.available_width() - 90.0).clamp(120.0, 420.0);
        let r = ui.add(
            egui::Slider::new(&mut value, 0.0..=150.0)
                .step_by(1.0)
                .fixed_decimals(0)
                .suffix("%")
                .trailing_fill(true),
        );
        if value > 100.0 {
            ui.label(RichText::new("boost").size(11.5).color(p.tertiary));
        }
        if r.changed() {
            st.editing[s] = Some((dev.name.clone(), value));
            st.pending[s] = Some((dev.name.clone(), value.round() as u32));
        }
        if r.drag_stopped() || (r.changed() && !r.dragged()) {
            // Final value: make sure it's sent, then let a refresh take over.
            st.last_sent = None;
        }
    });
    ui.add_space(6.0);
}

/// Send queued volume changes, rate-limited.
fn flush_volume(st: &mut State, ctx: &egui::Context, dragging: bool) {
    st.volume_op.poll();
    if st.volume_op.running() {
        ctx.request_repaint_after(VOLUME_INTERVAL);
        return;
    }
    let due = st.last_sent.is_none_or(|t| t.elapsed() >= VOLUME_INTERVAL);
    if !due {
        ctx.request_repaint_after(VOLUME_INTERVAL);
        return;
    }
    for dir in [Dir::Output, Dir::Input] {
        if let Some((name, percent)) = st.pending[slot(dir)].take() {
            st.volume_op.spawn(ctx, move || audio::set_volume(dir, &name, percent));
            st.last_sent = Some(Instant::now());
            return;
        }
    }
    // Nothing left to send and not dragging: drop the local values once the
    // device list has caught up.
    if !dragging
        && st.editing.iter().any(Option::is_some)
        && !st.data.running()
        && st.last_sent.is_some_and(|t| t.elapsed() > Duration::from_millis(300))
    {
        refresh(st, ctx);
        st.editing = [None, None];
    }
}

fn mixer_button(ui: &mut Ui, actions: &mut Actions, installed: &std::collections::HashSet<String>) {
    let mixer = ["pavucontrol", "pavucontrol-qt", "pwvucontrol"]
        .into_iter()
        .find(|m| cmd::exists(m));
    match mixer {
        Some(m) => {
            if widgets::button(ui, ButtonKind::Tonal, Some(ICON_TUNE), "Open mixer").clicked() {
                if let Err(e) = cmd::spawn_detached(m, &[]) {
                    actions.push(Action::Toast(e, ToastKind::Error));
                }
            }
        }
        None if !installed.contains("pavucontrol")
            && widgets::button(ui, ButtonKind::Outlined, Some(ICON_DOWNLOAD), "Install mixer")
                .on_hover_text("pavucontrol: per-app volumes, ports and profiles")
                .clicked() =>
        {
            actions.push(Action::Install {
                packages: vec!["pavucontrol".into()],
                aur: false,
            });
        }
        None => {}
    }
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let ctx = ui.ctx().clone();
    let installed = app.installed.clone();
    let st = &mut app.pages.sound;
    let actions = &mut app.actions;

    if let Some(result) = st.op.take() {
        if let Err(e) = result {
            actions.push(Action::Toast(e, ToastKind::Error));
        }
        refresh(st, &ctx);
    }
    if let Some(Err(e)) = st.volume_op.take() {
        actions.push(Action::Toast(e, ToastKind::Error));
    }
    let dragging = ui.input(|i| i.pointer.any_down());
    flush_volume(st, &ctx, dragging);

    common::page(ui, ICON_VOLUME_UP, "Sound", "Speakers, headphones and microphones.", |ui| {
        if !common::require(ui, actions, &installed, "pactl", "libpulse", "It talks to PipeWire (pipewire-pulse) or PulseAudio.") {
            return;
        }
        st.data.ensure(&ctx, audio::read);
        if st.data.finished_at.is_some_and(|t| t.elapsed() > REFRESH) && !st.data.running() && !dragging {
            refresh(st, &ctx);
        }
        if !st.data.running() {
            ctx.request_repaint_after(REFRESH);
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            mixer_button(ui, actions, &installed);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if common::refresh_button(ui, st.data.running() || st.op.running()) {
                    refresh(st, &ctx);
                }
            });
        });

        match st.data.value.clone() {
            None => common::loading(ui, "Looking for audio devices…"),
            Some(Err(e)) => common::banner(ui, ICON_ERROR, &format!("Couldn't read audio devices: {e}"), true, |_| {}),
            Some(Ok(state)) => {
                device_card(ui, st, &ctx, Dir::Output, &state);
                device_card(ui, st, &ctx, Dir::Input, &state);
            }
        }
    });
}
