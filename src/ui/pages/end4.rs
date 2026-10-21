//! end4-pC / illogical-impulse: which shell is installed and active, and
//! every option in the shared `~/.config/illogical-impulse/config.json`.

use crate::app::App;
use crate::end4::{self, Control, Opt, Status};
use crate::files::ConfigFile;
use crate::formats::Format;
use material_3_ui::sys::{paths, task::Task};
use material_3_ui::common;
use material_3_ui::widgets::{self, ButtonKind};
use crate::ui::{toast, Action, Actions, ToastKind};
use egui::{vec2, CornerRadius, RichText, Sense, Ui};
use material_3_ui::icons::{icons::*, MaterialIcon};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct State {
    status: Task<Status>,
    /// Install / update / restart, reported as a toast.
    job: Task<Result<String, String>>,
    job_label: String,
    file: Option<ConfigFile>,
    parsed: Option<Value>,
    parse_error: Option<String>,
    last_sync: Option<Instant>,
    /// One backup per session is enough; options change one at a time.
    backed_up: bool,
    section: String,
    query: String,
    /// Text being typed into string / JSON fields, by key.
    text_edits: HashMap<String, String>,
    /// Slider / drag values while being dragged, by key.
    num_edits: HashMap<String, f64>,
}

fn home() -> PathBuf {
    paths::home()
}

impl State {
    fn reparse(&mut self) {
        let Some(file) = &self.file else { return };
        if !file.exists() {
            self.parsed = None;
            self.parse_error = None;
            return;
        }
        match crate::formats::jsonc::parse(&file.text) {
            Ok(v) => {
                self.parsed = Some(v);
                self.parse_error = None;
            }
            Err(e) => {
                self.parsed = None;
                self.parse_error = Some(e);
            }
        }
    }

    fn sync(&mut self) {
        if self.file.is_none() {
            self.file = Some(ConfigFile::open(end4::config_path(&home()), Format::Json));
            self.reparse();
            self.last_sync = Some(Instant::now());
            return;
        }
        if self.last_sync.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
            self.last_sync = Some(Instant::now());
            let file = self.file.as_mut().expect("opened above");
            let missing_before = !file.exists();
            if file.sync_from_disk() || (missing_before && file.changed_on_disk()) {
                file.reload();
                self.reparse();
            }
        }
    }

    fn current(&self, opt: &Opt) -> (Value, bool) {
        match self.parsed.as_ref().and_then(|v| end4::lookup(v, &opt.key)) {
            Some(v) => (v.clone(), true),
            None => (opt.default.clone(), false),
        }
    }

    fn write(&mut self, opt: &Opt, value: Value, backups: bool, actions: &mut Actions) {
        let Some(file) = self.file.as_mut() else { return };
        let result = file.set(&opt.path(), &value).and_then(|_| {
            let r = file.save(backups && !self.backed_up);
            if r.is_ok() {
                self.backed_up = true;
            }
            r
        });
        match result {
            Ok(()) => self.reparse(),
            Err(e) => {
                file.reload();
                toast(actions, format!("Couldn't save {}: {e}", opt.key), ToastKind::Error);
            }
        }
    }

    fn start_job(&mut self, ctx: &egui::Context, label: &str, f: impl FnOnce() -> Result<String, String> + Send + 'static) {
        self.job_label = label.to_string();
        self.job.spawn(ctx, f);
    }
}

