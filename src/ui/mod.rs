//! User interface. Pages draw from app state and push [`Action`]s for
//! anything that needs the whole app (navigation, toasts, terminals).

pub mod editor;
pub mod pages;
pub mod sidebar;
pub mod welcome;

use material_3_ui::common::{self, Requirement};
use material_3_ui::icons::{icons::*, MaterialIcon};
use std::path::PathBuf;

pub use material_3_ui::toasts::ToastKind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Page {
    Overview,
    Appearance,
    Display,
    Sound,
    Network,
    Bluetooth,
    Power,
    DateTime,
    DefaultApps,
    Packages,
    About,
    Hyprland,
    End4,
    Apps,
    /// One app's config, by catalog id (or `custom:<index>`).
    App(String),
    /// Install, update and remove the Material 3 apps.
    MaterialApps,
    /// One Material 3 app, by short name (`music`).
    MaterialApp(String),
    Preferences,
}

/// Sidebar groups, in order.
pub const NAV: &[(&str, &[Page])] = &[
    ("", &[Page::Overview]),
    (
        "System",
        &[
            Page::Appearance,
            Page::Display,
            Page::Sound,
            Page::Network,
            Page::Bluetooth,
            Page::Power,
            Page::DateTime,
            Page::DefaultApps,
        ],
    ),
    ("Arch Linux", &[Page::Packages, Page::About]),
    ("Desktop", &[Page::Hyprland, Page::End4, Page::MaterialApps]),
    ("Configs", &[Page::Apps]),
];

impl Page {
    pub fn title(&self) -> &'static str {
        match self {
            Page::Overview => "Overview",
            Page::Appearance => "Appearance",
            Page::Display => "Displays",
            Page::Sound => "Sound",
            Page::Network => "Network",
            Page::Bluetooth => "Bluetooth",
            Page::Power => "Power",
            Page::DateTime => "Date & time",
            Page::DefaultApps => "Default apps",
            Page::Packages => "Packages",
            Page::About => "About",
            Page::Hyprland => "Hyprland",
            Page::End4 => "end4-pC / ii",
            Page::Apps | Page::App(_) => "App configs",
            Page::MaterialApps | Page::MaterialApp(_) => "Material 3 apps",
            Page::Preferences => "Preferences",
        }
    }

    pub fn icon(&self) -> MaterialIcon {
        match self {
            Page::Overview => ICON_DASHBOARD,
            Page::Appearance => ICON_PALETTE,
            Page::Display => ICON_MONITOR,
            Page::Sound => ICON_VOLUME_UP,
            Page::Network => ICON_WIFI,
            Page::Bluetooth => ICON_BLUETOOTH,
            Page::Power => ICON_BATTERY_CHARGING_FULL,
            Page::DateTime => ICON_SCHEDULE,
            Page::DefaultApps => ICON_APPS,
            Page::Packages => ICON_PACKAGE_2,
            Page::About => ICON_INFO,
            Page::Hyprland => ICON_WATER_DROP,
            Page::End4 => ICON_AUTO_AWESOME,
            Page::Apps | Page::App(_) => ICON_TUNE,
            Page::MaterialApps | Page::MaterialApp(_) => ICON_WIDGETS,
            Page::Preferences => ICON_SETTINGS,
        }
    }

    /// Command-line names.
    pub fn parse(s: &str) -> Option<Page> {
        Some(match s {
            "overview" => Page::Overview,
            "appearance" => Page::Appearance,
            "display" | "displays" => Page::Display,
            "sound" => Page::Sound,
            "network" | "wifi" => Page::Network,
            "bluetooth" => Page::Bluetooth,
            "power" => Page::Power,
            "datetime" | "time" => Page::DateTime,
            "default-apps" => Page::DefaultApps,
            "packages" | "pacman" => Page::Packages,
            "about" => Page::About,
            "hyprland" => Page::Hyprland,
            "end4" | "end4-pc" | "ii" | "illogical-impulse" => Page::End4,
            "apps" | "configs" => Page::Apps,
            "material-apps" | "m3-apps" => Page::MaterialApps,
            "preferences" | "prefs" => Page::Preferences,
            id if crate::catalog::find(id).is_some() => Page::App(id.to_string()),
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub enum Action {
    Goto(Page),
    Toast(String, ToastKind),
    /// Install packages in a terminal: from the configured repos (Arch,
    /// CachyOS, …) when they have them, otherwise with the AUR helper.
    Install { packages: Vec<String> },
    /// Run a shell command line in a terminal window.
    RunInTerminal(String),
    /// Open a file or folder with the default application.
    OpenPath(PathBuf),
    OpenUrl(String),
    /// Re-read the list of installed packages.
    RefreshPackages,
    /// Preferences changed: save them and re-apply the theme.
    ConfigChanged,
    /// Show the welcome flow instead of the pages.
    StartWelcome,
    /// Leave the welcome flow, recording that it was seen. Either go on to
    /// the pages or close the window.
    FinishWelcome { open_settings: bool },
}

pub type Actions = Vec<Action>;

pub fn toast(actions: &mut Actions, text: impl Into<String>, kind: ToastKind) {
    actions.push(Action::Toast(text.into(), kind));
}

/// [`common::require`] for a page: when `program` is missing, show a
/// banner offering to install `package` (in a terminal) and return false.
pub fn require(
    ui: &mut egui::Ui,
    actions: &mut Actions,
    installed: &std::collections::HashSet<String>,
    program: &str,
    package: &str,
    purpose: &str,
) -> bool {
    match common::require(ui, installed, program, package, purpose) {
        Requirement::Present => true,
        Requirement::Missing => false,
        Requirement::InstallRequested => {
            actions.push(Action::Install { packages: vec![package.to_string()] });
            false
        }
    }
}
