//! Hyprland basics: gaps, borders, blur, input… saved in a managed block of
//! the user's config and applied with `hyprctl reload`. Plus a searchable
//! list of the current keybinds.

use crate::app::App;
use crate::hypr::{self, Bind, Block, Kind, Setting, Target, Val};
use material_3_ui::sys::{paths, task::Task};
use material_3_ui::common;
use material_3_ui::widgets::{self, ButtonKind};
use crate::ui::{toast, Action, Actions, ToastKind};
use egui::{CornerRadius, Frame, Margin, RichText, Ui};
use material_3_ui::icons::icons::*;
use std::collections::{BTreeMap, HashMap};

#[derive(Default)]
pub struct State {
    target: Option<Target>,
    /// The managed block as it is on disk.
    block: Block,
    live: Task<BTreeMap<String, Val>>,
    /// Unapplied edits. `None` removes our override for the key.
    pending: BTreeMap<String, Option<Val>>,
    drag: HashMap<String, f64>,
    text_edits: HashMap<String, String>,
    apply: Task<Result<Block, String>>,
    binds: Task<Result<Vec<Bind>, String>>,
    bind_query: String,
}

impl State {
    fn load(&mut self, ctx: &egui::Context) {
        let target = hypr::target(&paths::home());
        self.block = hypr::load_block(&target);
        self.target = Some(target);
        if hypr::running() {
            self.live.spawn(ctx, hypr::live_values);
        }
    }

    /// The value shown for a setting.
    fn value(&self, s: &Setting) -> Val {
        if let Some(p) = self.pending.get(s.key) {
            return p.clone().unwrap_or_else(|| self.live_or_default(s));
        }
        if let Some(v) = self.block.settings.get(s.key) {
            return s.coerce(v.clone());
        }
        self.live_or_default(s)
    }

    fn live_or_default(&self, s: &Setting) -> Val {
        self.live
            .value
            .as_ref()
            .and_then(|m| m.get(s.key).cloned())
            .unwrap_or_else(|| s.default_value())
    }

    /// Whether our block sets this key (after pending edits).
    fn overridden(&self, key: &str) -> bool {
        match self.pending.get(key) {
            Some(p) => p.is_some(),
            None => self.block.settings.contains_key(key),
        }
    }

    fn edit(&mut self, s: &Setting, v: Val) {
        let saved = self.block.settings.get(s.key).map(|b| s.coerce(b.clone()));
        if saved.as_ref() == Some(&v) {
            self.pending.remove(s.key);
        } else {
            self.pending.insert(s.key.to_string(), Some(v));
        }
    }

    fn reset(&mut self, key: &str) {
        if self.block.settings.contains_key(key) {
            self.pending.insert(key.to_string(), None);
        } else {
            self.pending.remove(key);
        }
        self.text_edits.remove(key);
    }

    fn merged_block(&self) -> Block {
        let mut b = self.block.clone();
        for (k, v) in &self.pending {
            match v {
                Some(v) => {
                    b.settings.insert(k.clone(), v.clone());
                }
                None => {
                    b.settings.remove(k);
                }
            }
        }
        b
    }
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let ctx = app.ctx.clone();
    let backups = app.config.backups;
    let actions = &mut app.actions;
    let st = &mut app.pages.hyprland;
    if st.target.is_none() {
        st.load(&ctx);
    }
    st.live.poll();
    if let Some(result) = st.apply.take() {
        match result {
            Ok(block) => {
                st.block = block;
                st.pending.clear();
                st.text_edits.clear();
                if hypr::running() {
                    toast(actions, "Applied", ToastKind::Success);
                    st.live.spawn(&ctx, hypr::live_values);
                } else {
                    toast(actions, "Saved. Applies the next time Hyprland starts.", ToastKind::Success);
                }
            }
            Err(e) => toast(actions, format!("Couldn't apply: {e}"), ToastKind::Error),
        }
    }
    let running = hypr::running();
    let target = st.target.clone().expect("loaded above");

