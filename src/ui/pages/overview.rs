//! Start page: greeting, quick tiles into every page, the desktop setup,
//! and which app configs exist.

use crate::app::App;
use crate::catalog;
use crate::sys::power::{self, PowerSupplies};
use crate::sys::task::Task;
use crate::sys::{paths, sysinfo};
use crate::ui::widgets::{self, clickable_card, paint_icon, truncated_galley, ButtonKind};
use crate::ui::{Action, Page};
use chrono::Timelike;
use egui::{pos2, vec2, Color32, FontId, Id, Rect, RichText, Sense, Ui};
use egui_material_icons::{icons::*, MaterialIcon};
use std::time::Duration;

#[derive(Clone, Default)]
struct Desktop {
    hyprland_running: bool,
    hyprland_version: Option<String>,
    end4pc: bool,
    ii: bool,
    configured: Vec<&'static str>,
    hostname: String,
    uptime: u64,
}

fn detect() -> Desktop {
    let qs = paths::config_home().join("quickshell");
    let configured = catalog::APPS
        .iter()
        .filter(|a| {
            a.files
                .iter()
                .flat_map(|f| f.paths.iter())
                .any(|p| paths::expand(p).exists())
        })
        .map(|a| a.id)
        .collect();
    Desktop {
        hyprland_running: std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some(),
        hyprland_version: sysinfo::hyprland_version(),
        end4pc: qs.join("end4-pC").is_dir(),
        ii: qs.join("ii").is_dir(),
        configured,
        hostname: std::fs::read_to_string("/proc/sys/kernel/hostname")
            .unwrap_or_default()
            .trim()
            .to_string(),
        uptime: sysinfo::parse_uptime(&std::fs::read_to_string("/proc/uptime").unwrap_or_default())
            .unwrap_or(0),
    }
}

#[derive(Default)]
pub struct State {
    desktop: Task<Desktop>,
    supplies: Task<PowerSupplies>,
}

fn greeting() -> &'static str {
    match chrono::Local::now().hour() {
        5..=11 => "Good morning",
        12..=16 => "Good afternoon",
        17..=20 => "Good evening",
        _ => "Good night",
    }
}

fn section_title(ui: &mut Ui, title: &str, trailing: impl FnOnce(&mut Ui)) {
    let p = widgets::palette(ui);
    ui.add_space(24.0);
    ui.horizontal(|ui| {
        ui.label(widgets::title_text(title, 16.0).color(p.on_surface));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), trailing);
    });
    ui.add_space(8.0);
}