/// A small status pill.
fn status_chip(ui: &mut Ui, icon: MaterialIcon, text: &str, on: bool) {
    let p = widgets::palette(ui);
    let (bg, fg) = if on {
        (p.secondary_container, p.on_secondary_container)
    } else {
        (p.surface_container_highest, p.on_surface_variant)
    };
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), widgets::title_font(12.5), fg);
    let size = vec2(galley.size().x + 40.0, 28.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(8), bg);
    widgets::paint_icon(ui, icon, egui::pos2(rect.left() + 17.0, rect.center().y), 16.0, fg);
    ui.painter().galley(
        egui::pos2(rect.left() + 30.0, rect.center().y - galley.size().y / 2.0),
        galley,
        fg,
    );
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let ctx = app.ctx.clone();
    let backups = app.config.backups;
    let actions = &mut app.actions;
    let st = &mut app.pages.end4;

    st.status.ensure(&ctx, || end4::detect(&paths::home()));
    if let Some(result) = st.job.take() {
        match result {
            Ok(msg) => toast(actions, msg, ToastKind::Success),
            Err(e) => toast(actions, format!("{} failed: {e}", st.job_label), ToastKind::Error),
        }
        st.status.spawn(&ctx, || end4::detect(&paths::home()));
    }
    st.sync();
    if st.file.as_ref().is_some_and(|f| !f.exists()) || st.job.running() {
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    common::page(
        ui,
        ICON_AUTO_AWESOME,
        "end4-pC / illogical-impulse",
        "Settings for end-4's illogical-impulse shell (ii) and pctrade's end4-pC, its most popular fork. Both read the same config.json, so everything here applies to whichever one is running. Changes apply live.",
        |ui| {
            status_card(ui, st, &ctx, backups, actions);
            if st.file.as_ref().is_some_and(|f| !f.exists()) {
                common::banner(
                    ui,
                    ICON_INFO,
                    "~/.config/illogical-impulse/config.json doesn't exist yet. The shell creates it the first time it starts, or you can create it now with the default values.",
                    false,
                    |ui| {
                        if widgets::button(ui, ButtonKind::Filled, Some(ICON_ADD), "Create").clicked() {
                            let text = serde_json::to_string_pretty(&end4::default_config()).unwrap_or_default() + "\n";
                            if let Some(file) = st.file.as_mut() {
                                file.text = text;
                                match file.save(false) {
                                    Ok(()) => toast(actions, "config.json created", ToastKind::Success),
                                    Err(e) => toast(actions, format!("Couldn't create config.json: {e}"), ToastKind::Error),
                                }
                            }
                            st.reparse();
                        }
                    },
                );
                return;
            }
            if let Some(e) = st.parse_error.clone() {
                common::banner(
                    ui,
                    ICON_ERROR,
                    &format!("config.json has a syntax error, so options can't be shown: {e}"),
                    true,
                    |ui| {
                        if widgets::button(ui, ButtonKind::Tonal, Some(ICON_REFRESH), "Reload").clicked() {
                            if let Some(f) = st.file.as_mut() {
                                f.reload();
                            }
                            st.reparse();
                        }
                    },
                );
                return;
            }

            common::card(ui, "Essentials", |ui| {
                let opts: Vec<&Opt> = end4::ESSENTIALS.iter().filter_map(|k| end4::find(k)).collect();
                for (i, opt) in opts.iter().enumerate() {
                    if i > 0 {
                        ui.separator();
                    }
                    option_row(ui, st, opt, false, backups, actions);
                }
            });

            all_options(ui, st, backups, actions);
        },
    );
}

