//! App configs: a catalog of programs with known config files, and a page
//! per program with quick settings and a text editor.

mod detail;
mod quick;

use crate::app::App;
use crate::catalog::{self, AppDef, Category};
use crate::config::CustomFile;
use crate::files::ConfigFile;
use crate::formats::{kv, Format, Syntax};
use crate::sys::task::Task;
use crate::sys::{cmd, paths};
use crate::ui::widgets::{self, filter_chip, ButtonKind};
use crate::ui::{common, Action, Page};
use egui::{pos2, vec2, Align2, CornerRadius, Id, RichText, Ui};
use egui_material_icons::{icons::*, MaterialIcon};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Configured,
    NotConfigured,
    NotInstalled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tab {
    Quick,
    Editor,
}

#[derive(Default)]
pub struct State {
    query: String,
    category: Option<Category>,
    installed_only: bool,
    /// The "Add a config file" form, when open: (name, path).
    adding: Option<(String, String)>,
    /// Status per catalog entry, refreshed every few seconds.
    status: Option<(Instant, Vec<Status>)>,

    /// Open files by path, so unsaved edits survive page switches.
    pub(super) files: HashMap<PathBuf, ConfigFile>,
    /// Selected file per app id.
    pub(super) spec: HashMap<String, usize>,
    pub(super) tab: HashMap<String, Tab>,
    /// Text fields being typed into, by "path\0key", committed on focus loss.
    pub(super) text_bufs: HashMap<String, String>,
    pub(super) font_filter: String,
    pub(super) fonts: Task<Vec<String>>,
    /// Root-owned saves running through pkexec: (path, text, result).
    pub(super) system_save: Task<(PathBuf, String, Result<(), String>)>,
    /// A slider is being dragged; save once it's released.
    pub(super) pending_save: Option<PathBuf>,
    /// Files already backed up this session (one backup per session).
    pub(super) backed_up: HashSet<PathBuf>,
    pub(super) show_backups: bool,
}

/// Guess how to treat a file the user added themselves.
pub fn guess_format(path: &Path) -> Format {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "toml" => Format::Toml,
        "json" | "jsonc" | "json5" => Format::Json,
        "ini" | "desktop" | "kvconfig" => Format::Kv(kv::Dialect::INI),
        "lua" => Format::Text(Syntax::Lua),
        "yml" | "yaml" => Format::Text(Syntax::Yaml),
        "css" | "rasi" | "scss" => Format::Text(Syntax::Css),
        "sh" | "bash" | "zsh" | "fish" => Format::Text(Syntax::Shell),
        "kdl" | "xml" | "xsl" => Format::Text(Syntax::Plain),
        _ if name.ends_with("rc") || name.starts_with(".profile") || name.ends_with("profile") => {
            Format::Text(Syntax::Shell)
        }
        _ => Format::Text(Syntax::Hash),
    }
}

fn compute_status(installed: &HashSet<String>) -> Vec<Status> {
    catalog::APPS
        .iter()
        .map(|a| {
            if a.files.iter().any(|s| s.resolve().exists()) {
                Status::Configured
            } else if a.is_installed(installed) {
                Status::NotConfigured
            } else {
                Status::NotInstalled
            }
        })
        .collect()
}

fn matches(app: &AppDef, query: &str) -> bool {
    let q = query.to_lowercase();
    q.is_empty()
        || app.name.to_lowercase().contains(&q)
        || app.id.contains(&q)
        || app.description.to_lowercase().contains(&q)
        || app.package.contains(&q)
}

const CARD_W: f32 = 252.0;
const CARD_H: f32 = 104.0;
const GAP: f32 = 12.0;

/// Lay cards out in a wrapping grid.
fn grid<T>(ui: &mut Ui, items: &[T], mut card: impl FnMut(&mut Ui, &T)) {
    let avail = ui.available_width();
    let cols = (((avail + GAP) / (CARD_W + GAP)).floor() as usize).max(1);
    let w = ((avail - GAP * (cols as f32 - 1.0)) / cols as f32).max(160.0);
    for chunk in items.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for item in chunk {
                ui.allocate_ui(vec2(w, CARD_H), |ui| {
                    ui.set_width(w);
                    card(ui, item);
                });
            }
        });
        ui.add_space(GAP - ui.spacing().item_spacing.y);
    }
}

