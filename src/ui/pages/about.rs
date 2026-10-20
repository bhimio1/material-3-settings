//! About: distro, hardware and software versions.

use crate::app::App;
use crate::sys::pacman::format_size;
use crate::sys::sysinfo::{self, SystemInfo};
use crate::sys::task::Task;
use crate::ui::common::{self, card};
use crate::ui::widgets::{self, ButtonKind};
use crate::ui::{toast, ToastKind};
use egui::{vec2, CornerRadius, RichText, Ui};
use egui_material_icons::icons::*;

#[derive(Default)]
pub struct State {
    info: Task<SystemInfo>,
}

fn software(info: &SystemInfo, distro: &str) -> Vec<(&'static str, String)> {
    let dash = |s: &str| if s.is_empty() { "—".to_string() } else { s.to_string() };
    let mut v = vec![
        ("Distribution", dash(distro)),
        ("Kernel", dash(&info.kernel)),
        ("Desktop", dash(&info.desktop)),
        ("Session", dash(&info.session)),
    ];
    if let Some(h) = &info.hyprland {
        v.push(("Hyprland", h.clone()));
    }
    v.push(("Shell", dash(&info.shell)));
    if let Some(n) = info.packages {
        v.push(("Packages", format!("{n} (pacman)")));
    }
    v.push(("Firmware", if info.uefi { "UEFI".into() } else { "BIOS / legacy".into() }));
    v.push(("Uptime", sysinfo::format_uptime(info.uptime_secs)));
    v
}

fn hardware(info: &SystemInfo) -> Vec<(&'static str, String)> {
    let mut v = vec![(
        "Processor",
        if info.cpu.is_empty() {
            "—".into()
        } else {
            format!("{} ({} threads)", info.cpu, info.cores)
        },
    )];
    v.push((
        "Memory",
        format!(
            "{} ({} available)",
            format_size(info.mem_total),
            format_size(info.mem_available)
        ),
    ));
    for (i, g) in info.gpus.iter().enumerate() {
        v.push((if i == 0 { "Graphics" } else { "" }, g.clone()));
    }
    v
}

/// Markdown block for bug reports.
fn report(info: &SystemInfo, distro: &str) -> String {
    let mut out = String::from("### System information\n\n");
    for (k, v) in software(info, distro).into_iter().chain(hardware(info)) {
        let k = if k.is_empty() { "Graphics" } else { k };
        out.push_str(&format!("- **{k}:** {v}\n"));
    }
    for d in &info.disks {
        out.push_str(&format!(
            "- **Disk {}:** {} used of {}\n",
            d.mount,
            format_size(d.used),
            format_size(d.total)
        ));
    }
    out
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let ctx = app.ctx.clone();
    let p = app.palette.clone();
    let distro = if app.os.pretty_name.is_empty() {
        "Unknown Linux".to_string()
    } else {
        app.os.pretty_name.clone()
    };
    let arch_based = app.os.is_arch_based();
    let st = &mut app.pages.about;
    let actions = &mut app.actions;
    st.info.ensure(&ctx, sysinfo::load);

    common::page(ui, ICON_INFO, "About", "", |ui| {
        if !arch_based {
            common::banner(ui, ICON_WARNING, &format!("This app is made for Arch Linux and its derivatives. {distro} isn't Arch-based, so package management and some pages won't work."), false, |_| {});
        }
        let Some(info) = &st.info.value else {
            common::loading(ui, "Gathering system information…");
            return;
        };

        // Hero card: host name and distribution.
        ui.add_space(16.0);
        egui::Frame::new()
            .fill(p.primary_container)
            .corner_radius(CornerRadius::same(24))
            .inner_margin(egui::Margin::symmetric(24, 20))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(72.0, 72.0), egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(), 36.0, p.primary);
                    widgets::paint_icon(ui, ICON_COMPUTER, rect.center(), 38.0, p.on_primary);
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(if info.hostname.is_empty() { "This computer" } else { &info.hostname }).size(28.0).color(p.on_primary_container));
                        ui.label(RichText::new(&distro).size(15.0).color(p.on_primary_container));
                        if !info.user.is_empty() {
                            ui.label(RichText::new(format!("Signed in as {}", info.user)).size(13.0).color(p.on_primary_container.gamma_multiply(0.8)));
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::button(ui, ButtonKind::Filled, Some(ICON_CONTENT_COPY), "Copy system info").clicked() {
                            ui.ctx().copy_text(report(info, &distro));
                            toast(actions, "System information copied as Markdown", ToastKind::Info);
                        }
                    });
                });
            });

        card(ui, "Software", |ui| common::facts(ui, "about-sw", &software(info, &distro)));
        card(ui, "Hardware", |ui| common::facts(ui, "about-hw", &hardware(info)));

        if !info.disks.is_empty() {
            card(ui, "Storage", |ui| {
                for (i, d) in info.disks.iter().enumerate() {
                    if i > 0 {
                        ui.separator();
                    }
                    ui.add_space(8.0);
                    let fraction = if d.total == 0 { 0.0 } else { d.used as f32 / d.total as f32 };
                    ui.horizontal(|ui| {
                        widgets::icon(ui, if d.mount == "/" { ICON_HARD_DRIVE } else { ICON_HOME }, 22.0, p.primary);
                        ui.label(widgets::title_text(&d.mount, 14.0).color(p.on_surface));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(format!("{} used of {} ({:.0}%)", format_size(d.used), format_size(d.total), fraction * 100.0)).size(12.5).color(p.on_surface_variant));
                        });
                    });
                    ui.add_space(4.0);
                    widgets::linear_progress(ui, Some(fraction), ui.available_width());
                    ui.add_space(8.0);
                }
            });
        }

        ui.add_space(20.0);
        ui.horizontal(|ui| {
            if common::refresh_button(ui, st.info.running()) {
                st.info.spawn(&ctx, sysinfo::load);
            }
            ui.label(RichText::new(format!("Material 3 Settings {}", env!("CARGO_PKG_VERSION"))).size(12.0).color(p.on_surface_variant));
        });
    });
}