fn status_card(ui: &mut Ui, st: &mut State, ctx: &egui::Context, backups: bool, actions: &mut Actions) {
    let p = widgets::palette(ui);
    common::card(ui, "Shell", |ui| {
        let Some(status) = st.status.value.clone() else {
            common::loading(ui, "Looking for illogical-impulse…");
            return;
        };
        let busy = st.job.running();
        let active = status.active_name().map(str::to_string);

        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            status_chip(ui, ICON_DEPLOYED_CODE, if status.ii.installed { "ii installed" } else { "ii not installed" }, status.ii.installed);
            status_chip(ui, ICON_DEPLOYED_CODE, if status.fork.installed { "end4-pC installed" } else { "end4-pC not installed" }, status.fork.installed);
            match &active {
                Some(a) => status_chip(ui, ICON_STAR, &format!("Starts {a}"), true),
                None => status_chip(ui, ICON_STAR, "qsConfig not found", false),
            }
            if status.running.is_empty() {
                status_chip(ui, ICON_STOP_CIRCLE, "Not running", false);
            } else {
                status_chip(ui, ICON_PLAY_CIRCLE, &format!("Running {}", status.running.join(", ")), true);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if common::refresh_button(ui, st.status.running()) {
                    st.status.spawn(ctx, || end4::detect(&paths::home()));
                }
            });
        });
        ui.add_space(4.0);

        // end4-pC
        ui.separator();
        let fork_desc = if status.fork.installed {
            format!(
                "Fork of pctrade's end4-pC with Material 3 Expressive styling, desktop widgets, a preset gallery, and these Material 3 apps built in.{}",
                status.fork.commit.as_ref().map(|c| format!(" Version {c}.")).unwrap_or_default()
            )
        } else {
            "Fork of pctrade's end4-pC with Material 3 Expressive styling, desktop widgets, a preset gallery, and these Material 3 apps built in. It needs illogical-impulse installed and keeps its own folder, so nothing of your current setup is changed.".to_string()
        };
        common::row(ui, "end4-pC", &fork_desc, |ui| {
            if status.fork.installed {
                if active.as_deref() != Some(end4::FORK)
                    && widgets::button_enabled(ui, ButtonKind::Tonal, Some(ICON_STAR), "Make default", !busy).clicked()
                {
                    make_default(st, ctx, end4::FORK, backups);
                }
                if widgets::button_enabled(ui, ButtonKind::Text, Some(ICON_DOWNLOAD), "Update", !busy).clicked() {
                    st.start_job(ctx, "Update", || end4::update_shell(&paths::home(), end4::FORK));
                }
            } else if widgets::button_enabled(ui, ButtonKind::Filled, Some(ICON_DOWNLOAD), "Install", !busy).clicked() {
                st.start_job(ctx, "Install", || end4::clone_fork(&paths::home()));
            }
            if widgets::icon_button(ui, ICON_OPEN_IN_NEW, "Project page", true).clicked() {
                actions.push(Action::OpenUrl(end4::FORK_URL.into()));
            }
        });

        // ii
        ui.separator();
        let ii_desc = if status.ii.installed {
            "end-4's original shell from dots-hyprland.".to_string()
        } else {
            "end-4's original shell. Install the dots-hyprland dotfiles to get it.".to_string()
        };
        common::row(ui, "illogical-impulse (ii)", &ii_desc, |ui| {
            if status.ii.installed
                && active.as_deref() != Some(end4::UPSTREAM)
                && widgets::button_enabled(ui, ButtonKind::Tonal, Some(ICON_STAR), "Make default", !busy).clicked()
            {
                make_default(st, ctx, end4::UPSTREAM, backups);
            }
            if widgets::icon_button(ui, ICON_OPEN_IN_NEW, "Project page", true).clicked() {
                actions.push(Action::OpenUrl(end4::UPSTREAM_URL.into()));
            }
        });

        // Running shell
        ui.separator();
        let current = status.current_shell();
        let where_from = status
            .active
            .as_ref()
            .map(|(n, f)| format!("Hyprland starts {n} (set in {}).", paths::tilde(f)))
            .unwrap_or_else(|| "No qsConfig variable found in your Hyprland config.".into());
        common::row(ui, "Running shell", &where_from, |ui| {
            if widgets::button_enabled(ui, ButtonKind::Tonal, Some(ICON_RESTART_ALT), "Restart", !busy).clicked() {
                let name = current.clone();
                st.start_job(ctx, "Restart", move || end4::restart_shell(&name).map(|_| format!("Restarted {name}")));
            }
            if widgets::button_enabled(ui, ButtonKind::Text, Some(ICON_TUNE), "Shell settings", !busy).clicked() {
                let name = current.clone();
                st.start_job(ctx, "Opening shell settings", move || {
                    end4::open_shell_settings(&paths::home(), &name).map(|_| String::new())
                });
            }
        });

        ui.separator();
        let path = end4::config_path(&paths::home());
        common::row(ui, "Configuration file", &paths::tilde(&path), |ui| {
            if widgets::button(ui, ButtonKind::Text, Some(ICON_FOLDER_OPEN), "Show").clicked() {
                if let Some(dir) = path.parent() {
                    actions.push(Action::OpenPath(dir.to_path_buf()));
                }
            }
        });
        if busy {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().color(p.primary));
                ui.label(RichText::new(format!("{}…", st.job_label)).color(p.on_surface_variant));
            });
            ui.add_space(8.0);
        }
    });
}

fn make_default(st: &mut State, ctx: &egui::Context, name: &'static str, backups: bool) {
    st.start_job(ctx, "Changing the default shell", move || {
        let path = end4::set_default_shell(&paths::home(), name, backups)?;
        let _ = material_3_ui::sys::cmd::run("hyprctl", &["reload"]);
        Ok(format!(
            "{name} is now the default shell ({}). Restart the shell to switch now.",
            paths::tilde(&path)
        ))
    });
}

