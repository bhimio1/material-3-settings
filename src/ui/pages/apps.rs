//! Placeholder page.

use crate::app::App;
use crate::ui::common;

#[derive(Default)]
pub struct State {}

pub fn show_catalog(ui: &mut egui::Ui, app: &mut App) {
    let page = app.page.clone();
    common::page(ui, page.icon(), page.title(), "Not implemented yet.", |_| {});
}

pub fn show_app(ui: &mut egui::Ui, app: &mut App, id: &str) {
    common::page(ui, crate::ui::Page::Apps.icon(), id, "Not implemented yet.", |_| {});
    let _ = app;
}
