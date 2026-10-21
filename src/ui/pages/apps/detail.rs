//! One program's config page: header with actions, file picker, quick
//! settings and the text editor.

use super::quick::{self, Change};
use super::{guess_format, State, Tab};
use crate::app::App;
use crate::catalog::{self, Kind, Quick};
use crate::files::{self, ConfigFile};
use crate::formats::{self, Format};
use material_3_ui::sys::{cmd, paths};
use material_3_ui::widgets::{self, segmented, ButtonKind};
use crate::ui::{common, editor, Action, Page, ToastKind};
use egui::{vec2, CornerRadius, Frame, Key, Margin, Modifiers, RichText, Ui};
use material_3_ui::icons::{icons::*, MaterialIcon};
use std::path::{Path, PathBuf};

struct FileSpec {
    label: String,
    path: PathBuf,
    format: Format,
    template: &'static str,
    quick: &'static [Quick],
    system: bool,
}

/// A catalog app or one of the user's own files, in one shape.
struct Target {
    key: String,
    name: String,
    icon: MaterialIcon,
    description: String,
    /// (package, aur, installed)
    package: Option<(&'static str, bool, bool)>,
    docs: &'static str,
    reload: &'static str,
    specs: Vec<FileSpec>,
    custom_index: Option<usize>,
}

fn target(app: &App, id: &str) -> Option<Target> {
    if let Some(index) = id.strip_prefix("custom:").and_then(|i| i.parse::<usize>().ok()) {
        let f = app.config.custom_files.get(index)?;
        let path = paths::expand(&f.path);
        return Some(Target {
            key: id.to_string(),
            name: f.name.clone(),
            icon: ICON_DESCRIPTION,
            description: "A config file you added.".into(),
            package: None,
            docs: "",
            reload: "",
            specs: vec![FileSpec {
                label: f.name.clone(),
                format: guess_format(&path),
                path,
                template: "",
                quick: &[],
                system: false,
            }],
            custom_index: Some(index),
        });
    }
    let def = catalog::find(id)?;
    Some(Target {
        key: id.to_string(),
        name: def.name.to_string(),
        icon: def.icon,
        description: def.description.to_string(),
        package: Some((def.package, def.aur, super::installed(def, &app.installed))),
        docs: def.docs,
        reload: def.reload,
        specs: def
            .files
            .iter()
            .map(|s| FileSpec {
                label: s.label.to_string(),
                path: s.resolve(),
                format: s.format,
                template: s.template,
                quick: s.quick,
                system: s.system,
            })
            .collect(),
        custom_index: None,
    })
}

/// Save a file: through pkexec for system files (in the background),
/// directly otherwise. Backs up once per session when enabled.
fn save(
    state: &mut State,
    ctx: &egui::Context,
    path: &Path,
    system: bool,
    backups: bool,
    actions: &mut Vec<Action>,
    announce: bool,
) {
    let Some(file) = state.files.get_mut(path) else {
        return;
    };
    if system {
        if state.system_save.running() {
            actions.push(Action::Toast("Still saving the previous change…".into(), ToastKind::Info));
            return;
        }
        let path = path.to_path_buf();
        let text = file.text.clone();
        state
            .system_save
            .spawn(ctx, move || {
                let r = cmd::pkexec_write(&path, &text);
                (path, text, r)
            });
        return;
    }
    let first = !state.backed_up.contains(path);
    match file.save(backups && first) {
        Ok(()) => {
            state.backed_up.insert(path.to_path_buf());
            if announce {
                actions.push(Action::Toast(format!("Saved {}", paths::tilde(path)), ToastKind::Success));
            }
        }
        Err(e) => actions.push(Action::Toast(format!("Couldn't save: {e}"), ToastKind::Error)),
    }
}

fn status_chip(ui: &mut Ui, text: &str, fg: egui::Color32, bg: egui::Color32) {
    Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.0).color(fg));
        });
}