fn all_options(ui: &mut Ui, st: &mut State, backups: bool, actions: &mut Actions) {
    let p = widgets::palette(ui);
    let sections = end4::sections();
    if st.section.is_empty() || !sections.contains(&st.section.as_str()) {
        st.section = "appearance".into();
    }
    common::card(ui, "All options", |ui| {
        ui.add_space(10.0);
        egui::Frame::new()
            .fill(p.surface_container_high)
            .corner_radius(CornerRadius::same(22))
            .inner_margin(egui::Margin::symmetric(12, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    widgets::icon(ui, ICON_SEARCH, 20.0, p.on_surface_variant);
                    ui.add(
                        egui::TextEdit::singleline(&mut st.query)
                            .hint_text(format!("Search {} options", end4::schema().len()))
                            .frame(egui::Frame::NONE)
                            .desired_width(ui.available_width() - 40.0),
                    );
                    if !st.query.is_empty() && widgets::icon_button_sized(ui, ICON_CLOSE, "Clear", true, 28.0, false).clicked() {
                        st.query.clear();
                    }
                });
            });
        ui.add_space(8.0);

        let query = st.query.trim().to_string();
        let opts: Vec<&Opt> = if query.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                for s in &sections {
                    let label = end4::humanize(s);
                    if widgets::filter_chip(ui, &label, st.section == *s, None).clicked() {
                        st.section = s.to_string();
                    }
                }
            });
            ui.add_space(6.0);
            end4::schema().iter().filter(|o| o.section == st.section).collect()
        } else {
            let matcher = SkimMatcherV2::default();
            let mut scored: Vec<(i64, &Opt)> = end4::schema()
                .iter()
                .filter_map(|o| {
                    let hay = format!("{} {} {}", o.label, o.key, o.comment);
                    matcher.fuzzy_match(&hay, &query).map(|s| (s, o))
                })
                .collect();
            scored.sort_by_key(|s| std::cmp::Reverse(s.0));
            scored.into_iter().take(60).map(|(_, o)| o).collect()
        };

        if opts.is_empty() {
            common::note(ui, "No options match.");
            return;
        }
        let searching = !query.is_empty();
        let mut last_group: Option<String> = None;
        for opt in opts {
            let group = if searching {
                String::new()
            } else {
                opt.group.clone()
            };
            if last_group.as_ref() != Some(&group) {
                if !group.is_empty() {
                    ui.add_space(14.0);
                    ui.label(widgets::title_text(&group, 13.0).color(p.primary));
                } else if last_group.is_some() {
                    ui.separator();
                }
                last_group = Some(group);
            } else {
                ui.separator();
            }
            option_row(ui, st, opt, true, backups, actions);
        }
        ui.add_space(8.0);
    });
}

fn value_label(v: &Value) -> String {
    match v {
        Value::String(s) if s.is_empty() => "\"\"".into(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// One option: label, description and raw key on the left, control and a
/// reset button on the right.
fn option_row(ui: &mut Ui, st: &mut State, opt: &Opt, show_key: bool, backups: bool, actions: &mut Actions) {
    let p = widgets::palette(ui);
    let (current, _set) = st.current(opt);
    let differs = !end4::same_value(&current, &opt.default);
    let mut new_value: Option<Value> = None;

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width((ui.available_width() - 300.0).max(200.0));
            let title = if !show_key && !opt.group.is_empty() {
                format!("{} › {}", opt.group, opt.label)
            } else if !show_key && opt.section != "general" && opt.group.is_empty() {
                format!("{} › {}", end4::humanize(&opt.section), opt.label)
            } else {
                opt.label.clone()
            };
            ui.label(RichText::new(title).size(14.5).color(p.on_surface));
            // Enum comments only list the choices, which the combo box shows.
            let listing = matches!(opt.control, Control::IntEnum(_) | Control::StrEnum(_));
            if !opt.comment.is_empty() && !listing {
                ui.add(egui::Label::new(RichText::new(&opt.comment).size(12.5).color(p.on_surface_variant)).wrap());
            }
            if show_key {
                ui.label(RichText::new(&opt.key).monospace().size(11.0).color(p.outline));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if differs {
                if widgets::icon_button_sized(ui, ICON_RESTART_ALT, &format!("Reset to {}", value_label(&opt.default)), true, 30.0, false).clicked() {
                    st.text_edits.retain(|k, _| !k.starts_with(&format!("{}#", opt.key)));
                    new_value = Some(opt.default.clone());
                }
            } else {
                ui.add_space(30.0 + ui.spacing().item_spacing.x);
            }
            if let Some(v) = control(ui, st, opt, &current, show_key) {
                new_value = Some(v);
            }
        });
    });
    ui.add_space(6.0);

    if let Some(v) = new_value {
        if !end4::same_value(&v, &current) {
            st.write(opt, v, backups, actions);
        }
    }
}

