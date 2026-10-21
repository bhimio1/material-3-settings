//! This app's own preferences (`~/.config/m3-settings/config.toml`).

use material_3_ui::sys::arch::AurHelper;
use material_3_ui::theme::{ColorScheme, ThemeSettings, ThemeSource};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A config file the user added to the Apps page themselves.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct CustomFile {
    pub name: String,
    pub path: String,
}

/// How [`ThemeSettings`] is stored in config.toml: flat, with Settings' own
/// key names (`theme_source`, `color_scheme`), and the shared crate's
/// names (`source`, `scheme`) accepted too.
#[derive(Deserialize, Serialize)]
#[serde(remote = "ThemeSettings")]
struct Appearance {
    #[serde(rename = "theme_source", alias = "source", default)]
    source: ThemeSource,
    #[serde(rename = "color_scheme", alias = "scheme", default)]
    scheme: ColorScheme,
    #[serde(default = "default_seed")]
    seed_color: String,
    #[serde(default = "yes")]
    filled_icons: bool,
    #[serde(default = "one")]
    ui_scale: f32,
}

fn default_seed() -> String {
    ThemeSettings::default().seed_color
}

fn yes() -> bool {
    true
}

fn one() -> f32 {
    1.0
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Colors, light/dark, icons and scale, shared with the other Material
    /// 3 apps. Kept at the top level of config.toml under the names
    /// Settings has always used (see [`Appearance`]).
    #[serde(flatten, with = "Appearance")]
    pub appearance: ThemeSettings,
    /// Terminal used for package installs and other interactive commands.
    /// Empty = `$TERMINAL`, then the first installed known terminal.
    pub terminal: String,
    pub aur_helper: AurHelper,
    /// Keep a timestamped copy of a file before every save.
    pub backups: bool,
    pub editor_font_size: f32,
    pub custom_files: Vec<CustomFile>,
    /// Install method per Material 3 app (by short name), when not automatic.
    pub material_app_methods: BTreeMap<String, crate::material_apps::Method>,
    /// The welcome flow has been finished or closed at least once. The
    /// shell's own marker (see `welcome.rs`) is what decides whether it
    /// greets again; this records it for Settings.
    pub welcome_done: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            appearance: ThemeSettings::default(),
            terminal: String::new(),
            aur_helper: AurHelper::Auto,
            backups: true,
            editor_font_size: 13.0,
            custom_files: Vec::new(),
            material_app_methods: BTreeMap::new(),
            welcome_done: false,
        }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("m3-settings/config.toml"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path().ok_or_else(|| anyhow::anyhow!("no config directory"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_files_load_with_defaults() {
        let c: Config = toml::from_str("seed_color = \"#FF0000\"\n").unwrap();
        assert_eq!(c.appearance.seed_color, "#FF0000");
        assert!(c.backups);
        assert!(!c.welcome_done);
        let round: Config = toml::from_str(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert_eq!(round, c);
    }

    #[test]
    fn appearance_keeps_its_old_keys() {
        // A config.toml written before the shared crate.
        let old = "theme_source = \"seed\"\ncolor_scheme = \"light\"\nseed_color = \"#6750A4\"\nfilled_icons = false\nui_scale = 1.25\nterminal = \"foot\"\nwelcome_done = true\n";
        let c: Config = toml::from_str(old).unwrap();
        assert_eq!(c.appearance.source, ThemeSource::Seed);
        assert_eq!(c.appearance.scheme, ColorScheme::Light);
        assert_eq!(c.appearance.seed_color, "#6750A4");
        assert!(!c.appearance.filled_icons);
        assert_eq!(c.appearance.ui_scale, 1.25);
        assert_eq!(c.terminal, "foot");
        assert!(c.welcome_done);
        // Written back under the same names, still flat.
        let text = toml::to_string_pretty(&c).unwrap();
        assert!(text.contains("theme_source = \"seed\""), "{text}");
        assert!(text.contains("color_scheme = \"light\""), "{text}");
        assert!(!text.contains("[appearance]"), "{text}");
        // The shared crate's names are understood too.
        let c: Config = toml::from_str("source = \"seed\"\nscheme = \"dark\"\n").unwrap();
        assert_eq!(c.appearance.source, ThemeSource::Seed);
        assert_eq!(c.appearance.scheme, ColorScheme::Dark);
        // Missing appearance keys fall back to the defaults.
        let c: Config = toml::from_str("terminal = \"kitty\"\n").unwrap();
        assert_eq!(c.appearance, ThemeSettings::default());
    }
}
