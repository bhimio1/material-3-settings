//! Displays: resolution, refresh rate, scale, rotation and position per
//! monitor, through Hyprland. "Try" applies a rule for this session; "Keep"
//! also saves it in the managed block of the Hyprland config.

use crate::app::App;
use crate::hypr::{self, Monitor, MonitorRule};
use material_3_ui::sys::{cmd, paths, task::Task};
use material_3_ui::common;
use material_3_ui::widgets::{self, ButtonKind};
use crate::ui::{toast, ToastKind};
use egui::{RichText, Ui};
use material_3_ui::icons::icons::*;
use std::collections::HashMap;

const SCALES: &[f64] = &[0.5, 0.75, 1.0, 1.25, 1.333333, 1.5, 1.6, 1.666667, 1.75, 2.0, 2.25, 2.5, 3.0];

const TRANSFORMS: &[(i64, &str)] = &[
    (0, "Normal"),
    (1, "90°"),
    (2, "180°"),
    (3, "270°"),
    (4, "Flipped"),
    (5, "Flipped 90°"),
    (6, "Flipped 180°"),
    (7, "Flipped 270°"),
];

#[derive(Default)]
pub struct State {
    monitors: Task<Result<Vec<Monitor>, String>>,
    /// Working copies being edited, by output name.
    edits: HashMap<String, MonitorRule>,
    saved: Option<hypr::Block>,
    job: Task<Result<(String, Option<hypr::Block>), String>>,
    /// Read-only listing when not on Hyprland.
    other: Task<Option<String>>,
}

fn format_scale(s: f64) -> String {
    let t = format!("{s:.6}");
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn rule_from(m: &Monitor) -> MonitorRule {
    MonitorRule {
        output: m.name.clone(),
        mode: m.mode(),
        position: format!("{}x{}", m.x, m.y),
        scale: format_scale(m.scale),
        transform: m.transform,
        disabled: m.disabled,
    }
}

fn other_listing() -> Option<String> {
    if cmd::exists("wlr-randr") {
        if let Ok(out) = cmd::run("wlr-randr", &[]) {
            return Some(out);
        }
    }
    if std::env::var_os("DISPLAY").is_some() && cmd::exists("xrandr") {
        if let Ok(out) = cmd::run("xrandr", &["--query"]) {
            return Some(out);
        }
    }
    None
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let ctx = app.ctx.clone();
    let backups = app.config.backups;
    let actions = &mut app.actions;
    let st = &mut app.pages.display;
    let running = hypr::running();
    let target = hypr::target(&paths::home());

    if let Some(result) = st.job.take() {
        match result {
            Ok((msg, block)) => {
                toast(actions, msg, ToastKind::Success);
                if let Some(b) = block {
                    st.saved = Some(b);
                }
            }
            Err(e) => toast(actions, e, ToastKind::Error),
        }
        st.monitors.spawn(&ctx, hypr::monitors);
    }
    if st.saved.is_none() {
        st.saved = Some(hypr::load_block(&target));
    }

    common::page(
        ui,
        ICON_MONITOR,
        "Displays",
        "Resolution, refresh rate, scaling and arrangement. Try a change first; keep it to save it in your Hyprland config.",
        |ui| {
            if !running {
                common::banner(
                    ui,
                    ICON_INFO,
                    "This page configures monitors through Hyprland, which isn't running in this session. On other Wayland compositors use wlr-randr or your compositor's settings; on X11, xrandr.",
                    false,
                    |_| {},
                );
                st.other.ensure(&ctx, other_listing);
                if let Some(Some(text)) = &st.other.value {
                    common::card(ui, "Connected outputs", |ui| {
                        ui.add_space(8.0);
                        ui.label(RichText::new(text.trim()).monospace().size(12.0));
                        ui.add_space(8.0);
                    });
                }
                return;
            }

            st.monitors.ensure(&ctx, hypr::monitors);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Saved rules go to {}", paths::tilde(&target.path)))
                        .size(12.5)
                        .color(widgets::palette(ui).on_surface_variant),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if common::refresh_button(ui, st.monitors.running() || st.job.running()) {
                        st.edits.clear();
                        st.monitors.spawn(&ctx, hypr::monitors);
                    }
                });
            });

            let monitors = match &st.monitors.value {
                None => {
                    common::loading(ui, "Reading monitors…");
                    return;
                }
                Some(Err(e)) => {
                    common::banner(ui, ICON_ERROR, &format!("Couldn't read monitors: {e}"), true, |_| {});
                    return;
                }
                Some(Ok(m)) => m.clone(),
            };
            if monitors.is_empty() {
                common::note(ui, "No monitors reported by Hyprland.");
            }
            for m in &monitors {
                monitor_card(ui, st, &ctx, m, &target, backups);
            }
        },
    );
}