fn control(ui: &mut Ui, st: &mut State, opt: &Opt, current: &Value, in_list: bool) -> Option<Value> {
    // The same option can appear in Essentials and in All options.
    let id = ("end4-opt", opt.key.as_str(), in_list);
    let ekey = format!("{}#{in_list}", opt.key);
    match &opt.control {
        Control::Bool => {
            let mut on = current.as_bool().unwrap_or(false);
            widgets::switch(ui, &mut on).changed().then_some(Value::Bool(on))
        }
        Control::IntEnum(options) => {
            let cur = current.as_i64().unwrap_or(0);
            let label = options
                .iter()
                .find(|(n, _)| *n == cur)
                .map(|(_, l)| l.clone())
                .unwrap_or_else(|| cur.to_string());
            let mut out = None;
            egui::ComboBox::from_id_salt(id).width(200.0).selected_text(label).show_ui(ui, |ui| {
                for (n, l) in options {
                    if ui.selectable_label(*n == cur, l).clicked() && *n != cur {
                        out = Some(Value::from(*n));
                    }
                }
            });
            out
        }
        Control::StrEnum(options) => {
            let cur = current.as_str().unwrap_or("").to_string();
            let mut out = None;
            egui::ComboBox::from_id_salt(id)
                .width(200.0)
                .selected_text(if cur.is_empty() { "—".to_string() } else { cur.clone() })
                .show_ui(ui, |ui| {
                    let mut all = options.clone();
                    if !cur.is_empty() && !all.contains(&cur) {
                        all.insert(0, cur.clone());
                    }
                    for o in &all {
                        if ui.selectable_label(*o == cur, o).clicked() && *o != cur {
                            out = Some(Value::String(o.clone()));
                        }
                    }
                });
            out
        }
        Control::Int { min, max, slider } => {
            let base = current.as_f64().unwrap_or(0.0);
            let mut v = st.num_edits.get(&ekey).copied().unwrap_or(base).round() as i64;
            let resp = if *slider {
                ui.add(egui::Slider::new(&mut v, *min..=*max))
            } else {
                ui.add(egui::DragValue::new(&mut v).range(*min..=*max).speed(drag_speed(base)))
            };
            number_commit(st, &ekey, &resp, v as f64).map(|f| Value::from(f.round() as i64))
        }
        Control::Real { min, max, slider } => {
            let base = current.as_f64().unwrap_or(0.0);
            let mut v = st.num_edits.get(&ekey).copied().unwrap_or(base);
            let resp = if *slider {
                ui.add(egui::Slider::new(&mut v, *min..=*max).step_by(0.01))
            } else {
                ui.add(
                    egui::DragValue::new(&mut v)
                        .range(*min..=*max)
                        .speed(drag_speed(base) / 10.0)
                        .max_decimals(3),
                )
            };
            number_commit(st, &ekey, &resp, v).map(|f| {
                serde_json::Number::from_f64((f * 1000.0).round() / 1000.0)
                    .map(Value::Number)
                    .unwrap_or(Value::from(0))
            })
        }
        Control::Text => {
            let cur = current.as_str().unwrap_or("").to_string();
            let buf = st.text_edits.entry(ekey.clone()).or_insert_with(|| cur.clone());
            let resp = ui.add(egui::TextEdit::singleline(buf).desired_width(220.0).hint_text("(empty)"));
            if resp.lost_focus() {
                let text = st.text_edits.remove(&ekey).unwrap_or_default();
                (text != cur).then_some(Value::String(text))
            } else {
                if !resp.has_focus() {
                    // Follow external changes while not editing.
                    st.text_edits.insert(ekey.clone(), cur);
                }
                None
            }
        }
        Control::Json => {
            let cur = serde_json::to_string(current).unwrap_or_default();
            let buf = st.text_edits.entry(ekey.clone()).or_insert_with(|| cur.clone());
            let valid = serde_json::from_str::<Value>(buf).is_ok();
            let p = widgets::palette(ui);
            let resp = ui.add(
                egui::TextEdit::singleline(buf)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(260.0)
                    .text_color(if valid { p.on_surface } else { p.error }),
            );
            let resp = resp.on_hover_text("JSON. Press Enter to save.");
            if resp.lost_focus() {
                let text = st.text_edits.remove(&ekey).unwrap_or_default();
                match serde_json::from_str::<Value>(&text) {
                    Ok(v) if v != *current => Some(v),
                    Ok(_) => None,
                    Err(_) => {
                        // Keep the broken text so it can be fixed.
                        st.text_edits.insert(ekey.clone(), text);
                        None
                    }
                }
            } else {
                if !resp.has_focus() && valid {
                    st.text_edits.insert(ekey.clone(), cur);
                }
                None
            }
        }
    }
}

fn drag_speed(base: f64) -> f64 {
    (base.abs() / 50.0).clamp(0.1, 50.0)
}

/// Hold the value while dragging; commit when released or typed.
fn number_commit(st: &mut State, key: &str, resp: &egui::Response, v: f64) -> Option<f64> {
    if resp.dragged() {
        st.num_edits.insert(key.to_string(), v);
        return None;
    }
    if resp.drag_stopped() {
        st.num_edits.remove(key);
        return Some(v);
    }
    if resp.changed() {
        st.num_edits.remove(key);
        return Some(v);
    }
    None
}