pub fn show(ui: &mut Ui, app: &mut App, id: &str) {
    let p = app.palette.clone();
    let ctx = app.ctx.clone();
    let mut actions: Vec<Action> = Vec::new();

    let Some(target) = target(app, id) else {
        common::page(ui, ICON_TUNE, "Not found", "This config isn't in the catalog any more.", |ui| {
            ui.add_space(12.0);
            if widgets::button(ui, ButtonKind::Tonal, Some(ICON_ARROW_BACK), "All apps").clicked() {
                actions.push(Action::Goto(Page::Apps));
            }
        });
        app.actions.extend(actions);
        return;
    };

    let backups_enabled = app.config.backups;
    let font_size = app.config.editor_font_size;
    let state = &mut app.pages.apps;

    // Background work.
    if let Some((path, text, result)) = state.system_save.take() {
        match result {
            Ok(()) => {
                if let Some(f) = state.files.get_mut(&path) {
                    f.disk = Some(text);
                }
                actions.push(Action::Toast(format!("Saved {}", path.display()), ToastKind::Success));
            }
            Err(e) => actions.push(Action::Toast(format!("Couldn't save {}: {e}", path.display()), ToastKind::Error)),
        }
    }
    if target.specs.iter().any(|s| s.quick.iter().any(|q| q.kind == Kind::Font)) {
        state.fonts.ensure(&ctx, quick::load_fonts);
    }
    if let Some(path) = state.pending_save.clone() {
        if !ui.input(|i| i.pointer.any_down()) {
            state.pending_save = None;
            let system = target.specs.iter().any(|s| s.path == path && s.system);
            save(state, &ctx, &path, system, backups_enabled, &mut actions, false);
        }
    }

    // Which file.
    let default_spec = target.specs.iter().position(|s| s.path.exists()).unwrap_or(0);
    let spec_index = (*state.spec.entry(target.key.clone()).or_insert(default_spec)).min(target.specs.len() - 1);
    let spec = &target.specs[spec_index];
    let file = state
        .files
        .entry(spec.path.clone())
        .or_insert_with(|| ConfigFile::open(spec.path.clone(), spec.format));
    file.sync_from_disk();
    let has_quick = spec.format.structured() && !spec.quick.is_empty();
    let tab_entry = state
        .tab
        .entry(target.key.clone())
        .or_insert(if has_quick { Tab::Quick } else { Tab::Editor });
    if !has_quick {
        *tab_entry = Tab::Editor;
    }
    let mut tab = *tab_entry;

    if material_3_ui::keys::take_key(ui.ctx(), Modifiers::COMMAND, Key::S) {
        if let Err(e) = state.files[&spec.path].validate() {
            actions.push(Action::Toast(format!("Not saved — syntax error: {e}"), ToastKind::Error));
        } else {
            save(state, &ctx, &spec.path, spec.system, backups_enabled, &mut actions, true);
        }
    }

    Frame::new().inner_margin(Margin { left: 24, right: 24, top: 20, bottom: 16 }).show(ui, |ui| {
        // ---------------------------------------------------------- header
        ui.horizontal(|ui| {
            if widgets::icon_button(ui, ICON_ARROW_BACK, "All apps", true).clicked() {
                actions.push(Action::Goto(Page::Apps));
            }
            ui.add_space(4.0);
            let (rect, _) = ui.allocate_exact_size(vec2(48.0, 48.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 24.0, p.primary_container);
            widgets::paint_icon(ui, target.icon, rect.center(), 26.0, p.on_primary_container);
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.set_max_width((ui.available_width() - 360.0).max(160.0));
                ui.label(RichText::new(&target.name).size(24.0).color(p.on_surface));
                ui.add(egui::Label::new(RichText::new(&target.description).size(13.0).color(p.on_surface_variant)).wrap());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(index) = target.custom_index {
                    if widgets::button(ui, ButtonKind::Text, Some(ICON_DELETE), "Remove from list").clicked() {
                        if index < app.config.custom_files.len() {
                            app.config.custom_files.remove(index);
                        }
                        actions.push(Action::ConfigChanged);
                        actions.push(Action::Goto(Page::Apps));
                    }
                }
                if let Some(dir) = spec.path.parent() {
                    if widgets::icon_button(ui, ICON_FOLDER_OPEN, "Open the folder", dir.exists()).clicked() {
                        actions.push(Action::OpenPath(dir.to_path_buf()));
                    }
                }
                if !target.docs.is_empty()
                    && widgets::icon_button(ui, ICON_MENU_BOOK, "Documentation", true).clicked()
                {
                    actions.push(Action::OpenUrl(target.docs.to_string()));
                }
                if let Some((package, aur, installed)) = target.package {
                    if !installed {
                        let label = if aur { "Install (AUR)" } else { "Install" };
                        if widgets::button(ui, ButtonKind::Filled, Some(ICON_DOWNLOAD), label)
                            .on_hover_text(format!("Installs the `{package}` package in a terminal"))
                            .clicked()
                        {
                            actions.push(Action::Install { packages: vec![package.to_string()] });
                        }
                    } else if !target.reload.is_empty()
                        && widgets::button(ui, ButtonKind::Tonal, Some(ICON_REFRESH), "Reload app")
                            .on_hover_text(target.reload)
                            .clicked()
                    {
                        match cmd::spawn_detached("sh", &["-c", target.reload]) {
                            Ok(()) => actions.push(Action::Toast(format!("Asked {} to reload its config", target.name), ToastKind::Info)),
                            Err(e) => actions.push(Action::Toast(e, ToastKind::Error)),
                        }
                    }
                }
            });
        });

        ui.add_space(14.0);

        // ------------------------------------------------- file selector
        if target.specs.len() > 1 {
            let mut chosen = spec_index;
            let options: Vec<(usize, MaterialIcon, &str)> = target
                .specs
                .iter()
                .enumerate()
                .map(|(i, s)| (i, if s.path.exists() { ICON_DESCRIPTION } else { ICON_NOTE_ADD }, s.label.as_str()))
                .collect();
            if segmented(ui, &mut chosen, &options) {
                state.spec.insert(target.key.clone(), chosen);
                state.show_backups = false;
            }
            ui.add_space(10.0);
        }

        let file = state.files.get_mut(&spec.path).expect("opened above");
        let exists = file.exists();
        let dirty = file.dirty();

        // Path and status.
        ui.horizontal(|ui| {
            widgets::icon(ui, if spec.system { ICON_LOCK } else { ICON_DESCRIPTION }, 18.0, p.on_surface_variant);
            ui.label(RichText::new(paths::tilde(&spec.path)).monospace().size(12.5).color(p.on_surface_variant));
            if spec.system {
                status_chip(ui, "System file · saving asks for your password", p.on_tertiary_container, p.tertiary_container);
            }
            if !exists && !dirty {
                status_chip(ui, "Not created yet", p.on_surface_variant, p.surface_container_high);
            } else if dirty {
                status_chip(ui, "● Unsaved changes", p.on_tertiary_container, p.tertiary_container);
            }
            if state.system_save.running() {
                ui.add(egui::Spinner::new().color(p.primary));
            }
        });

        if let Some(e) = file.load_error.clone() {
            common::banner(ui, ICON_ERROR, &format!("Couldn't read the file: {e}"), true, |_| {});
            return;
        }

        // -------------------------------------------------- missing file
        if !exists && !dirty {
            common::card(ui, "", |ui| {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    widgets::icon(ui, ICON_NOTE_ADD, 36.0, p.primary);
                    ui.vertical(|ui| {
                        ui.label(widgets::title_text(format!("{} doesn't exist yet", spec.label), 16.0).color(p.on_surface));
                        let what = if spec.template.is_empty() {
                            "Create it to start editing.".to_string()
                        } else {
                            format!("{} uses its built-in defaults until you create one. The template is a short, commented starting point.", target.name)
                        };
                        ui.add(egui::Label::new(RichText::new(what).size(13.0).color(p.on_surface_variant)).wrap());
                    });
                });
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let mut create = None;
                    if !spec.template.is_empty()
                        && widgets::button(ui, ButtonKind::Filled, Some(ICON_AUTO_AWESOME), "Create from template").clicked()
                    {
                        create = Some(spec.template);
                    }
                    let empty_kind = if spec.template.is_empty() { ButtonKind::Filled } else { ButtonKind::Outlined };
                    if widgets::button(ui, empty_kind, Some(ICON_ADD), "Create empty file").clicked() {
                        create = Some("");
                    }
                    if let Some(text) = create {
                        if let Some(f) = state.files.get_mut(&spec.path) {
                            f.text = text.to_string();
                        }
                        save(state, &ctx, &spec.path, spec.system, false, &mut actions, true);
                        state.tab.insert(target.key.clone(), if has_quick { Tab::Quick } else { Tab::Editor });
                    }
                });
                ui.add_space(12.0);
            });
            return;
        }

        // ------------------------------------------------------- toolbar
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if has_quick
                && segmented(ui, &mut tab, &[(Tab::Quick, ICON_TUNE, "Quick settings"), (Tab::Editor, ICON_CODE, "Editor")])
            {
                state.tab.insert(target.key.clone(), tab);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let file = &state.files[&spec.path];
                let valid = file.validate();
                if tab == Tab::Editor {
                    let save_clicked = widgets::button_enabled(ui, ButtonKind::Filled, Some(ICON_SAVE), "Save", dirty && valid.is_ok())
                        .on_hover_text("Ctrl+S")
                        .clicked();
                    let revert = widgets::button_enabled(ui, ButtonKind::Text, Some(ICON_UNDO), "Revert", dirty).clicked();
                    if save_clicked {
                        save(state, &ctx, &spec.path, spec.system, backups_enabled, &mut actions, true);
                    }
                    if revert {
                        if let Some(f) = state.files.get_mut(&spec.path) {
                            f.reload();
                        }
                        state.text_bufs.clear();
                    }
                }
                let backups = !spec.system && !files::list_backups(&spec.path).is_empty();
                if backups
                    && widgets::icon_button_sized(ui, ICON_HISTORY, "Backups", true, 36.0, state.show_backups).clicked()
                {
                    state.show_backups = !state.show_backups;
                }
            });
        });

        // ------------------------------------------------------- backups
        if state.show_backups && !spec.system {
            let list = files::list_backups(&spec.path);
            common::card(ui, "Backups", |ui| {
                common::note(ui, "A copy is kept the first time a file is saved in each session. Restoring loads it into the editor; save to keep it.");
                for b in list.iter().take(10) {
                    common::row(ui, &material_3_ui::fs::backup_label(b), &b.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), |ui| {
                        if widgets::button(ui, ButtonKind::Tonal, Some(ICON_RESTORE), "Restore").clicked() {
                            match std::fs::read_to_string(b) {
                                Ok(text) => {
                                    if let Some(f) = state.files.get_mut(&spec.path) {
                                        f.text = text;
                                    }
                                    state.tab.insert(target.key.clone(), Tab::Editor);
                                    state.show_backups = false;
                                    actions.push(Action::Toast("Backup loaded — review it and save to keep it".into(), ToastKind::Info));
                                }
                                Err(e) => actions.push(Action::Toast(format!("Couldn't read backup: {e}"), ToastKind::Error)),
                            }
                        }
                    });
                }
            });
        }

        let file = state.files.get_mut(&spec.path).expect("opened above");
        if file.dirty() && file.changed_on_disk() {
            let mut reload = false;
            common::banner(ui, ICON_WARNING, "Another program changed this file. Saving will overwrite its changes.", false, |ui| {
                reload = widgets::button(ui, ButtonKind::Text, Some(ICON_REFRESH), "Load theirs").clicked();
            });
            if reload {
                file.reload();
            }
        }
        if let Err(e) = file.validate() {
            common::banner(ui, ICON_ERROR, &format!("Syntax error: {e}"), true, |_| {});
        }

        ui.add_space(10.0);
        match tab {
            Tab::Quick => {
                if file.dirty() {
                    common::banner(ui, ICON_INFO, "There are unsaved edits from the editor. Changing a setting here saves them too.", false, |_| {});
                }
                let fonts = state.fonts.value.clone();
                let mut changes: Vec<(&Quick, quick::Outcome)> = Vec::new();
                egui::ScrollArea::vertical()
                    .id_salt(("quick", &spec.path))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_max_width(ui.available_width().min(common::PAGE_WIDTH));
                        let file = &state.files[&spec.path];
                        Frame::new()
                            .fill(p.surface_container)
                            .corner_radius(CornerRadius::same(20))
                            .inner_margin(Margin::symmetric(20, 8))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                for (i, q) in spec.quick.iter().enumerate() {
                                    if i > 0 {
                                        ui.separator();
                                    }
                                    common::row(ui, q.label, q.help, |ui| {
                                        let out = quick::control(
                                            ui,
                                            q,
                                            file,
                                            fonts.as_deref(),
                                            &mut state.font_filter,
                                            &mut state.text_bufs,
                                        );
                                        if out.change.is_some() {
                                            changes.push((q, out));
                                        }
                                    });
                                }
                            });
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new("Changes are saved right away. Settings you reset are commented out, so the program uses its default.")
                                .size(12.0)
                                .color(p.on_surface_variant),
                        );
                        ui.add_space(12.0);
                    });
                for (q, out) in changes {
                    let file = state.files.get_mut(&spec.path).expect("opened above");
                    let result = match out.change.expect("filtered") {
                        Change::Set(v) => file.set(q.key, &v),
                        Change::Unset => file.unset(q.key),
                        Change::Flag(on) => match file.format {
                            Format::Kv(d) => {
                                file.text = formats::set_flag(&file.text, &d, q.key, on);
                                Ok(())
                            }
                            _ => Err("not a key/value file".into()),
                        },
                    };
                    match result {
                        Err(e) => actions.push(Action::Toast(format!("Couldn't change {}: {e}", q.label), ToastKind::Error)),
                        Ok(()) if out.dragging => state.pending_save = Some(spec.path.clone()),
                        Ok(()) => save(state, &ctx, &spec.path, spec.system, backups_enabled, &mut actions, false),
                    }
                }
            }
            Tab::Editor => {
                let file = state.files.get_mut(&spec.path).expect("opened above");
                let syntax = file.format.syntax();
                Frame::new()
                    .fill(p.surface_container)
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(Margin::same(4))
                    .show(ui, |ui| {
                        ui.set_min_size(ui.available_size());
                        editor::show(ui, egui::Id::unique(("editor", &spec.path)), &mut file.text, syntax, font_size, &p);
                    });
            }
        }
    });

    app.actions.extend(actions);
}
