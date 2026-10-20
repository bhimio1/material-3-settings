//! Page scaffolding shared by every settings page: the scrolling page body,
//! titled cards, and title/description/control rows.

use super::widgets::{self, ButtonKind};
use super::{Action, Actions};
use egui::{CornerRadius, Frame, Margin, RichText, Ui};
use egui_material_icons::{icons::*, MaterialIcon};
use std::collections::HashSet;

/// Widest the page content gets.
pub const PAGE_WIDTH: f32 = 880.0;

/// A scrolling page with a large header.
pub fn page(ui: &mut Ui, icon: MaterialIcon, title: &str, subtitle: &str, body: impl FnOnce(&mut Ui)) {
    let p = widgets::palette(ui);
    egui::ScrollArea::vertical()
        .id_salt(("page", title))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::symmetric(32, 24)).show(ui, |ui| {
                ui.set_max_width(ui.available_width().min(PAGE_WIDTH));
                ui.horizontal(|ui| {
                    widgets::icon(ui, icon, 30.0, p.primary);
                    ui.label(RichText::new(title).size(28.0).color(p.on_surface));
                });
                if !subtitle.is_empty() {
                    ui.add(egui::Label::new(RichText::new(subtitle).color(p.on_surface_variant)).wrap());
                }
                body(ui);
                ui.add_space(24.0);
            });
        });
}

/// A titled group of rows.
pub fn card(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    let p = widgets::palette(ui);
    ui.add_space(20.0);
    if !title.is_empty() {
        ui.label(widgets::title_text(title, 14.0).color(p.primary));
        ui.add_space(6.0);
    }
    Frame::new()
        .fill(p.surface_container)
        .corner_radius(CornerRadius::same(20))
        .inner_margin(Margin::symmetric(20, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// A settings row: title and description on the left, control on the right.
pub fn row(ui: &mut Ui, title: &str, desc: &str, control: impl FnOnce(&mut Ui)) {
    let p = widgets::palette(ui);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width((ui.available_width() - 280.0).max(200.0));
            ui.label(RichText::new(title).size(14.5).color(p.on_surface));
            if !desc.is_empty() {
                ui.add(egui::Label::new(RichText::new(desc).size(12.5).color(p.on_surface_variant)).wrap());
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
    ui.add_space(8.0);
}

/// Body text inside a card.
pub fn note(ui: &mut Ui, text: &str) {
    let p = widgets::palette(ui);
    ui.add_space(8.0);
    ui.add(egui::Label::new(RichText::new(text).size(13.0).color(p.on_surface_variant)).wrap());
    ui.add_space(8.0);
}

/// A tinted banner for warnings and status messages.
pub fn banner(ui: &mut Ui, icon: MaterialIcon, text: &str, error: bool, actions: impl FnOnce(&mut Ui)) {
    let p = widgets::palette(ui);
    let (bg, fg) = if error {
        (p.error_container, p.on_error_container)
    } else {
        (p.tertiary_container, p.on_tertiary_container)
    };
    ui.add_space(12.0);
    Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::symmetric(16, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                widgets::icon(ui, icon, 20.0, fg);
                ui.vertical(|ui| {
                    ui.set_max_width((ui.available_width() - 260.0).max(200.0));
                    ui.add(egui::Label::new(RichText::new(text).color(fg)).wrap());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), actions);
            });
        });
}

/// A spinner with a label, for values still loading.
pub fn loading(ui: &mut Ui, text: &str) {
    let p = widgets::palette(ui);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new().color(p.primary));
        ui.label(RichText::new(text).color(p.on_surface_variant));
    });
    ui.add_space(8.0);
}

/// Check that a command-line tool is available. When it isn't, show a
/// banner offering to install its package and return false.
pub fn require(
    ui: &mut Ui,
    actions: &mut Actions,
    installed: &HashSet<String>,
    program: &str,
    package: &str,
    purpose: &str,
) -> bool {
    if crate::sys::cmd::exists(program) {
        return true;
    }
    let have_pkg = installed.contains(package);
    let text = if have_pkg {
        format!("`{program}` isn't on your PATH. {purpose}")
    } else {
        format!("`{program}` is needed for this page ({package}). {purpose}")
    };
    banner(ui, ICON_WARNING, &text, false, |ui| {
        if !have_pkg
            && widgets::button(ui, ButtonKind::Filled, Some(ICON_DOWNLOAD), "Install").clicked()
        {
            actions.push(Action::Install {
                packages: vec![package.to_string()],
                aur: false,
            });
        }
    });
    false
}

/// A small refresh button for the top-right of a card or page.
pub fn refresh_button(ui: &mut Ui, busy: bool) -> bool {
    if busy {
        ui.add(egui::Spinner::new().color(widgets::palette(ui).primary));
        false
    } else {
        widgets::icon_button(ui, ICON_REFRESH, "Refresh", true).clicked()
    }
}

/// A combo box over string options; returns true when the value changed.
pub fn combo(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, value: &mut String, options: &[String], width: f32) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(if value.is_empty() { "—".to_string() } else { value.clone() })
        .show_ui(ui, |ui| {
            for opt in options {
                if ui.selectable_label(value == opt, opt).clicked() && value != opt {
                    *value = opt.clone();
                    changed = true;
                }
            }
        });
    changed
}

/// Label/value pairs in two columns (used by About and status cards).
pub fn facts(ui: &mut Ui, id: &str, items: &[(&str, String)]) {
    let p = widgets::palette(ui);
    ui.add_space(8.0);
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([24.0, 8.0])
        .show(ui, |ui| {
            for (k, v) in items {
                ui.label(RichText::new(*k).color(p.on_surface_variant));
                ui.add(egui::Label::new(RichText::new(v).color(p.on_surface)).wrap());
                ui.end_row();
            }
        });
    ui.add_space(8.0);
}