    common::page(
        ui,
        ICON_WATER_DROP,
        "Hyprland",
        "Window gaps, borders, effects and input. Your changes are kept in their own block of your Hyprland config and applied with a reload.",
        |ui| {
            if !running {
                common::banner(
                    ui,
                    ICON_INFO,
                    "Hyprland isn't running in this session. You can still change settings; they're saved to your config and take effect the next time Hyprland starts.",
                    false,
                    |_| {},
                );
            }
            pending_bar(ui, st, &ctx, &target, backups, running);

            common::card(ui, "", |ui| {
                let desc = if target.end4 {
                    format!(
                        "{} — loaded after end-4's defaults, so these values win and survive dots updates.",
                        paths::tilde(&target.path)
                    )
                } else {
                    paths::tilde(&target.path)
                };
                common::row(ui, "Saved to", &desc, |ui| {
                    if widgets::button(ui, ButtonKind::Text, Some(ICON_FOLDER_OPEN), "Show").clicked() {
                        if let Some(dir) = target.path.parent() {
                            actions.push(Action::OpenPath(dir.to_path_buf()));
                        }
                    }
                    if running && common::refresh_button(ui, st.live.running()) {
                        st.load(&ctx);
                    }
                });
            });

            for (group, settings) in hypr::GROUPS {
                common::card(ui, group, |ui| {
                    for (i, s) in settings.iter().enumerate() {
                        if i > 0 {
                            ui.separator();
                        }
                        setting_row(ui, st, s);
                    }
                });
            }

            binds_card(ui, st, &ctx, running, actions);
        },
    );
}

fn pending_bar(ui: &mut Ui, st: &mut State, ctx: &egui::Context, target: &Target, backups: bool, running: bool) {
    if st.pending.is_empty() && !st.apply.running() {
        return;
    }
    let p = widgets::palette(ui);
    ui.add_space(12.0);
    Frame::new()
        .fill(p.secondary_container)
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::symmetric(16, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                widgets::icon(ui, ICON_EDIT, 20.0, p.on_secondary_container);
                let n = st.pending.len();
                ui.label(
                    RichText::new(format!("{n} unsaved change{}", if n == 1 { "" } else { "s" }))
                        .color(p.on_secondary_container),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if st.apply.running() {
                        ui.add(egui::Spinner::new().color(p.primary));
                        return;
                    }
                    if widgets::button(ui, ButtonKind::Filled, Some(ICON_CHECK), "Apply").clicked() {
                        let block = st.merged_block();
                        let target = target.clone();
                        st.apply.spawn(ctx, move || {
                            hypr::save_block(&target, &block, backups)?;
                            if running {
                                hypr::reload()?;
                                // Give Hyprland a moment before values are re-read.
                                std::thread::sleep(std::time::Duration::from_millis(250));
                            }
                            Ok(block)
                        });
                    }
                    if widgets::button(ui, ButtonKind::Text, Some(ICON_UNDO), "Revert").clicked() {
                        st.pending.clear();
                        st.text_edits.clear();
                        st.drag.clear();
                    }
                });
            });
        });
}

fn setting_row(ui: &mut Ui, st: &mut State, s: &'static Setting) {
    let value = st.value(s);
    let overridden = st.overridden(s.key);
    let pending = st.pending.contains_key(s.key);
    let mut title = s.label.to_string();
    if pending {
        title.push_str(" •");
    }
    common::row(ui, &title, s.help, |ui| {
        if overridden {
            if widgets::icon_button_sized(ui, ICON_RESTART_ALT, "Remove my override", true, 30.0, false).clicked() {
                st.reset(s.key);
            }
        } else {
            ui.add_space(30.0 + ui.spacing().item_spacing.x);
        }
        if let Some(v) = control(ui, st, s, &value) {
            st.edit(s, v);
        }
    });
}