/// Lay out fixed-height cards in responsive columns.
fn card_grid(ui: &mut Ui, count: usize, min_w: f32, h: f32, mut card: impl FnMut(&mut Ui, usize, Rect)) {
    if count == 0 {
        return;
    }
    let gap = 12.0;
    let width = ui.available_width();
    let cols = (((width + gap) / (min_w + gap)).floor() as usize).max(1);
    let w = (width - gap * (cols as f32 - 1.0)) / cols as f32;
    let rows = count.div_ceil(cols);
    let (area, _) = ui.allocate_exact_size(vec2(width, rows as f32 * (h + gap) - gap), Sense::hover());
    for i in 0..count {
        let (r, c) = (i / cols, i % cols);
        let rect = Rect::from_min_size(
            pos2(area.left() + c as f32 * (w + gap), area.top() + r as f32 * (h + gap)),
            vec2(w, h),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        card(&mut child, i, rect);
    }
}

struct Tile {
    page: Page,
    subtitle: String,
    /// Icon circle colors.
    accent: (Color32, Color32),
}

fn tile(ui: &mut Ui, rect: Rect, t: &Tile) -> bool {
    let p = widgets::palette(ui);
    let (_, resp) = clickable_card(ui, rect.size(), p.surface_container, Id::unique(("overview-tile", t.page.title())));
    let c = pos2(rect.left() + 34.0, rect.center().y);
    ui.painter().circle_filled(c, 22.0, t.accent.0);
    paint_icon(ui, t.page.icon(), c, 22.0, t.accent.1);
    let text_w = rect.width() - 76.0;
    let title = truncated_galley(ui, t.page.title(), widgets::title_font(14.5), p.on_surface, text_w);
    let sub = truncated_galley(ui, &t.subtitle, FontId::proportional(12.5), p.on_surface_variant, text_w);
    ui.painter().galley(pos2(rect.left() + 66.0, rect.center().y - title.size().y), title, p.on_surface);
    ui.painter().galley(pos2(rect.left() + 66.0, rect.center().y + 2.0), sub, p.on_surface_variant);
    resp.clicked()
}

/// A wide banner card with an icon, a title, a line of text and buttons.
fn desktop_card(
    ui: &mut Ui,
    icon: MaterialIcon,
    fill: (Color32, Color32),
    title: &str,
    text: &str,
    buttons: impl FnOnce(&mut Ui),
) {
    egui::Frame::new()
        .fill(fill.0)
        .corner_radius(egui::CornerRadius::same(20))
        .inner_margin(egui::Margin::symmetric(20, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                widgets::icon(ui, icon, 32.0, fill.1);
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.set_max_width((ui.available_width() - 300.0).max(180.0));
                    ui.label(widgets::title_text(title, 16.0).color(fill.1));
                    ui.add(egui::Label::new(RichText::new(text).size(13.0).color(fill.1.gamma_multiply(0.85))).wrap());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), buttons);
            });
        });
    ui.add_space(10.0);
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let ctx = app.ctx.clone();
    let p = app.palette.clone();
    let st = &mut app.pages.overview;
    st.desktop.ensure(&ctx, detect);
    st.supplies.ensure(&ctx, power::read_supplies);
    if st.supplies.finished_at.is_some_and(|t| t.elapsed() > Duration::from_secs(30)) && !st.supplies.running() {
        st.supplies.spawn(&ctx, power::read_supplies);
    }
    ctx.request_repaint_after(Duration::from_secs(30));

    let mut actions = Vec::new();
    let desktop = st.desktop.value.clone().unwrap_or_default();
    let supplies = st.supplies.value.clone().unwrap_or_default();
    let user = std::env::var("USER").unwrap_or_default();
    let distro = app.os.pretty_name.clone();
    let installed_count = app.installed.len();
    let terminal = crate::sys::term::pick(&app.config.terminal);
    super::material_apps::poll(app);
    let material = super::material_apps::summary(app);
    let st = &mut app.pages.overview;

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(32, 28)).show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(1100.0));
            let hello = if user.is_empty() { greeting().to_string() } else { format!("{}, {user}", greeting()) };
            ui.label(RichText::new(hello).size(32.0).color(p.on_surface));
            let mut sub = vec![chrono::Local::now().format("%A, %B %-d").to_string()];
            if !desktop.hostname.is_empty() {
                sub.push(desktop.hostname.clone());
            }
            if !distro.is_empty() {
                sub.push(distro.clone());
            }
            ui.label(RichText::new(sub.join("  ·  ")).size(15.0).color(p.on_surface_variant));

            if !app.os.is_arch_based() {
                crate::ui::common::banner(ui, ICON_WARNING, "This app is made for Arch Linux. Package management and some pages need pacman and systemd.", false, |_| {});
            }

            // ------------------------------------------------- quick tiles
            section_title(ui, "Settings", |_| {});
            let battery = supplies.batteries.first().and_then(|b| b.capacity.map(|c| (c, b.status.clone())));
            let accents = [
                (p.primary_container, p.on_primary_container),
                (p.secondary_container, p.on_secondary_container),
                (p.tertiary_container, p.on_tertiary_container),
            ];
            let tiles = vec![
                Tile { page: Page::Appearance, subtitle: "Wallpaper, themes, icons and fonts".into(), accent: accents[0] },
                Tile { page: Page::Display, subtitle: "Resolution, scale and refresh rate".into(), accent: accents[1] },
                Tile { page: Page::Sound, subtitle: "Output, input and volume".into(), accent: accents[2] },
                Tile { page: Page::Network, subtitle: "Wi-Fi and connections".into(), accent: accents[0] },
                Tile { page: Page::Bluetooth, subtitle: "Devices and pairing".into(), accent: accents[1] },
                Tile {
                    page: Page::Power,
                    subtitle: match &battery {
                        Some((c, s)) => format!("Battery {c}% · {s}"),
                        None => "Power mode and session".into(),
                    },
                    accent: accents[2],
                },
                Tile { page: Page::DateTime, subtitle: chrono::Local::now().format("%H:%M · time zone and NTP").to_string(), accent: accents[0] },
                Tile { page: Page::DefaultApps, subtitle: "Browser, files, media…".into(), accent: accents[1] },
                Tile {
                    page: Page::Packages,
                    subtitle: if installed_count > 0 { format!("{installed_count} packages installed") } else { "Updates and pacman options".into() },
                    accent: accents[2],
                },
                Tile { page: Page::About, subtitle: format!("Up {}", sysinfo::format_uptime(desktop.uptime)), accent: accents[0] },
            ];
            card_grid(ui, tiles.len(), 250.0, 72.0, |ui, i, rect| {
                if tile(ui, rect, &tiles[i]) {
                    actions.push(Action::Goto(tiles[i].page.clone()));
                }
            });

            // ------------------------------------------------ desktop
            section_title(ui, "Your desktop", |_| {});
            let hypr_text = match (&desktop.hyprland_version, desktop.hyprland_running) {
                (Some(v), _) => format!("Running Hyprland {v}. Gaps, borders, blur, animations, input and more."),
                (None, true) => "Running Hyprland. Gaps, borders, blur, animations, input and more.".into(),
                (None, false) => "Hyprland isn't running in this session. You can still edit its config.".into(),
            };
            desktop_card(ui, ICON_WATER_DROP, (p.secondary_container, p.on_secondary_container), "Hyprland", &hypr_text, |ui| {
                if widgets::button(ui, ButtonKind::Filled, Some(ICON_TUNE), "Open").clicked() {
                    actions.push(Action::Goto(Page::Hyprland));
                }
            });
            let (title, text) = match (desktop.end4pc, desktop.ii) {
                (true, _) => ("end4-pC", "The end4-pC shell is installed. Bar, dock, widgets, wallpaper theming and all its other options.".to_string()),
                (false, true) => ("illogical-impulse", "end-4's illogical-impulse shell is installed. Its fork end4-pC adds presets, widgets and more.".to_string()),
                (false, false) => ("end-4 dotfiles", "Neither illogical-impulse nor end4-pC was found in ~/.config/quickshell.".to_string()),
            };
            desktop_card(ui, ICON_AUTO_AWESOME, (p.tertiary_container, p.on_tertiary_container), title, &text, |ui| {
                if widgets::button(ui, ButtonKind::Filled, Some(ICON_TUNE), if desktop.end4pc || desktop.ii { "Customize" } else { "Learn more" }).clicked() {
                    actions.push(Action::Goto(Page::End4));
                }
            });
            let total = crate::material_apps::APPS.len();
            let text = match material {
                Some((n, 0)) => format!("{n} of {total} installed. Music, Games, Browser, AI, Viewer, Text, Monitor and Calendar are a click away."),
                Some((n, u)) => format!("{n} of {total} installed, {u} update{} available.", if u == 1 { "" } else { "s" }),
                None => "Music, Games, Browser, AI, Viewer, Text, Monitor and Calendar, made to match this desktop.".into(),
            };
            desktop_card(ui, ICON_WIDGETS, (p.primary_container, p.on_primary_container), "Material 3 apps", &text, |ui| {
                let updates = material.is_some_and(|(_, u)| u > 0);
                if widgets::button(ui, ButtonKind::Filled, Some(if updates { ICON_UPDATE } else { ICON_APPS }), if updates { "Update" } else { "Manage" }).clicked() {
                    actions.push(Action::Goto(Page::MaterialApps));
                }
            });

            // ------------------------------------------------ configs
            let total = catalog::APPS.len();
            section_title(ui, "App configs", |ui| {
                if widgets::button(ui, ButtonKind::Text, Some(ICON_ARROW_FORWARD), "All apps").clicked() {
                    actions.push(Action::Goto(Page::Apps));
                }
            });
            let configured: Vec<&catalog::AppDef> = desktop.configured.iter().filter_map(|id| catalog::find(id)).collect();
            ui.label(RichText::new(if st.desktop.value.is_none() {
                "Looking for config files…".to_string()
            } else {
                format!("{} of {total} known apps have a config file. Pick one to edit it, or create a config for another app on the Apps page.", configured.len())
            }).color(p.on_surface_variant));
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                for a in &configured {
                    if widgets::filter_chip(ui, a.name, false, Some(a.icon)).clicked() {
                        actions.push(Action::Goto(Page::App(a.id.to_string())));
                    }
                }
            });

            if terminal.is_none() {
                ui.add_space(16.0);
                crate::ui::common::banner(ui, ICON_TERMINAL, "No terminal emulator found. Installing packages opens a terminal; pick or install one in Preferences.", false, |ui| {
                    if widgets::button(ui, ButtonKind::Tonal, None, "Preferences").clicked() {
                        actions.push(Action::Goto(Page::Preferences));
                    }
                });
            }
            ui.add_space(24.0);
        });
    });
    app.actions.extend(actions);
}
