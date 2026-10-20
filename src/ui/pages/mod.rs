//! One module per page. Each has a `State` (kept for the app's lifetime)
//! and a `show` function.

pub mod about;
pub mod appearance;
pub mod apps;
pub mod bluetooth;
pub mod datetime;
pub mod default_apps;
pub mod display;
pub mod end4;
pub mod hyprland;
pub mod network;
pub mod overview;
pub mod packages;
pub mod power;
pub mod prefs;
pub mod sound;

use super::Page;
use crate::app::App;

#[derive(Default)]
pub struct States {
    pub about: about::State,
    pub appearance: appearance::State,
    pub apps: apps::State,
    pub bluetooth: bluetooth::State,
    pub datetime: datetime::State,
    pub default_apps: default_apps::State,
    pub display: display::State,
    pub end4: end4::State,
    pub hyprland: hyprland::State,
    pub network: network::State,
    pub overview: overview::State,
    pub packages: packages::State,
    pub power: power::State,
    pub prefs: prefs::State,
    pub sound: sound::State,
}

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    match app.page.clone() {
        Page::Overview => overview::show(ui, app),
        Page::Appearance => appearance::show(ui, app),
        Page::Display => display::show(ui, app),
        Page::Sound => sound::show(ui, app),
        Page::Network => network::show(ui, app),
        Page::Bluetooth => bluetooth::show(ui, app),
        Page::Power => power::show(ui, app),
        Page::DateTime => datetime::show(ui, app),
        Page::DefaultApps => default_apps::show(ui, app),
        Page::Packages => packages::show(ui, app),
        Page::About => about::show(ui, app),
        Page::Hyprland => hyprland::show(ui, app),
        Page::End4 => end4::show(ui, app),
        Page::Apps => apps::show_catalog(ui, app),
        Page::App(id) => apps::show_app(ui, app, &id),
        Page::Preferences => prefs::show(ui, app),
    }
}