fn monitor_card(ui: &mut Ui, st: &mut State, ctx: &egui::Context, m: &Monitor, target: &hypr::Target, backups: bool) {
    let p = widgets::palette(ui);
    let live = rule_from(m);
    let saved_rule = st
        .saved
        .as_ref()
        .and_then(|b| b.monitors.iter().find(|r| r.output == m.name).cloned());
    let busy = st.job.running();
    let title = if m.description.is_empty() {
        m.name.clone()
    } else {
        format!("{} · {}", m.name, m.description)
    };
    let edit = st.edits.entry(m.name.clone()).or_insert_with(|| live.clone());

    common::card(ui, &title, |ui| {
        common::row(ui, "Enabled", "", |ui| {
            let mut on = !edit.disabled;
            if widgets::switch(ui, &mut on).changed() {
                edit.disabled = !on;
            }
        });
        ui.add_enabled_ui(!edit.disabled, |ui| {
            ui.separator();
            common::row(ui, "Resolution & refresh rate", if m.vrr { "Variable refresh rate is on." } else { "" }, |ui| {
                let mut modes = m.modes.clone();
                if !modes.contains(&edit.mode) {
                    modes.insert(0, edit.mode.clone());
                }
                egui::ComboBox::from_id_salt(("mode", m.name.as_str()))
                    .width(220.0)
                    .selected_text(pretty_mode(&edit.mode))
                    .show_ui(ui, |ui| {
                        for mode in &modes {
                            if ui.selectable_label(*mode == edit.mode, pretty_mode(mode)).clicked() {
                                edit.mode = mode.clone();
                            }
                        }
                    });
            });
            ui.separator();
            common::row(ui, "Scale", "", |ui| {
                let cur: f64 = edit.scale.parse().unwrap_or(1.0);
                let mut scales: Vec<f64> = SCALES.to_vec();
                if !scales.iter().any(|s| (s - cur).abs() < 1e-3) {
                    scales.push(cur);
                    scales.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                }
                egui::ComboBox::from_id_salt(("scale", m.name.as_str()))
                    .width(120.0)
                    .selected_text(format!("{:.0}%", cur * 100.0))
                    .show_ui(ui, |ui| {
                        for s in scales {
                            if ui.selectable_label((s - cur).abs() < 1e-3, format!("{:.0}%", s * 100.0)).clicked() {
                                edit.scale = format_scale(s);
                            }
                        }
                    });
            });
            ui.separator();
            common::row(ui, "Rotation", "", |ui| {
                let label = TRANSFORMS
                    .iter()
                    .find(|(t, _)| *t == edit.transform)
                    .map(|(_, l)| *l)
                    .unwrap_or("Normal");
                egui::ComboBox::from_id_salt(("transform", m.name.as_str()))
                    .width(140.0)
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        for (t, l) in TRANSFORMS {
                            if ui.selectable_label(*t == edit.transform, *l).clicked() {
                                edit.transform = *t;
                            }
                        }
                    });
            });
            ui.separator();
            common::row(ui, "Position", "Top-left corner in the layout, like 1920x0, or auto.", |ui| {
                ui.add(egui::TextEdit::singleline(&mut edit.position).desired_width(120.0));
            });
        });
        ui.separator();

        let changed = *edit != live;
        let desc = match &saved_rule {
            Some(r) => format!("Saved rule: {}", r.conf_value()),
            None => "Not saved in your config; Hyprland's default rule applies.".to_string(),
        };
        let mut try_rule = None;
        let mut keep_rule = None;
        let mut forget = false;
        common::row(ui, "", &desc, |ui| {
            if widgets::button_enabled(ui, ButtonKind::Filled, Some(ICON_CHECK), "Keep", !busy && (changed || saved_rule.as_ref() != Some(&*edit))).clicked() {
                keep_rule = Some(edit.clone());
            }
            if widgets::button_enabled(ui, ButtonKind::Tonal, Some(ICON_VISIBILITY), "Try", !busy && changed).clicked() {
                try_rule = Some(edit.clone());
            }
            if changed && widgets::button(ui, ButtonKind::Text, Some(ICON_UNDO), "Reset").clicked() {
                *edit = live.clone();
            }
            if saved_rule.is_some()
                && widgets::icon_button(ui, ICON_DELETE, "Remove the saved rule", !busy).clicked()
            {
                forget = true;
            }
        });
        if busy {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().color(p.primary));
            });
        }

        let syntax = target.syntax;
        if let Some(rule) = try_rule {
            st.job.spawn(ctx, move || {
                hypr::apply_monitor(&rule, syntax)?;
                Ok((format!("Applied to {} for this session", rule.output), None))
            });
        } else if let Some(rule) = keep_rule {
            let mut block = st.saved.clone().unwrap_or_default();
            let target = target.clone();
            st.job.spawn(ctx, move || {
                block.set_monitor(rule.clone());
                hypr::save_block(&target, &block, backups)?;
                // A reload picks the saved rule up if applying it live fails.
                if hypr::apply_monitor(&rule, syntax).is_err() {
                    hypr::reload()?;
                }
                Ok((format!("Saved {}", rule.output), Some(block)))
            });
        } else if forget {
            let mut block = st.saved.clone().unwrap_or_default();
            let name = m.name.clone();
            let target = target.clone();
            st.job.spawn(ctx, move || {
                block.monitors.retain(|r| r.output != name);
                hypr::save_block(&target, &block, backups)?;
                let _ = hypr::reload();
                Ok((format!("Removed the rule for {name}"), Some(block)))
            });
        }
    });
}

/// `2560x1440@143.97` → `2560 × 1440 · 143.97 Hz`.
fn pretty_mode(mode: &str) -> String {
    match mode.split_once('@') {
        Some((res, hz)) => format!("{} · {} Hz", res.replace('x', " × "), hz),
        None => mode.replace('x', " × "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting() {
        assert_eq!(format_scale(1.0), "1");
        assert_eq!(format_scale(1.333333), "1.333333");
        assert_eq!(format_scale(1.25), "1.25");
        assert_eq!(pretty_mode("1920x1080@60.00"), "1920 × 1080 · 60.00 Hz");
    }
}