fn control(ui: &mut Ui, st: &mut State, s: &Setting, value: &Val) -> Option<Val> {
    match s.kind {
        Kind::Bool => {
            let mut on = value.as_bool().unwrap_or(false);
            widgets::switch(ui, &mut on).changed().then_some(Val::Bool(on))
        }
        Kind::Int { min, max } => {
            let base = value.as_f64().unwrap_or(0.0);
            let mut v = st.drag.get(s.key).copied().unwrap_or(base).round() as i64;
            let resp = ui.add(egui::Slider::new(&mut v, min..=max));
            commit(st, s.key, &resp, v as f64).map(|f| Val::Int(f.round() as i64))
        }
        Kind::Float { min, max } => {
            let base = value.as_f64().unwrap_or(0.0);
            let mut v = st.drag.get(s.key).copied().unwrap_or(base);
            let resp = ui.add(egui::Slider::new(&mut v, min..=max).step_by(0.01).fixed_decimals(2));
            commit(st, s.key, &resp, v).map(|f| Val::Float((f * 100.0).round() / 100.0))
        }
        Kind::Choice(options) => {
            let cur = value.as_str();
            let label = options
                .iter()
                .find(|(v, _)| *v == cur)
                .map(|(_, l)| l.to_string())
                .unwrap_or_else(|| cur.clone());
            let mut out = None;
            egui::ComboBox::from_id_salt(("hypr", s.key))
                .width(180.0)
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for (v, l) in options {
                        if ui.selectable_label(*v == cur, *l).clicked() && *v != cur {
                            out = Some(Val::Str(v.to_string()));
                        }
                    }
                });
            out
        }
        Kind::IntChoice(options) => {
            let cur = value.as_f64().unwrap_or(0.0) as i64;
            let label = options
                .iter()
                .find(|(v, _)| *v == cur)
                .map(|(_, l)| l.to_string())
                .unwrap_or_else(|| cur.to_string());
            let mut out = None;
            egui::ComboBox::from_id_salt(("hypr", s.key))
                .width(180.0)
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for (v, l) in options {
                        if ui.selectable_label(*v == cur, *l).clicked() && *v != cur {
                            out = Some(Val::Int(*v));
                        }
                    }
                });
            out
        }
        Kind::Text => {
            let cur = value.as_str();
            let buf = st.text_edits.entry(s.key.to_string()).or_insert_with(|| cur.clone());
            let resp = ui.add(egui::TextEdit::singleline(buf).desired_width(220.0));
            if resp.lost_focus() {
                let text = buf.trim().to_string();
                st.text_edits.remove(s.key);
                (text != cur).then_some(Val::Str(text))
            } else {
                if !resp.has_focus() {
                    st.text_edits.insert(s.key.to_string(), cur);
                }
                None
            }
        }
    }
}

fn commit(st: &mut State, key: &str, resp: &egui::Response, v: f64) -> Option<f64> {
    if resp.dragged() {
        st.drag.insert(key.to_string(), v);
        None
    } else if resp.drag_stopped() || resp.changed() {
        st.drag.remove(key);
        Some(v)
    } else {
        None
    }
}

fn binds_card(ui: &mut Ui, st: &mut State, ctx: &egui::Context, running: bool, _actions: &mut Actions) {
    let p = widgets::palette(ui);
    common::card(ui, "Keybinds", |ui| {
        if !running {
            common::note(ui, "Start Hyprland to see your keybinds.");
            return;
        }
        st.binds.ensure(ctx, hypr::binds);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut st.bind_query)
                    .hint_text("Search keybinds")
                    .desired_width(ui.available_width() - 48.0),
            );
            if common::refresh_button(ui, st.binds.running()) {
                st.binds.spawn(ctx, hypr::binds);
            }
        });
        ui.add_space(6.0);
        let Some(result) = &st.binds.value else {
            common::loading(ui, "Reading keybinds…");
            return;
        };
        let binds = match result {
            Ok(b) => b,
            Err(e) => {
                common::note(ui, &format!("Couldn't read keybinds: {e}"));
                return;
            }
        };
        let q = st.bind_query.to_lowercase();
        let shown: Vec<&Bind> = binds
            .iter()
            .filter(|b| {
                q.is_empty()
                    || b.keys.to_lowercase().contains(&q)
                    || b.action.to_lowercase().contains(&q)
                    || b.description.to_lowercase().contains(&q)
            })
            .collect();
        if shown.is_empty() {
            common::note(ui, "No keybinds match.");
            return;
        }
        // A plain grid: a nested scroll area fights the page's own.
        const LIMIT: usize = 150;
        let total = shown.len();
        egui::Grid::new("hypr-binds-grid")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .show(ui, |ui| {
                for b in shown.into_iter().take(LIMIT) {
                    Frame::new()
                        .fill(p.surface_container_highest)
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::symmetric(8, 3))
                        .show(ui, |ui| {
                            ui.label(RichText::new(&b.keys).monospace().size(12.0).color(p.on_surface));
                        });
                    ui.vertical(|ui| {
                        if b.description.is_empty() {
                            ui.label(RichText::new(&b.action).color(p.on_surface));
                        } else {
                            ui.label(RichText::new(&b.description).color(p.on_surface));
                            ui.label(RichText::new(&b.action).size(11.5).color(p.on_surface_variant));
                        }
                    });
                    ui.end_row();
                }
            });
        if total > LIMIT {
            common::note(ui, &format!("{} more. Search to narrow the list.", total - LIMIT));
        }
        ui.add_space(8.0);
    });
}
