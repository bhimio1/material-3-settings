//! Packages: updates, orphans, the cache, AUR helper, pacman.conf options,
//! mirrors and package search.

use crate::app::App;
use crate::sys::arch::{self, AurHelper};
use crate::sys::pacman::{self, SearchResult, Stats, Update};
use crate::sys::task::Task;
use crate::sys::{cmd, term};
use crate::ui::common::{self, card, row};
use crate::ui::widgets::{self, ButtonKind};
use crate::ui::{toast, Action, Actions, ToastKind};
use egui::{RichText, Ui};
use egui_material_icons::icons::*;
use std::path::Path;

#[derive(Default)]
pub struct State {
    stats: Task<Result<Stats, String>>,
    updates: Task<Result<Vec<Update>, String>>,
    show_updates: bool,
    /// /etc/pacman.conf as last read or saved, and the staged edit.
    conf_disk: Option<Result<String, String>>,
    conf_edit: String,
    conf_save: Task<Result<String, String>>,
    mirrors: Option<Vec<String>>,
    query: String,
    installed_only: bool,
    search: Task<Result<Vec<SearchResult>, String>>,
    searched: String,
}

fn load_conf(st: &mut State) {
    match std::fs::read_to_string(pacman::PACMAN_CONF) {
        Ok(text) => {
            st.conf_edit = text.clone();
            st.conf_disk = Some(Ok(text));
        }
        Err(e) => st.conf_disk = Some(Err(e.to_string())),
    }
    st.mirrors = Some(
        std::fs::read_to_string(pacman::MIRRORLIST)
            .map(|t| pacman::mirrors(&t))
            .unwrap_or_default(),
    );
}

fn stat_tile(ui: &mut Ui, width: f32, value: String, label: &str, emphasis: bool) {
    let p = widgets::palette(ui);
    let (bg, fg) = if emphasis {
        (p.tertiary_container, p.on_tertiary_container)
    } else {
        (p.surface_container_high, p.on_surface)
    };
    egui::Frame::new()
        .fill(bg)
        .corner_radius(egui::CornerRadius::same(16))
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(width - 32.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(value).size(26.0).color(fg));
                ui.add(egui::Label::new(RichText::new(label).size(12.5).color(fg.gamma_multiply(0.8))).truncate());
            });
        });
}

fn helper_name(h: AurHelper) -> &'static str {
    match h {
        AurHelper::Auto => "Auto",
        AurHelper::Paru => "paru",
        AurHelper::Yay => "yay",
        AurHelper::None => "None",
    }
}

const INSTALL_PARU: &str = "sudo pacman -S --needed base-devel git && cd \"$(mktemp -d)\" && git clone https://aur.archlinux.org/paru-bin.git && cd paru-bin && makepkg -si";

