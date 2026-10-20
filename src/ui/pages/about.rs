//! Placeholder page.

use crate::app::App;
use crate::ui::common;

#[derive(Default)]
pub struct State {}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    let page = app.page.clone();
    common::page(ui, page.icon(), page.title(), "Not implemented yet.", |_| {});
}
