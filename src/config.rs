//! This app's own preferences (`~/.config/m3-settings/config.toml`).

use crate::sys::arch::AurHelper;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ColorScheme {
    #[default]
    System,
    Light,
    Dark,
}

/// Where the Material palette comes from.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeSource {
    /// The end-4 shell's colors, else the file manager's theme.json, else
    /// the seed color.
    #[default]
    Auto,
    /// Generate a tonal palette from `seed_color`.
    Seed,
}

/// A config file the user added to the Apps page themselves.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct CustomFile {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub theme_source: ThemeSource,
    pub color_scheme: ColorScheme,
    pub seed_color: String,
    pub filled_icons: bool,
    pub ui_scale: f32,
    /// Terminal used for package installs and other interactive commands.
    /// Empty = `$TERMINAL`, then the first installed known terminal.
    pub terminal: String,
    pub aur_helper: AurHelper,
    /// Keep a timestamped copy of a file before every save.
    pub backups: bool,
    pub editor_font_size: f32,
    pub custom_files: Vec<CustomFile>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme_source: ThemeSource::Auto,
            color_scheme: ColorScheme::System,
            seed_color: crate::theme::palette::DEFAULT_SEED.into(),
            filled_icons: true,
            ui_scale: 1.0,
            terminal: String::new(),
            aur_helper: AurHelper::Auto,
            backups: true,
            editor_font_size: 13.0,
            custom_files: Vec::new(),
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
        assert_eq!(c.seed_color, "#FF0000");
        assert!(c.backups);
        let round: Config = toml::from_str(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert_eq!(round, c);
    }
}
