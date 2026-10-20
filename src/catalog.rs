//! The apps whose config files the Apps page knows how to edit: where the
//! files live, their format, a starter template, and "quick settings" that
//! get proper controls instead of raw text.

use crate::formats::{kv::Dialect, BoolStyle, Format, Syntax};
use egui_material_icons::{icons::*, MaterialIcon};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Terminal,
    Shell,
    Desktop,
    Launcher,
    Bar,
    Notifications,
    Editor,
    Media,
    Tools,
    System,
    Theming,
}

impl Category {
    pub const ALL: [Category; 11] = [
        Category::Terminal,
        Category::Shell,
        Category::Desktop,
        Category::Bar,
        Category::Launcher,
        Category::Notifications,
        Category::Editor,
        Category::Media,
        Category::Tools,
        Category::Theming,
        Category::System,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Terminal => "Terminals",
            Category::Shell => "Shells & prompts",
            Category::Desktop => "Window managers & lock screens",
            Category::Launcher => "Launchers",
            Category::Bar => "Bars & widgets",
            Category::Notifications => "Notifications",
            Category::Editor => "Editors",
            Category::Media => "Media",
            Category::Tools => "Command-line tools",
            Category::System => "System",
            Category::Theming => "Theming",
        }
    }
}

/// The control a quick setting gets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Bool(BoolStyle),
    Int { min: i64, max: i64 },
    Float { min: f64, max: f64, step: f64 },
    Text,
    /// Free text with suggestions.
    Choice(&'static [&'static str]),
    /// A `#rrggbb` color (written in the file's own notation, see `quoted`).
    Color,
    /// A font family name (offers installed fonts).
    Font,
}

#[derive(Clone, Copy, Debug)]
pub struct Quick {
    /// `section.key` for INI files, dotted path for TOML, `/` path for JSON.
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub kind: Kind,
    /// Wrap text values in double quotes (btop, some INI files).
    pub quoted: bool,
}

impl Quick {
    pub const fn new(key: &'static str, label: &'static str, kind: Kind) -> Self {
        Self {
            key,
            label,
            help: "",
            kind,
            quoted: false,
        }
    }

    pub const fn help(mut self, help: &'static str) -> Self {
        self.help = help;
        self
    }

    pub const fn quoted(mut self) -> Self {
        self.quoted = true;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ConfigSpec {
    /// Display name of this file ("Main config", "Style").
    pub label: &'static str,
    /// Candidate paths, first existing wins; the first is used when creating.
    pub paths: &'static [&'static str],
    pub format: Format,
    /// Contents for a new file.
    pub template: &'static str,
    pub quick: &'static [Quick],
    /// Root-owned file, saved through pkexec.
    pub system: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct AppDef {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: MaterialIcon,
    pub category: Category,
    pub description: &'static str,
    /// pacman (or AUR) package providing it.
    pub package: &'static str,
    pub aur: bool,
    /// Binary used to detect the app when the package name differs.
    pub binary: &'static str,
    /// Command that makes a running instance reload its config, if any.
    pub reload: &'static str,
    pub docs: &'static str,
    pub files: &'static [ConfigSpec],
}

pub fn find(id: &str) -> Option<&'static AppDef> {
    APPS.iter().find(|a| a.id == id)
}

/// Placeholder so the module compiles before the catalog is filled in.
pub static APPS: &[AppDef] = &[AppDef {
    id: "kitty",
    name: "kitty",
    icon: ICON_TERMINAL,
    category: Category::Terminal,
    description: "GPU-accelerated terminal emulator.",
    package: "kitty",
    aur: false,
    binary: "kitty",
    reload: "pkill -USR1 -x kitty",
    docs: "https://sw.kovidgoyal.net/kitty/conf/",
    files: &[ConfigSpec {
        label: "kitty.conf",
        paths: &["~/.config/kitty/kitty.conf"],
        format: Format::Kv(Dialect::SPACE),
        template: "# kitty configuration\n",
        quick: &[Quick::new("font_size", "Font size", Kind::Float { min: 6.0, max: 40.0, step: 0.5 })],
        system: false,
    }],
}];

#[allow(dead_code)]
const _UNUSED: Syntax = Syntax::Plain;
