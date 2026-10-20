//! Navigation drawer: a search field, page groups, and Preferences.

use super::widgets::{self, nav_item, section_header};
use super::{Action, Page, NAV};
use crate::app::App;
use egui::{RichText, Ui};
use egui_material_icons::icons::*;

fn matches(haystack: &str, query: &str) -> bool {
    haystack.to_lowercase().contains(&query.to_lowercase())
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let p = app.palette.clone();
    let mut actions = Vec::new();
    let current = app.page.clone();

    ui.horizontal(|ui| {
        ui.add_space(12.0);
        widgets::icon(ui, ICON_SETTINGS, 26.0, p.primary);
        ui.label(widgets::title_text("Settings", 20.0).color(p.on_surface));
    });
    ui.add_space(14.0);

    let search = egui::Frame::new()
        .fill(p.surface_container_high)
        .corner_radius(egui::CornerRadius::same(22))
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::icon(ui, ICON_SEARCH, 20.0, p.on_surface_variant);
                ui.add(
                    egui::TextEdit::singleline(&mut app.nav_query)
                        .hint_text("Search settings")
                        .frame(egui::Frame::NONE)
                        .desired_width(ui.available_width() - 28.0),
                )
            })
            .inner
        })
        .inner;
    if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
        search.request_focus();
    }
    if !app.nav_query.is_empty() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.nav_query.clear();
    }
    ui.add_space(8.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let query = app.nav_query.trim().to_string();
            if !query.is_empty() {
                let mut any = false;
                for (_, pages) in NAV {
                    for page in pages.iter() {
                        if matches(page.title(), &query) {
                            any = true;
                            if nav_item(ui, page.icon(), page.title(), current == *page, None).clicked() {
                                actions.push(Action::Goto(page.clone()));
                            }
                        }
                    }
                }
                let apps: Vec<_> = crate::catalog::APPS
                    .iter()
                    .filter(|a| matches(a.name, &query) || matches(a.id, &query) || matches(a.category.label(), &query))
                    .collect();
                if !apps.is_empty() {
                    section_header(ui, "App configs");
                    for a in apps {
                        any = true;
                        let page = Page::App(a.id.to_string());
                        if nav_item(ui, a.icon, a.name, current == page, None).clicked() {
                            actions.push(Action::Goto(page));
                        }
                    }
                }
                if !any {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.label(RichText::new("No matches").color(p.on_surface_variant));
                    });
                }
                return;
            }

            for (group, pages) in NAV {
                if !group.is_empty() {
                    section_header(ui, group);
                }
                for page in pages.iter() {
                    let selected = current == *page
                        || (*page == Page::Apps && matches!(current, Page::App(_)));
                    if nav_item(ui, page.icon(), page.title(), selected, None).clicked() {
                        actions.push(Action::Goto(page.clone()));
                    }
                }
            }
            ui.add_space(8.0);
            if nav_item(
                ui,
                ICON_PALETTE,
                "Preferences",
                current == Page::Preferences,
                None,
            )
            .on_hover_text("Theme and behaviour of this app")
            .clicked()
            {
                actions.push(Action::Goto(Page::Preferences));
            }
        });
    app.actions.extend(actions);
}