pub fn show(ui: &mut Ui, app: &mut App) {
    let ctx = app.ctx.clone();
    let p = app.palette.clone();
    let helper_pref = app.config.aur_helper;
    let helper = helper_pref.resolve();
    let st = &mut app.pages.packages;
    let actions: &mut Actions = &mut app.actions;
    let has_pacman = cmd::exists("pacman");

    if has_pacman {
        st.stats.ensure(&ctx, pacman::load_stats);
    }
    if st.conf_disk.is_none() {
        load_conf(st);
    }
    st.updates.poll();
    st.search.poll();
    if let Some(result) = st.conf_save.take() {
        match result {
            Ok(text) => {
                st.conf_disk = Some(Ok(text));
                toast(actions, "Saved /etc/pacman.conf (previous version kept as pacman.conf.bak)", ToastKind::Success);
            }
            Err(e) => toast(actions, format!("Couldn't save pacman.conf: {e}"), ToastKind::Error),
        }
    }

    let mut new_helper = None;
    let os_name = app.os.pretty_name.clone();
    let cachyos = app.os.is_cachyos();
    let installed = app.installed.clone();
    common::page(ui, ICON_PACKAGE_2, "Packages", "Updates, cleanup and pacman options.", |ui| {
        if !has_pacman {
            common::banner(ui, ICON_WARNING, &format!("pacman wasn't found. This page manages Arch Linux packages{}.", if os_name.is_empty() { String::new() } else { format!("; this system is {os_name}") }), true, |_| {});
            return;
        }

        // ------------------------------------------------------ overview
        ui.add_space(16.0);
        match &st.stats.value {
            None => common::loading(ui, "Counting packages…"),
            Some(Err(e)) => common::banner(ui, ICON_ERROR, &format!("Couldn't query pacman: {e}"), true, |_| {}),
            Some(Ok(s)) => {
                let gap = 10.0;
                let avail = ui.available_width();
                let cols = ((avail + gap) / (150.0 + gap)).floor().clamp(1.0, 5.0);
                let w = (avail - gap * (cols - 1.0)) / cols - 0.5;
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                    stat_tile(ui, w, s.total.to_string(), "installed", false);
                    stat_tile(ui, w, s.explicit.to_string(), "explicit", false);
                    stat_tile(ui, w, s.foreign.len().to_string(), "AUR / foreign", false);
                    stat_tile(ui, w, s.orphans.len().to_string(), "orphans", !s.orphans.is_empty());
                    stat_tile(ui, w, pacman::format_size(s.cache_bytes), "package cache", s.cache_bytes > 5 << 30);
                });
            }
        }

        // ------------------------------------------------------- CachyOS
        if cachyos {
            card(ui, "CachyOS", |ui| {
                let repos = match &st.conf_disk {
                    Some(Ok(text)) => pacman::cachyos_repos(text),
                    _ => Vec::new(),
                };
                let desc = if repos.is_empty() {
                    "No CachyOS repositories are enabled in pacman.conf.".to_string()
                } else {
                    format!("Optimised repositories in use: {}.", repos.join(", "))
                };
                let kernel = crate::sys::cmd::run("uname", &["-r"]).unwrap_or_default();
                row(ui, "Repositories", &desc, |_| {});
                ui.separator();
                row(ui, "Kernel", kernel.trim(), |ui| {
                    tool_button(ui, actions, &installed, "cachyos-kernel-manager", "cachyos-kernel-manager", "Kernel manager");
                });
                ui.separator();
                row(ui, "CachyOS tools", "Welcome app, gaming packages and system tweaks.", |ui| {
                    tool_button(ui, actions, &installed, "cachyos-hello", "cachyos-hello", "CachyOS Hello");
                    if !installed.contains("cachyos-gaming-meta")
                        && widgets::button(ui, ButtonKind::Outlined, Some(ICON_SPORTS_ESPORTS), "Gaming packages")
                            .on_hover_text("Install cachyos-gaming-meta (Proton-CachyOS, Wine, gamescope and friends)")
                            .clicked()
                    {
                        actions.push(Action::Install { packages: vec!["cachyos-gaming-meta".into()] });
                    }
                });
                ui.add_space(4.0);
            });
        }

        // ------------------------------------------------------- updates
        card(ui, "Updates", |ui| {
            let installed = app.installed.clone();
            let has_checkupdates = cmd::exists("checkupdates");
            let status = match (&st.updates.value, st.updates.running()) {
                (_, true) => "Checking…".to_string(),
                (None, false) => if has_checkupdates { "Not checked yet." } else { "Install pacman-contrib to check for updates without touching the system database." }.to_string(),
                (Some(Err(e)), false) => format!("Couldn't check: {e}"),
                (Some(Ok(u)), false) if u.is_empty() => "Everything is up to date.".to_string(),
                (Some(Ok(u)), false) => {
                    let aur = u.iter().filter(|x| x.aur).count();
                    format!("{} update{} available{}.", u.len(), if u.len() == 1 { "" } else { "s" }, if aur > 0 { format!(" ({aur} from the AUR)") } else { String::new() })
                }
            };
            row(ui, "System update", &status, |ui| {
                if widgets::button(ui, ButtonKind::Filled, Some(ICON_SYSTEM_UPDATE_ALT), "Update now").clicked() {
                    actions.push(Action::RunInTerminal(arch::upgrade_command(helper_pref)));
                }
                if has_checkupdates {
                    if st.updates.running() {
                        ui.add(egui::Spinner::new().color(p.primary));
                    } else if widgets::button(ui, ButtonKind::Tonal, Some(ICON_REFRESH), "Check").clicked() {
                        st.updates.spawn(&ctx, move || pacman::load_updates(helper));
                        st.show_updates = true;
                    }
                } else if !installed.contains("pacman-contrib")
                    && widgets::button(ui, ButtonKind::Tonal, Some(ICON_DOWNLOAD), "pacman-contrib").clicked()
                {
                    actions.push(Action::Install { packages: vec!["pacman-contrib".into()] });
                }
            });
            if let Some(Ok(list)) = &st.updates.value {
                if !list.is_empty() {
                    ui.horizontal(|ui| {
                        if widgets::button(ui, ButtonKind::Text, Some(if st.show_updates { ICON_EXPAND_LESS } else { ICON_EXPAND_MORE }), if st.show_updates { "Hide list" } else { "Show list" }).clicked() {
                            st.show_updates = !st.show_updates;
                        }
                    });
                    if st.show_updates {
                        egui::ScrollArea::vertical().id_salt("updates").max_height(240.0).show(ui, |ui| {
                            egui::Grid::new("update-grid").num_columns(3).spacing([18.0, 4.0]).striped(false).show(ui, |ui| {
                                for u in list {
                                    ui.label(RichText::new(&u.name).color(p.on_surface));
                                    ui.label(RichText::new(format!("{} → {}", u.old, u.new)).size(12.5).color(p.on_surface_variant));
                                    ui.label(RichText::new(if u.aur { "AUR" } else { "" }).size(12.0).color(p.tertiary));
                                    ui.end_row();
                                }
                            });
                        });
                        ui.add_space(8.0);
                    }
                }
            }
        });

        // ------------------------------------------------------- cleanup
        let mut refresh_stats = false;
        if let Some(Ok(s)) = &st.stats.value {
            let busy = st.stats.running();
            card(ui, "Cleanup", |ui| {
                let orphans_desc = if s.orphans.is_empty() {
                    "No orphaned dependencies.".to_string()
                } else {
                    let preview: Vec<&str> = s.orphans.iter().take(6).map(String::as_str).collect();
                    format!("Dependencies nothing needs anymore: {}{}", preview.join(", "), if s.orphans.len() > 6 { format!(" and {} more", s.orphans.len() - 6) } else { String::new() })
                };
                row(ui, "Orphaned packages", &orphans_desc, |ui| {
                    if widgets::button_enabled(ui, ButtonKind::Tonal, Some(ICON_DELETE_SWEEP), "Remove", !s.orphans.is_empty()).clicked() {
                        let list: Vec<String> = s.orphans.iter().map(|o| term::shell_quote(o)).collect();
                        actions.push(Action::RunInTerminal(format!("sudo pacman -Rns {}", list.join(" "))));
                    }
                });
                ui.separator();
                let has_paccache = cmd::exists("paccache");
                row(
                    ui,
                    "Package cache",
                    &format!(
                        "{} in {} files at {}. {}",
                        pacman::format_size(s.cache_bytes),
                        s.cache_files,
                        pacman::CACHE_DIR,
                        if has_paccache { "Cleaning keeps the two newest versions of each package." } else { "Cleaning removes cached versions of packages that aren't installed." }
                    ),
                    |ui| {
                        if widgets::button(ui, ButtonKind::Tonal, Some(ICON_CLEANING_SERVICES), "Clean").clicked() {
                            actions.push(Action::RunInTerminal(if has_paccache { "sudo paccache -rk2 && sudo paccache -ruk0".into() } else { "sudo pacman -Sc".into() }));
                        }
                    },
                );
                ui.separator();
                row(ui, "Refresh", "Recount packages and the cache.", |ui| {
                    if common::refresh_button(ui, busy) {
                        refresh_stats = true;
                        actions.push(Action::RefreshPackages);
                    }
                });
            });
        }
        if refresh_stats {
            st.stats.spawn(&ctx, pacman::load_stats);
        }

        // ---------------------------------------------------------- AUR
        card(ui, "AUR helper", |ui| {
            let found: Vec<&str> = ["paru", "yay"].into_iter().filter(|h| cmd::exists(h)).collect();
            let desc = if found.is_empty() {
                "No AUR helper installed.".to_string()
            } else {
                format!("Installed: {}.{}", found.join(", "), helper.map(|h| format!(" Using {h}.")).unwrap_or_default())
            };
            row(ui, "Helper", &desc, |ui| {
                let mut value = helper_pref;
                let options = [AurHelper::Auto, AurHelper::Paru, AurHelper::Yay, AurHelper::None]
                    .map(|h| {
                        let icon = match h {
                            AurHelper::Auto => ICON_AUTO_AWESOME,
                            AurHelper::None => ICON_BLOCK,
                            _ => ICON_PACKAGE_2,
                        };
                        (h, icon, helper_name(h))
                    });
                if widgets::segmented(ui, &mut value, &options) {
                    new_helper = Some(value);
                }
            });
            if found.is_empty() {
                ui.separator();
                row(ui, "Install paru", "AUR-only apps need a helper. This builds paru-bin from the AUR with makepkg (needs base-devel and git).", |ui| {
                    if widgets::button(ui, ButtonKind::Filled, Some(ICON_DOWNLOAD), "Install").clicked() {
                        actions.push(Action::RunInTerminal(INSTALL_PARU.into()));
                    }
                });
            }
        });

        // --------------------------------------------------- pacman.conf
        card(ui, "pacman options", |ui| match st.conf_disk.clone() {
            None => {}
            Some(Err(e)) => common::note(ui, &format!("Couldn't read {}: {e}", pacman::PACMAN_CONF)),
            Some(Ok(disk)) => {
                for (key, title, desc) in pacman::FLAGS {
                    let mut on = pacman::flag(&st.conf_edit, key);
                    row(ui, title, &format!("{desc} ({key})"), |ui| {
                        if widgets::switch(ui, &mut on).changed() {
                            st.conf_edit = pacman::set_flag(&st.conf_edit, key, on);
                        }
                    });
                    ui.separator();
                }
                let current = pacman::parallel_downloads(&st.conf_edit);
                row(ui, "Parallel downloads", "Download several packages at once (ParallelDownloads).", |ui| {
                    let mut n = current.unwrap_or(1);
                    if ui.add(egui::Slider::new(&mut n, 1..=20)).changed() {
                        st.conf_edit = pacman::set_parallel_downloads(&st.conf_edit, Some(n));
                    }
                });
                ui.separator();
                let mut multilib = pacman::repo_enabled(&st.conf_edit, "multilib");
                row(ui, "multilib repository", "32-bit libraries, needed for Steam, Wine and some drivers.", |ui| {
                    if widgets::switch(ui, &mut multilib).changed() {
                        st.conf_edit = pacman::set_repo_enabled(&st.conf_edit, "multilib", multilib);
                    }
                });
                let dirty = st.conf_edit != disk;
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let saving = st.conf_save.running();
                    if widgets::button_enabled(ui, ButtonKind::Filled, Some(ICON_SAVE), "Save (asks for password)", dirty && !saving).clicked() {
                        let text = st.conf_edit.clone();
                        st.conf_save.spawn(&ctx, move || cmd::pkexec_write(Path::new(pacman::PACMAN_CONF), &text).map(|_| text));
                    }
                    if widgets::button_enabled(ui, ButtonKind::Text, Some(ICON_UNDO), "Discard", dirty && !saving).clicked() {
                        st.conf_edit = disk.clone();
                    }
                    if saving {
                        ui.add(egui::Spinner::new().color(p.primary));
                    } else if dirty {
                        ui.label(RichText::new("Unsaved changes").color(p.tertiary));
                    }
                    if dirty && multilib && !pacman::repo_enabled(&disk, "multilib") {
                        ui.label(RichText::new("Run a system update afterwards to sync multilib.").size(12.5).color(p.on_surface_variant));
                    }
                });
                ui.add_space(8.0);
            }
        });

        // ------------------------------------------------------- mirrors
        card(ui, "Mirrors", |ui| {
            let mirrors = st.mirrors.clone().unwrap_or_default();
            let desc = if mirrors.is_empty() {
                "No active Server lines in the mirrorlist.".to_string()
            } else {
                format!("{} active mirror{}. The first ones are tried first.", mirrors.len(), if mirrors.len() == 1 { "" } else { "s" })
            };
            row(ui, pacman::MIRRORLIST, &desc, |ui| {
                if cmd::exists("cachyos-rate-mirrors") {
                    // Ranks the Arch and every CachyOS mirrorlist in one go.
                    if widgets::button(ui, ButtonKind::Tonal, Some(ICON_SPEED), "Rank mirrors").clicked() {
                        actions.push(Action::RunInTerminal("sudo cachyos-rate-mirrors".into()));
                    }
                } else if cmd::exists("reflector") {
                    if widgets::button(ui, ButtonKind::Tonal, Some(ICON_SPEED), "Rank mirrors").clicked() {
                        actions.push(Action::RunInTerminal("sudo reflector --latest 20 --protocol https --sort rate --save /etc/pacman.d/mirrorlist".into()));
                    }
                } else if widgets::button(ui, ButtonKind::Outlined, Some(ICON_DOWNLOAD), "Install reflector").clicked() {
                    actions.push(Action::Install { packages: vec!["reflector".into()] });
                }
                if widgets::icon_button(ui, ICON_REFRESH, "Reload", true).clicked() {
                    load_conf(st);
                }
            });
            for m in mirrors.iter().take(5) {
                ui.label(RichText::new(m).monospace().size(12.0).color(p.on_surface_variant));
            }
            ui.add_space(8.0);
        });

        // -------------------------------------------------------- search
        card(ui, "Find packages", |ui| {
            ui.add_space(10.0);
            let mut go = false;
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut st.query).hint_text("Search the repositories").desired_width((ui.available_width() - 260.0).max(160.0)));
                go |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if widgets::filter_chip(ui, "Installed only", st.installed_only, None).clicked() {
                    st.installed_only = !st.installed_only;
                    go = true;
                }
                go |= widgets::button_enabled(ui, ButtonKind::Tonal, Some(ICON_SEARCH), "Search", !st.query.trim().is_empty()).clicked();
            });
            if go && !st.query.trim().is_empty() {
                let q = st.query.clone();
                let local = st.installed_only;
                st.searched = q.clone();
                st.search.spawn(&ctx, move || pacman::search(&q, local));
            }
            if st.search.running() {
                common::loading(ui, "Searching…");
            } else if let Some(result) = &st.search.value {
                match result {
                    Err(e) => common::note(ui, e),
                    Ok(list) if list.is_empty() => common::note(ui, &format!("Nothing found for “{}”.", st.searched)),
                    Ok(list) => {
                        ui.add_space(6.0);
                        ui.label(RichText::new(format!("{} result{}", list.len(), if list.len() == 1 { "" } else { "s" })).size(12.5).color(p.on_surface_variant));
                        egui::ScrollArea::vertical().id_salt("pkg-search").max_height(420.0).show(ui, |ui| {
                            for (i, r) in list.iter().take(200).enumerate() {
                                if i > 0 {
                                    ui.separator();
                                }
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.set_max_width((ui.available_width() - 130.0).max(160.0));
                                        ui.horizontal(|ui| {
                                            ui.label(widgets::title_text(&r.name, 14.0).color(p.on_surface));
                                            ui.label(RichText::new(format!("{} · {}", r.version, r.repo)).size(12.0).color(p.on_surface_variant));
                                        });
                                        ui.add(egui::Label::new(RichText::new(&r.description).size(12.5).color(p.on_surface_variant)).wrap());
                                    });
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if r.installed {
                                            if widgets::button(ui, ButtonKind::Text, Some(ICON_DELETE), "Remove").clicked() {
                                                actions.push(Action::RunInTerminal(format!("sudo pacman -Rs {}", term::shell_quote(&r.name))));
                                            }
                                        } else if widgets::button(ui, ButtonKind::Tonal, Some(ICON_DOWNLOAD), "Install").clicked() {
                                            actions.push(Action::Install { packages: vec![r.name.clone()] });
                                        }
                                    });
                                });
                            }
                        });
                    }
                }
            }
            ui.add_space(8.0);
        });
    });

    if let Some(h) = new_helper {
        app.config.aur_helper = h;
        app.actions.push(Action::ConfigChanged);
    }
}

/// Opens a CachyOS tool, or offers to install it.
fn tool_button(
    ui: &mut Ui,
    actions: &mut Actions,
    installed: &std::collections::HashSet<String>,
    program: &str,
    package: &str,
    label: &str,
) {
    if cmd::exists(program) {
        if widgets::button(ui, ButtonKind::Tonal, Some(ICON_OPEN_IN_NEW), label).clicked() {
            if let Err(e) = cmd::spawn_detached(program, &[]) {
                toast(actions, e, ToastKind::Error);
            }
        }
    } else if !installed.contains(package)
        && widgets::button(ui, ButtonKind::Outlined, Some(ICON_DOWNLOAD), label).clicked()
    {
        actions.push(Action::Install { packages: vec![package.to_string()] });
    }
}