/// A catalog card: icon, name, description, status line.
fn app_card(
    ui: &mut Ui,
    id: Id,
    icon: MaterialIcon,
    name: &str,
    description: &str,
    status: (&str, egui::Color32, bool),
) -> egui::Response {
    let p = widgets::palette(ui);
    let w = ui.available_width();
    let (rect, response) = widgets::clickable_card(ui, vec2(w, CARD_H), p.surface_container, id);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let c = pos2(rect.left() + 34.0, rect.top() + 34.0);
        painter.circle_filled(c, 20.0, p.secondary_container);
        widgets::paint_icon(ui, icon, c, 22.0, p.on_secondary_container);
        let text_x = rect.left() + 66.0;
        let name_g = widgets::truncated_galley(ui, name, widgets::title_font(15.0), p.on_surface, rect.right() - text_x - 12.0);
        ui.painter().galley(pos2(text_x, rect.top() + 16.0), name_g, p.on_surface);
        let (label, color, dot) = status;
        let y = rect.top() + 38.0;
        if dot {
            ui.painter().circle_filled(pos2(text_x + 4.0, y + 8.0), 4.0, color);
        }
        ui.painter().text(
            pos2(text_x + if dot { 14.0 } else { 0.0 }, y + 8.0),
            Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.0),
            color,
        );
        let mut job = egui::text::LayoutJob::single_section(
            description.to_string(),
            egui::TextFormat::simple(egui::FontId::proportional(12.5), p.on_surface_variant),
        );
        job.wrap = egui::text::TextWrapping {
            max_width: rect.width() - 32.0,
            max_rows: 2,
            break_anywhere: false,
            overflow_character: Some('…'),
        };
        let desc = ui.painter().layout_job(job);
        ui.painter().galley(pos2(rect.left() + 16.0, rect.top() + 62.0), desc, p.on_surface_variant);
    }
    response
}

pub fn show_catalog(ui: &mut Ui, app: &mut App) {
    let p = app.palette.clone();
    let state = &mut app.pages.apps;
    let mut actions: Vec<Action> = Vec::new();

    let stale = state
        .status
        .as_ref()
        .is_none_or(|(t, _)| t.elapsed() > Duration::from_secs(3));
    if stale {
        state.status = Some((Instant::now(), compute_status(&app.installed)));
    }
    let statuses = state.status.as_ref().map(|(_, s)| s.clone()).unwrap_or_default();

    let configured = statuses.iter().filter(|s| **s == Status::Configured).count();
    let subtitle = format!(
        "Edit and create config files for {} programs — {configured} already configured on this system.",
        catalog::APPS.len()
    );

    common::page(ui, ICON_TUNE, "App configs", &subtitle, |ui| {
        ui.add_space(16.0);
        // Search
        egui::Frame::new()
            .fill(p.surface_container_high)
            .corner_radius(CornerRadius::same(24))
            .inner_margin(egui::Margin::symmetric(14, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    widgets::icon(ui, ICON_SEARCH, 20.0, p.on_surface_variant);
                    ui.add(
                        egui::TextEdit::singleline(&mut state.query)
                            .hint_text("Search apps")
                            .frame(egui::Frame::NONE)
                            .desired_width(ui.available_width() - 36.0),
                    );
                    if !state.query.is_empty()
                        && widgets::icon_button_sized(ui, ICON_CLOSE, "Clear", true, 24.0, false).clicked()
                    {
                        state.query.clear();
                    }
                });
            });
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            if filter_chip(ui, "All", state.category.is_none(), None).clicked() {
                state.category = None;
            }
            for cat in Category::ALL {
                let on = state.category == Some(cat);
                if filter_chip(ui, cat.short(), on, None).clicked() {
                    state.category = if on { None } else { Some(cat) };
                }
            }
            ui.add_space(8.0);
            if filter_chip(ui, "Installed", state.installed_only, Some(ICON_FILTER_LIST)).clicked() {
                state.installed_only = !state.installed_only;
            }
        });

        let mut any = false;
        for cat in Category::ALL {
            if state.category.is_some_and(|c| c != cat) {
                continue;
            }
            let entries: Vec<(usize, &AppDef)> = catalog::APPS
                .iter()
                .enumerate()
                .filter(|(i, a)| {
                    a.category == cat
                        && matches(a, &state.query)
                        && (!state.installed_only
                            || statuses.get(*i).is_some_and(|s| *s != Status::NotInstalled))
                })
                .collect();
            if entries.is_empty() {
                continue;
            }
            any = true;
            ui.add_space(20.0);
            ui.label(widgets::title_text(cat.label(), 14.0).color(p.primary));
            ui.add_space(8.0);
            grid(ui, &entries, |ui, (i, a)| {
                let status = match statuses.get(*i) {
                    Some(Status::Configured) => ("Configured", p.primary, true),
                    Some(Status::NotConfigured) => ("Not configured yet", p.on_surface_variant, false),
                    _ => ("Not installed", p.on_surface_variant.gamma_multiply(0.7), false),
                };
                if app_card(ui, Id::unique(("app-card", a.id)), a.icon, a.name, a.description, status).clicked() {
                    actions.push(Action::Goto(Page::App(a.id.to_string())));
                }
            });
        }

        // The user's own files.
        let custom: Vec<(usize, CustomFile)> = app
            .config
            .custom_files
            .iter()
            .cloned()
            .enumerate()
            .filter(|(_, f)| {
                let q = state.query.to_lowercase();
                q.is_empty() || f.name.to_lowercase().contains(&q) || f.path.to_lowercase().contains(&q)
            })
            .collect();
        if state.category.is_none() && !state.installed_only {
            any = true;
            ui.add_space(20.0);
            ui.label(widgets::title_text("Your files", 14.0).color(p.primary));
            ui.add_space(8.0);
            enum Item {
                File(usize, CustomFile),
                Add,
            }
            let mut items: Vec<Item> = custom.into_iter().map(|(i, f)| Item::File(i, f)).collect();
            items.push(Item::Add);
            grid(ui, &items, |ui, item| match item {
                Item::File(i, f) => {
                    let path = paths::expand(&f.path);
                    let status = if path.exists() {
                        ("Exists", p.primary, true)
                    } else {
                        ("Will be created", p.on_surface_variant, false)
                    };
                    if app_card(ui, Id::unique(("custom-card", i)), ICON_DESCRIPTION, &f.name, &paths::tilde(&path), status).clicked() {
                        actions.push(Action::Goto(Page::App(format!("custom:{i}"))));
                    }
                }
                Item::Add => {
                    if app_card(
                        ui,
                        Id::unique("custom-add"),
                        ICON_NOTE_ADD,
                        "Add a config file",
                        "Any file you edit often, from dotfiles to scripts.",
                        ("", p.on_surface_variant, false),
                    )
                    .clicked()
                    {
                        state.adding = Some(Default::default());
                    }
                }
            });
            if let Some((name, path)) = &mut state.adding {
                let mut close = false;
                common::card(ui, "Add a config file", |ui| {
                    common::row(ui, "Path", "~ is expanded. The file is created if it doesn't exist.", |ui| {
                        ui.add(egui::TextEdit::singleline(path).hint_text("~/.config/app/config").desired_width(300.0));
                    });
                    common::row(ui, "Name", "Shown on the card.", |ui| {
                        let hint = Path::new(path.trim())
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        ui.add(egui::TextEdit::singleline(name).hint_text(hint).desired_width(300.0));
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let valid = !path.trim().is_empty() && !paths::expand(path.trim()).is_dir();
                        if widgets::button_enabled(ui, ButtonKind::Filled, Some(ICON_ADD), "Add", valid).clicked() {
                            let path_s = path.trim().to_string();
                            let name_s = if name.trim().is_empty() {
                                Path::new(&path_s)
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| path_s.clone())
                            } else {
                                name.trim().to_string()
                            };
                            app.config.custom_files.push(CustomFile { name: name_s, path: path_s });
                            actions.push(Action::ConfigChanged);
                            actions.push(Action::Goto(Page::App(format!(
                                "custom:{}",
                                app.config.custom_files.len() - 1
                            ))));
                            close = true;
                        }
                        if widgets::button(ui, ButtonKind::Text, None, "Cancel").clicked() {
                            close = true;
                        }
                    });
                    ui.add_space(8.0);
                });
                if close {
                    state.adding = None;
                }
            }
        }

        if !any {
            ui.add_space(32.0);
            ui.vertical_centered(|ui| {
                widgets::icon(ui, ICON_SEARCH, 40.0, p.on_surface_variant);
                ui.label(RichText::new("No apps match").color(p.on_surface_variant));
            });
        }
    });
    app.actions.extend(actions);
}

pub fn show_app(ui: &mut Ui, app: &mut App, id: &str) {
    detail::show(ui, app, id);
}

/// Is a program available at all (used for the install button)?
pub(super) fn installed(def: &AppDef, packages: &HashSet<String>) -> bool {
    def.is_installed(packages) || cmd::exists(def.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_guessing() {
        assert_eq!(guess_format(Path::new("/x/config.toml")), Format::Toml);
        assert_eq!(guess_format(Path::new("/x/settings.jsonc")), Format::Json);
        assert_eq!(guess_format(Path::new("/x/init.lua")), Format::Text(Syntax::Lua));
        assert_eq!(guess_format(Path::new("/home/u/.zshrc")), Format::Text(Syntax::Shell));
        assert_eq!(guess_format(Path::new("/x/kitty.conf")), Format::Text(Syntax::Hash));
    }
}
