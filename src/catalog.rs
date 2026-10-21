//! The apps whose config files the Apps page knows how to edit: where the
//! files live, their format, a starter template, and "quick settings" that
//! get proper controls instead of raw text.

use crate::formats::kv::{Dialect, Sep};
use crate::formats::{BoolStyle, Format, Syntax};
use material_3_ui::icons::{icons::*, MaterialIcon};

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
            Category::Media => "Media & gaming",
            Category::Tools => "Command-line tools",
            Category::System => "System",
            Category::Theming => "Theming",
        }
    }

    /// Short name for filter chips.
    pub fn short(self) -> &'static str {
        match self {
            Category::Terminal => "Terminals",
            Category::Shell => "Shells",
            Category::Desktop => "Desktop",
            Category::Launcher => "Launchers",
            Category::Bar => "Bars",
            Category::Notifications => "Notifications",
            Category::Editor => "Editors",
            Category::Media => "Media",
            Category::Tools => "Tools",
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
    /// A `#rrggbb` color.
    Color,
    /// A color written without the `#` (`rrggbb`, foot).
    ColorBare,
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
    /// Wrap text values in double quotes (btop, zathura, makepkg.conf…).
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

impl ConfigSpec {
    const fn new(
        label: &'static str,
        paths: &'static [&'static str],
        format: Format,
        template: &'static str,
        quick: &'static [Quick],
    ) -> Self {
        Self {
            label,
            paths,
            format,
            template,
            quick,
            system: false,
        }
    }

    const fn system(mut self) -> Self {
        self.system = true;
        self
    }

    /// The path to use: the first that exists, else the first candidate.
    pub fn resolve(&self) -> std::path::PathBuf {
        let candidates: Vec<_> = self
            .paths
            .iter()
            .map(|p| material_3_ui::sys::paths::expand(p))
            .collect();
        candidates
            .iter()
            .find(|p| p.exists())
            .cloned()
            .unwrap_or_else(|| candidates[0].clone())
    }
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

impl AppDef {
    #[allow(clippy::too_many_arguments)]
    const fn new(
        id: &'static str,
        name: &'static str,
        icon: MaterialIcon,
        category: Category,
        description: &'static str,
        package: &'static str,
        docs: &'static str,
        files: &'static [ConfigSpec],
    ) -> Self {
        Self {
            id,
            name,
            icon,
            category,
            description,
            package,
            aur: false,
            binary: package,
            reload: "",
            docs,
            files,
        }
    }

    const fn bin(mut self, binary: &'static str) -> Self {
        self.binary = binary;
        self
    }

    const fn aur(mut self) -> Self {
        self.aur = true;
        self
    }

    const fn reload(mut self, cmd: &'static str) -> Self {
        self.reload = cmd;
        self
    }

    /// Installed according to pacman's database or the binary on `$PATH`.
    pub fn is_installed(&self, packages: &std::collections::HashSet<String>) -> bool {
        packages.contains(self.package) || material_3_ui::sys::cmd::exists(self.binary)
    }
}

pub fn find(id: &str) -> Option<&'static AppDef> {
    APPS.iter().find(|a| a.id == id)
}

// ------------------------------------------------------------------ helpers

const YES_NO: Kind = Kind::Bool(BoolStyle::YesNo);
const TRUE_FALSE: Kind = Kind::Bool(BoolStyle::TrueFalse);
const CAPITAL: Kind = Kind::Bool(BoolStyle::Capitalized);
const FLAG: Kind = Kind::Bool(BoolStyle::Flag);
const ONE_ZERO: Kind = Kind::Bool(BoolStyle::OneZero);
const TEXT: Kind = Kind::Text;

const fn int(min: i64, max: i64) -> Kind {
    Kind::Int { min, max }
}

const fn float(min: f64, max: f64, step: f64) -> Kind {
    Kind::Float { min, max, step }
}

const fn choice(options: &'static [&'static str]) -> Kind {
    Kind::Choice(options)
}

const fn q(key: &'static str, label: &'static str, kind: Kind) -> Quick {
    Quick::new(key, label, kind)
}

/// `key=value` with no sections (MangoHud, makepkg.conf, environment.d).
const EQUALS_COMPACT: Dialect = Dialect {
    sep: Sep::Equals,
    prefix: "",
    sections: false,
    compact: true,
};

const fn prefixed(prefix: &'static str, sep: Sep, compact: bool) -> Dialect {
    Dialect {
        sep,
        prefix,
        sections: false,
        compact,
    }
}

const SPACE: Format = Format::Kv(Dialect::SPACE);
const EQUALS: Format = Format::Kv(Dialect::EQUALS);
const INI: Format = Format::Kv(Dialect::INI);
const INI_COMPACT: Format = Format::Kv(Dialect::INI_COMPACT);
const COMPACT: Format = Format::Kv(EQUALS_COMPACT);

const HASH_TEXT: Format = Format::Text(Syntax::Hash);
const SHELL_TEXT: Format = Format::Text(Syntax::Shell);
const LUA_TEXT: Format = Format::Text(Syntax::Lua);
const CSS_TEXT: Format = Format::Text(Syntax::Css);
const YAML_TEXT: Format = Format::Text(Syntax::Yaml);
const PLAIN_TEXT: Format = Format::Text(Syntax::Plain);

const CURSOR_SHAPES: &[&str] = &["block", "beam", "underline"];
const ANCHORS: &[&str] = &[
    "top-right",
    "top-center",
    "top-left",
    "bottom-right",
    "bottom-center",
    "bottom-left",
    "center",
];

// ---------------------------------------------------------------- terminals

const KITTY_QUICK: &[Quick] = &[
    q("font_family", "Font", Kind::Font),
    q("font_size", "Font size", float(6.0, 40.0, 0.5)).help("In points."),
    q("background_opacity", "Background opacity", float(0.0, 1.0, 0.05))
        .help("Needs a compositor. 1 is opaque."),
    q("window_padding_width", "Window padding", int(0, 64)).help("In points."),
    q("cursor_shape", "Cursor shape", choice(CURSOR_SHAPES)),
    q("cursor_blink_interval", "Cursor blink interval", float(0.0, 2.0, 0.1))
        .help("Seconds. 0 disables blinking."),
    q("tab_bar_style", "Tab bar style", choice(&["fade", "slant", "separator", "powerline", "hidden"])),
    q("enable_audio_bell", "Audio bell", YES_NO),
    q("confirm_os_window_close", "Confirm closing", int(-1, 10))
        .help("Ask when closing a window with at least this many tabs. 0 never asks."),
    q("background", "Background color", Kind::Color),
    q("foreground", "Text color", Kind::Color),
];

const KITTY_TEMPLATE: &str = "\
# kitty configuration — see https://sw.kovidgoyal.net/kitty/conf/
# Reload a running kitty with ctrl+shift+F5.

font_family      monospace
font_size        11.0

window_padding_width 8
# background_opacity 0.95

cursor_shape     block
enable_audio_bell no
tab_bar_style    powerline
";

const FOOT_QUICK: &[Quick] = &[
    q("main.font", "Font", TEXT).help("fontconfig pattern, e.g. JetBrains Mono:size=11"),
    q("main.pad", "Padding", TEXT).help("Horizontal × vertical, e.g. 8x8"),
    q("main.term", "TERM", choice(&["foot", "xterm-256color"])),
    q("colors.alpha", "Background opacity", float(0.0, 1.0, 0.05)),
    q("colors.background", "Background color", Kind::ColorBare),
    q("colors.foreground", "Text color", Kind::ColorBare),
    q("cursor.style", "Cursor shape", choice(&["block", "beam", "underline"])),
    q("cursor.blink", "Blinking cursor", YES_NO),
    q("scrollback.lines", "Scrollback lines", int(0, 1_000_000)),
];

const FOOT_TEMPLATE: &str = "\
# foot configuration — see man 5 foot.ini

[main]
font=monospace:size=11
pad=8x8

[scrollback]
lines=10000

[cursor]
style=block
blink=no

[colors]
alpha=1.0
";

const ALACRITTY_QUICK: &[Quick] = &[
    q("font.normal.family", "Font", Kind::Font),
    q("font.size", "Font size", float(6.0, 40.0, 0.5)),
    q("window.opacity", "Background opacity", float(0.0, 1.0, 0.05)),
    q("window.padding.x", "Horizontal padding", int(0, 64)),
    q("window.padding.y", "Vertical padding", int(0, 64)),
    q("window.decorations", "Window decorations", choice(&["Full", "None", "Transparent", "Buttonless"])),
    q("cursor.style.shape", "Cursor shape", choice(&["Block", "Beam", "Underline"])),
    q("cursor.style.blinking", "Cursor blinking", choice(&["Never", "Off", "On", "Always"])),
    q("scrolling.history", "Scrollback lines", int(0, 100_000)),
    q("colors.primary.background", "Background color", Kind::Color),
    q("colors.primary.foreground", "Text color", Kind::Color),
];

const ALACRITTY_TEMPLATE: &str = "\
# Alacritty configuration — see https://alacritty.org/config-alacritty.html

[font]
size = 11.0

[font.normal]
family = \"monospace\"

[window]
opacity = 1.0
decorations = \"Full\"

[window.padding]
x = 8
y = 8

[scrolling]
history = 10000
";

const GHOSTTY_QUICK: &[Quick] = &[
    q("font-family", "Font", Kind::Font),
    q("font-size", "Font size", float(6.0, 40.0, 0.5)),
    q("theme", "Theme", TEXT).help("Run `ghostty +list-themes` to see them all."),
    q("background-opacity", "Background opacity", float(0.0, 1.0, 0.05)),
    q("window-padding-x", "Horizontal padding", int(0, 64)),
    q("window-padding-y", "Vertical padding", int(0, 64)),
    q("cursor-style", "Cursor shape", choice(&["block", "bar", "underline", "block_hollow"])),
    q("window-decoration", "Window decorations", choice(&["auto", "client", "server", "none"])),
    q("gtk-titlebar", "GTK title bar", TRUE_FALSE),
    q("copy-on-select", "Copy on select", choice(&["true", "false", "clipboard"])),
    q("confirm-close-surface", "Confirm closing", TRUE_FALSE),
];

const GHOSTTY_TEMPLATE: &str = "\
# Ghostty configuration — see https://ghostty.org/docs/config
# Reload a running Ghostty with ctrl+shift+,

font-family = monospace
font-size = 11

window-padding-x = 8
window-padding-y = 8
# background-opacity = 0.95
";

const WEZTERM_TEMPLATE: &str = "\
-- WezTerm configuration — see https://wezfurlong.org/wezterm/config/files.html
local wezterm = require 'wezterm'
local config = wezterm.config_builder()

config.font = wezterm.font 'monospace'
config.font_size = 11.0
config.window_background_opacity = 1.0
config.hide_tab_bar_if_only_one_tab = true

return config
";

// ------------------------------------------------------------------- shells

const FISH_TEMPLATE: &str = "\
# fish configuration — runs for every new shell.

if status is-interactive
    # Commands to run in interactive sessions can go here
    set -g fish_greeting
end
";

const ZSH_TEMPLATE: &str = "\
# ~/.zshrc

HISTFILE=~/.zsh_history
HISTSIZE=10000
SAVEHIST=10000
setopt share_history hist_ignore_dups

autoload -Uz compinit && compinit
bindkey -e
";

const BASH_TEMPLATE: &str = "\
# ~/.bashrc

# If not running interactively, don't do anything
[[ $- != *i* ]] && return

alias ls='ls --color=auto'
alias grep='grep --color=auto'
PS1='[\\u@\\h \\W]\\$ '
";

const STARSHIP_QUICK: &[Quick] = &[
    q("add_newline", "Blank line between prompts", TRUE_FALSE),
    q("command_timeout", "Command timeout", int(50, 10_000)).help("Milliseconds."),
    q("character.success_symbol", "Prompt symbol", TEXT).help("e.g. [❯](bold green)"),
    q("character.error_symbol", "Prompt symbol after an error", TEXT),
    q("directory.truncation_length", "Directory depth", int(0, 20)),
    q("directory.truncate_to_repo", "Truncate to repository root", TRUE_FALSE),
    q("git_branch.symbol", "Git branch symbol", TEXT),
    q("cmd_duration.min_time", "Show durations longer than", int(0, 60_000)).help("Milliseconds."),
    q("line_break.disabled", "Single-line prompt", TRUE_FALSE),
];

const STARSHIP_TEMPLATE: &str = "\
# Starship prompt — see https://starship.rs/config/
\"$schema\" = 'https://starship.rs/config-schema.json'

add_newline = true

[character]
success_symbol = '[❯](bold green)'
error_symbol = '[❯](bold red)'

[directory]
truncation_length = 3
";

const TMUX_DIALECT: Dialect = prefixed("set -g ", Sep::Space, false);

const TMUX_QUICK: &[Quick] = &[
    q("prefix", "Prefix key", choice(&["C-b", "C-a", "C-Space", "C-s"])),
    q("mouse", "Mouse support", choice(&["on", "off"])),
    q("base-index", "First window number", int(0, 1)),
    q("renumber-windows", "Renumber windows", choice(&["on", "off"])),
    q("history-limit", "Scrollback lines", int(0, 1_000_000)),
    q("escape-time", "Escape delay", int(0, 1000)).help("Milliseconds. Low values feel better in vim."),
    q("default-terminal", "TERM", choice(&["tmux-256color", "screen-256color"])).quoted(),
    q("status-position", "Status bar position", choice(&["bottom", "top"])),
    q("mode-keys", "Copy-mode keys", choice(&["vi", "emacs"])),
];

const TMUX_TEMPLATE: &str = "\
# tmux configuration — reload with: tmux source-file ~/.config/tmux/tmux.conf

set -g mouse on
set -g base-index 1
set -g renumber-windows on
set -g history-limit 50000
set -g escape-time 10
set -g default-terminal \"tmux-256color\"
";

// ------------------------------------------------------------------ desktop

const HYPRLAND_TEMPLATE: &str = "\
# Hyprland configuration — see https://wiki.hypr.land/Configuring/
# The Hyprland page in this app manages common options for you.

monitor = , preferred, auto, 1

$terminal = kitty
$menu = fuzzel

general {
    gaps_in = 5
    gaps_out = 10
    border_size = 2
}

$mainMod = SUPER
bind = $mainMod, Return, exec, $terminal
bind = $mainMod, Space, exec, $menu
bind = $mainMod, Q, killactive
";

const HYPRLAND_LUA_TEMPLATE: &str = "\
-- Hyprland configuration (Lua) — see https://wiki.hypr.land/Configuring/

hl.monitor({ output = \"\", mode = \"preferred\", position = \"auto\", scale = 1 })

hl.config({
    general = {
        gaps_in = 5,
        gaps_out = 10,
        border_size = 2,
    },
})
";

const HYPRIDLE_TEMPLATE: &str = "\
# hypridle — see https://wiki.hypr.land/Hypr-Ecosystem/hypridle/

general {
    lock_cmd = pidof hyprlock || hyprlock
    before_sleep_cmd = loginctl lock-session
    after_sleep_cmd = hyprctl dispatch dpms on
}

# Dim after 5 minutes
listener {
    timeout = 300
    on-timeout = brightnessctl -s set 10%
    on-resume = brightnessctl -r
}

# Lock after 10 minutes
listener {
    timeout = 600
    on-timeout = loginctl lock-session
}

# Screen off after 11 minutes
listener {
    timeout = 660
    on-timeout = hyprctl dispatch dpms off
    on-resume = hyprctl dispatch dpms on
}
";

const HYPRLOCK_TEMPLATE: &str = "\
# hyprlock — see https://wiki.hypr.land/Hypr-Ecosystem/hyprlock/

general {
    hide_cursor = true
}

background {
    path = screenshot
    blur_passes = 3
}

input-field {
    size = 250, 50
    position = 0, -80
    halign = center
    valign = center
}

label {
    text = $TIME
    font_size = 64
    position = 0, 80
    halign = center
    valign = center
}
";

const HYPRPAPER_QUICK: &[Quick] = &[
    q("preload", "Preloaded image", TEXT).help("Path of a wallpaper to load into memory."),
    q("wallpaper", "Wallpaper", TEXT).help("monitor,path — leave the monitor empty for all."),
    q("splash", "Hyprland splash text", TRUE_FALSE),
    q("ipc", "IPC", choice(&["on", "off"])).help("Needed to change wallpapers with hyprctl."),
];

const HYPRPAPER_TEMPLATE: &str = "\
# hyprpaper — see https://wiki.hypr.land/Hypr-Ecosystem/hyprpaper/

preload = ~/Pictures/wallpaper.png
wallpaper = ,~/Pictures/wallpaper.png
splash = false
ipc = on
";

const SWAY_TEMPLATE: &str = "\
# sway configuration — start from /etc/sway/config and see man 5 sway

set $mod Mod4
set $term foot
set $menu fuzzel

output * bg ~/Pictures/wallpaper.png fill

bindsym $mod+Return exec $term
bindsym $mod+d exec $menu
bindsym $mod+Shift+q kill
bindsym $mod+Shift+c reload

include /etc/sway/config.d/*
";

const I3_TEMPLATE: &str = "\
# i3 configuration — see https://i3wm.org/docs/userguide.html

set $mod Mod4
font pango:monospace 10

bindsym $mod+Return exec i3-sensible-terminal
bindsym $mod+d exec --no-startup-id dmenu_run
bindsym $mod+Shift+q kill
bindsym $mod+Shift+r restart

bar {
    status_command i3status
}
";

const NIRI_TEMPLATE: &str = "\
// niri configuration — see https://github.com/YaLTeR/niri/wiki/Configuration:-Introduction
// A full default config is at /usr/share/doc/niri/default-config.kdl

input {
    keyboard {
        xkb {
            layout \"us\"
        }
    }
    touchpad {
        tap
        natural-scroll
    }
}

layout {
    gaps 16
    default-column-width { proportion 0.5; }
}

binds {
    Mod+Return { spawn \"foot\"; }
    Mod+D { spawn \"fuzzel\"; }
    Mod+Q { close-window; }
}
";

const PICOM_TEMPLATE: &str = "\
# picom — see man picom; options end with a semicolon.

backend = \"glx\";
vsync = true;

shadow = true;
shadow-radius = 12;

fading = true;
fade-delta = 4;

corner-radius = 8;
inactive-opacity = 1.0;
";

// --------------------------------------------------------------------- bars

const WAYBAR_QUICK: &[Quick] = &[
    q("layer", "Layer", choice(&["top", "bottom", "overlay"])),
    q("position", "Position", choice(&["top", "bottom", "left", "right"])),
    q("height", "Height", int(0, 200)).help("Pixels. 0 fits the content."),
    q("spacing", "Module spacing", int(0, 64)),
    q("margin-top", "Top margin", int(0, 100)),
    q("margin-left", "Left margin", int(0, 100)),
    q("margin-right", "Right margin", int(0, 100)),
    q("reload_style_on_change", "Reload style on change", TRUE_FALSE),
];

const WAYBAR_TEMPLATE: &str = r#"// Waybar configuration — see https://github.com/Alexays/Waybar/wiki/Configuration
{
    "layer": "top",
    "position": "top",
    "height": 32,
    "spacing": 6,
    "reload_style_on_change": true,
    "modules-left": ["hyprland/workspaces"],
    "modules-center": ["clock"],
    "modules-right": ["pulseaudio", "network", "battery", "tray"],
    "clock": {
        "format": "{:%a %d %b  %H:%M}"
    }
}
"#;

const WAYBAR_CSS: &str = "\
/* Waybar style — see https://github.com/Alexays/Waybar/wiki/Styling */
* {
    font-family: sans-serif;
    font-size: 13px;
}

window#waybar {
    background: rgba(20, 20, 24, 0.85);
    color: #e6e1e5;
}

#workspaces button.active {
    border-bottom: 2px solid #d0bcff;
}
";

const CAVA_QUICK: &[Quick] = &[
    q("general.framerate", "Frame rate", int(1, 240)),
    q("general.bars", "Bars", int(0, 512)).help("0 fits the window."),
    q("general.bar_width", "Bar width", int(1, 32)),
    q("general.bar_spacing", "Bar spacing", int(0, 32)),
    q("general.sensitivity", "Sensitivity", int(1, 500)).help("Percent."),
    q("input.method", "Audio input", choice(&["pipewire", "pulse", "alsa", "fifo"])),
    q("output.method", "Output", choice(&["ncurses", "noncurses", "raw", "sdl"])),
    q("color.gradient", "Gradient colors", ONE_ZERO),
    q("smoothing.noise_reduction", "Noise reduction", int(0, 100)),
];

const CAVA_TEMPLATE: &str = "\
## cava configuration — see https://github.com/karlstav/cava

[general]
framerate = 60
bars = 0
bar_width = 2
bar_spacing = 1

[input]
method = pipewire

[output]
method = ncurses

[color]
gradient = 0

[smoothing]
noise_reduction = 77
";

// ---------------------------------------------------------------- launchers

const ROFI_TEMPLATE: &str = "\
/* rofi configuration — see man rofi and `rofi -dump-config` */
configuration {
    modi: \"drun,run,window\";
    show-icons: true;
    terminal: \"kitty\";
    drun-display-format: \"{name}\";
}

@theme \"/usr/share/rofi/themes/Arc-Dark.rasi\"
";

const WOFI_QUICK: &[Quick] = &[
    q("show", "Default mode", choice(&["drun", "run", "dmenu"])),
    q("width", "Width", TEXT).help("Pixels or percent, e.g. 600 or 40%"),
    q("height", "Height", TEXT),
    q("prompt", "Prompt", TEXT),
    q("allow_images", "Show icons", TRUE_FALSE),
    q("insensitive", "Case-insensitive search", TRUE_FALSE),
    q("term", "Terminal", TEXT),
];

const WOFI_TEMPLATE: &str = "\
# wofi — see man 5 wofi
show=drun
width=600
height=400
prompt=Search
allow_images=true
insensitive=true
";

const WOFI_CSS: &str = "\
/* wofi style — see man 7 wofi */
window {
    border-radius: 16px;
    background-color: #1d1b20;
}

#entry:selected {
    background-color: #4a4458;
}
";

const FUZZEL_QUICK: &[Quick] = &[
    q("main.font", "Font", TEXT).help("fontconfig pattern, e.g. Inter:size=12"),
    q("main.terminal", "Terminal", TEXT).help("Used for Terminal=true apps, e.g. kitty -e"),
    q("main.prompt", "Prompt", TEXT).quoted(),
    q("main.lines", "Visible results", int(1, 50)),
    q("main.width", "Width", int(10, 200)).help("In characters."),
    q("main.icons-enabled", "Show icons", YES_NO),
    q("main.layer", "Layer", choice(&["top", "overlay"])),
    q("border.width", "Border width", int(0, 16)),
    q("border.radius", "Corner radius", int(0, 64)),
];

const FUZZEL_TEMPLATE: &str = "\
# fuzzel — see man 5 fuzzel.ini

[main]
font=sans-serif:size=12
terminal=kitty -e
prompt=\"❯ \"
lines=10
width=40
icons-enabled=yes

[border]
width=2
radius=16
";

const TOFI_QUICK: &[Quick] = &[
    q("font", "Font", TEXT).help("Font name or path to a font file."),
    q("font-size", "Font size", int(6, 72)),
    q("width", "Width", TEXT).help("Pixels or percent."),
    q("height", "Height", TEXT),
    q("corner-radius", "Corner radius", int(0, 64)),
    q("border-width", "Border width", int(0, 16)),
    q("outline-width", "Outline width", int(0, 16)),
    q("prompt-text", "Prompt", TEXT).quoted(),
    q("hide-cursor", "Hide the mouse cursor", TRUE_FALSE),
    q("matching-algorithm", "Matching", choice(&["normal", "prefix", "fuzzy"])),
];

const TOFI_TEMPLATE: &str = "\
# tofi — see man 5 tofi
font = sans-serif
font-size = 14
width = 640
height = 360
corner-radius = 16
border-width = 2
outline-width = 0
prompt-text = \"run: \"
matching-algorithm = fuzzy
";

// ------------------------------------------------------------ notifications

const DUNST_QUICK: &[Quick] = &[
    q("global.font", "Font", TEXT).help("Pango font, e.g. Inter 10"),
    q("global.origin", "Position", choice(&[
        "top-right", "top-center", "top-left", "bottom-right", "bottom-center", "bottom-left",
        "left-center", "right-center",
    ])),
    q("global.offset", "Offset", TEXT).help("Distance from the corner, e.g. (12, 48)"),
    q("global.width", "Width", int(50, 1000)),
    q("global.notification_limit", "Max visible", int(0, 50)).help("0 shows all."),
    q("global.corner_radius", "Corner radius", int(0, 64)),
    q("global.frame_width", "Border width", int(0, 16)),
    q("global.frame_color", "Border color", Kind::Color).quoted(),
    q("global.follow", "Follow", choice(&["none", "mouse", "keyboard"])),
    q("urgency_normal.timeout", "Timeout", int(0, 120)).help("Seconds. 0 never expires."),
];

const DUNST_TEMPLATE: &str = "\
# dunst — see man 5 dunst

[global]
font = sans-serif 10
origin = top-right
offset = (12, 48)
width = 320
notification_limit = 5
corner_radius = 12
frame_width = 2
frame_color = \"#d0bcff\"
follow = mouse

[urgency_low]
timeout = 5

[urgency_normal]
timeout = 8

[urgency_critical]
timeout = 0
";

const MAKO_QUICK: &[Quick] = &[
    q("font", "Font", TEXT).help("Pango font, e.g. Inter 10"),
    q("anchor", "Position", choice(ANCHORS)),
    q("width", "Width", int(50, 1000)),
    q("height", "Max height", int(20, 1000)),
    q("default-timeout", "Timeout", int(0, 120_000)).help("Milliseconds. 0 never expires."),
    q("max-visible", "Max visible", int(-1, 50)),
    q("border-size", "Border width", int(0, 16)),
    q("border-radius", "Corner radius", int(0, 64)),
    q("background-color", "Background color", Kind::Color),
    q("border-color", "Border color", Kind::Color),
    q("layer", "Layer", choice(&["background", "bottom", "top", "overlay"])),
];

const MAKO_TEMPLATE: &str = "\
# mako — see man 5 mako
font=sans-serif 10
anchor=top-right
width=320
default-timeout=8000
border-size=2
border-radius=12
background-color=#1d1b20
border-color=#d0bcff

[urgency=high]
default-timeout=0
";

const SWAYNC_QUICK: &[Quick] = &[
    q("positionX", "Horizontal position", choice(&["right", "center", "left"])),
    q("positionY", "Vertical position", choice(&["top", "center", "bottom"])),
    q("control-center-width", "Control center width", int(200, 1200)),
    q("notification-window-width", "Notification width", int(200, 1200)),
    q("timeout", "Timeout", int(0, 120)).help("Seconds."),
    q("timeout-low", "Timeout (low urgency)", int(0, 120)),
    q("timeout-critical", "Timeout (critical)", int(0, 120)).help("0 never expires."),
    q("fit-to-screen", "Full-height control center", TRUE_FALSE),
    q("hide-on-clear", "Hide when cleared", TRUE_FALSE),
];

const SWAYNC_TEMPLATE: &str = r#"{
    "$schema": "/etc/xdg/swaync/configSchema.json",
    "positionX": "right",
    "positionY": "top",
    "control-center-width": 500,
    "notification-window-width": 400,
    "timeout": 8,
    "timeout-low": 4,
    "timeout-critical": 0,
    "fit-to-screen": true,
    "hide-on-clear": false
}
"#;

const SWAYNC_CSS: &str = "\
/* swaync style — start from /etc/xdg/swaync/style.css */
.notification {
    border-radius: 16px;
}

.control-center {
    border-radius: 24px;
}
";

// ------------------------------------------------------------------ editors

const NVIM_TEMPLATE: &str = "\
-- Neovim configuration — see :help lua-guide

vim.g.mapleader = ' '

vim.opt.number = true
vim.opt.relativenumber = true
vim.opt.mouse = 'a'
vim.opt.clipboard = 'unnamedplus'
vim.opt.expandtab = true
vim.opt.shiftwidth = 4
vim.opt.tabstop = 4
vim.opt.ignorecase = true
vim.opt.smartcase = true
vim.opt.termguicolors = true
";

const HELIX_QUICK: &[Quick] = &[
    q("theme", "Theme", TEXT).help("Run `hx --health` or `:theme` to browse."),
    q("editor.line-number", "Line numbers", choice(&["absolute", "relative"])),
    q("editor.mouse", "Mouse support", TRUE_FALSE),
    q("editor.cursorline", "Highlight current line", TRUE_FALSE),
    q("editor.bufferline", "Buffer line", choice(&["never", "always", "multiple"])),
    q("editor.true-color", "Force true color", TRUE_FALSE),
    q("editor.auto-save", "Save on focus loss", TRUE_FALSE),
    q("editor.soft-wrap.enable", "Soft wrap", TRUE_FALSE),
    q("editor.cursor-shape.insert", "Insert-mode cursor", choice(&["block", "bar", "underline"])),
];

const HELIX_TEMPLATE: &str = "\
# Helix — see https://docs.helix-editor.com/configuration.html
theme = \"default\"

[editor]
line-number = \"relative\"
mouse = true
cursorline = true

[editor.cursor-shape]
insert = \"bar\"
normal = \"block\"
select = \"underline\"
";

const MICRO_QUICK: &[Quick] = &[
    q("colorscheme", "Color scheme", TEXT),
    q("tabsize", "Tab width", int(1, 16)),
    q("tabstospaces", "Indent with spaces", TRUE_FALSE),
    q("ruler", "Line numbers", TRUE_FALSE),
    q("softwrap", "Soft wrap", TRUE_FALSE),
    q("mouse", "Mouse support", TRUE_FALSE),
    q("scrollbar", "Scroll bar", TRUE_FALSE),
    q("clipboard", "Clipboard", choice(&["external", "terminal", "internal"])),
];

const MICRO_TEMPLATE: &str = r#"{
    "colorscheme": "default",
    "tabsize": 4,
    "tabstospaces": true
}
"#;

const VSCODE_QUICK: &[Quick] = &[
    q("editor.fontFamily", "Editor font", TEXT).help("CSS font list, e.g. 'JetBrains Mono', monospace"),
    q("editor.fontSize", "Editor font size", int(6, 48)),
    q("editor.tabSize", "Tab width", int(1, 16)),
    q("editor.wordWrap", "Word wrap", choice(&["off", "on", "wordWrapColumn", "bounded"])),
    q("editor.minimap.enabled", "Minimap", TRUE_FALSE),
    q("editor.formatOnSave", "Format on save", TRUE_FALSE),
    q("files.autoSave", "Auto save", choice(&["off", "afterDelay", "onFocusChange", "onWindowChange"])),
    q("workbench.colorTheme", "Color theme", TEXT),
    q("window.titleBarStyle", "Title bar", choice(&["native", "custom"])),
    q("terminal.integrated.fontSize", "Terminal font size", int(6, 48)),
];

const VSCODE_TEMPLATE: &str = r#"{
    // VS Code user settings — see https://code.visualstudio.com/docs/getstarted/settings
    "editor.fontSize": 14,
    "window.titleBarStyle": "native"
}
"#;

const ZED_QUICK: &[Quick] = &[
    q("buffer_font_family", "Editor font", Kind::Font),
    q("buffer_font_size", "Editor font size", int(6, 48)),
    q("ui_font_size", "Interface font size", int(6, 48)),
    q("tab_size", "Tab width", int(1, 16)),
    q("vim_mode", "Vim mode", TRUE_FALSE),
    q("soft_wrap", "Soft wrap", choice(&["none", "editor_width", "preferred_line_length", "bounded"])),
    q("format_on_save", "Format on save", choice(&["on", "off"])),
    q("relative_line_numbers", "Relative line numbers", TRUE_FALSE),
];

const ZED_TEMPLATE: &str = r#"// Zed settings — see https://zed.dev/docs/configuring-zed
{
    "ui_font_size": 16,
    "buffer_font_size": 15
}
"#;

// -------------------------------------------------------------------- media

const MPV_QUICK: &[Quick] = &[
    q("profile", "Quality profile", choice(&["high-quality", "fast", "gpu-hq"])),
    q("vo", "Video output", choice(&["gpu-next", "gpu", "dmabuf-wayland"])),
    q("hwdec", "Hardware decoding", choice(&["auto-safe", "auto", "no", "vaapi", "nvdec"])),
    q("save-position-on-quit", "Resume playback", YES_NO),
    q("keep-open", "Keep open at the end", choice(&["no", "yes", "always"])),
    q("volume", "Start volume", int(0, 130)),
    q("slang", "Subtitle languages", TEXT).help("Comma separated, e.g. en,eng"),
    q("alang", "Audio languages", TEXT),
    q("sub-auto", "Load external subtitles", choice(&["no", "exact", "fuzzy", "all"])),
    q("screenshot-format", "Screenshot format", choice(&["png", "jpg", "webp", "avif"])),
];

const MPV_TEMPLATE: &str = "\
# mpv — see https://mpv.io/manual/stable/
profile=high-quality
vo=gpu-next
hwdec=auto-safe
save-position-on-quit=yes
volume=70
";

const ZATHURA_DIALECT: Dialect = prefixed("set ", Sep::Space, false);

const ZATHURA_QUICK: &[Quick] = &[
    q("selection-clipboard", "Copy selections to", choice(&["clipboard", "primary"])),
    q("recolor", "Dark mode (recolor pages)", TRUE_FALSE),
    q("recolor-darkcolor", "Recolor text", Kind::Color).quoted(),
    q("recolor-lightcolor", "Recolor background", Kind::Color).quoted(),
    q("default-bg", "Background color", Kind::Color).quoted(),
    q("font", "Interface font", TEXT).quoted().help("Pango font, e.g. monospace normal 10"),
    q("adjust-open", "Fit when opening", choice(&["best-fit", "width"])),
    q("scroll-step", "Scroll step", int(10, 500)),
    q("guioptions", "Interface elements", TEXT).quoted().help("c=command line, s=status bar, h/v=scroll bars"),
];

const ZATHURA_TEMPLATE: &str = "\
# zathura — see man 5 zathurarc
set selection-clipboard clipboard
set adjust-open \"best-fit\"
set recolor false
";

const MANGOHUD_QUICK: &[Quick] = &[
    q("fps", "Frame rate", FLAG),
    q("frametime", "Frame time", FLAG),
    q("frame_timing", "Frame time graph", FLAG),
    q("cpu_stats", "CPU usage", FLAG),
    q("cpu_temp", "CPU temperature", FLAG),
    q("gpu_stats", "GPU usage", FLAG),
    q("gpu_temp", "GPU temperature", FLAG),
    q("ram", "RAM usage", FLAG),
    q("vram", "VRAM usage", FLAG),
    q("fps_limit", "FPS limit", int(0, 1000)).help("0 is unlimited."),
    q("position", "Position", choice(&["top-left", "top-right", "bottom-left", "bottom-right", "top-center"])),
    q("font_size", "Font size", int(8, 64)),
    q("background_alpha", "Background opacity", float(0.0, 1.0, 0.05)),
    q("toggle_hud", "Toggle key", TEXT).help("e.g. Shift_R+F12"),
    q("no_display", "Hidden until toggled", FLAG),
];

const MANGOHUD_TEMPLATE: &str = "\
# MangoHud — see https://github.com/flightlessmango/MangoHud
# Enable for a game with: mangohud %command%

fps
frametime
gpu_stats
cpu_stats
ram
position=top-left
font_size=20
toggle_hud=Shift_R+F12
";

// -------------------------------------------------------------------- tools

const BTOP_QUICK: &[Quick] = &[
    q("color_theme", "Theme", choice(&["Default", "TTY", "gruvbox_dark", "nord", "dracula", "tokyo-night"])).quoted(),
    q("theme_background", "Theme background", CAPITAL),
    q("truecolor", "True color", CAPITAL),
    q("vim_keys", "Vim keys", CAPITAL),
    q("rounded_corners", "Rounded corners", CAPITAL),
    q("update_ms", "Update interval", int(100, 10_000)).help("Milliseconds."),
    q("shown_boxes", "Boxes", TEXT).quoted().help("Any of: cpu mem net proc gpu0"),
    q("proc_sorting", "Sort processes by", choice(&["cpu lazy", "cpu direct", "memory", "pid", "name", "command", "threads", "user"])).quoted(),
    q("graph_symbol", "Graph style", choice(&["braille", "block", "tty"])).quoted(),
    q("temp_scale", "Temperature unit", choice(&["celsius", "fahrenheit", "kelvin"])).quoted(),
];

const BTOP_TEMPLATE: &str = "\
#? btop configuration — btop rewrites this file when you change options in it.
color_theme = \"Default\"
theme_background = True
truecolor = True
vim_keys = False
rounded_corners = True
update_ms = 2000
shown_boxes = \"cpu mem net proc\"
";

const FASTFETCH_QUICK: &[Quick] = &[
    q("logo/type", "Logo", choice(&["auto", "builtin", "small", "file", "kitty", "sixel", "chafa", "none"])),
    q("logo/source", "Logo source", TEXT).help("Distro name or image path."),
    q("logo/padding/top", "Logo top padding", int(0, 20)),
    q("display/separator", "Separator", TEXT),
    q("display/color/keys", "Key color", choice(&["blue", "magenta", "cyan", "green", "red", "yellow"])),
];

const FASTFETCH_TEMPLATE: &str = r#"// fastfetch — see `fastfetch --gen-config` for every option
{
    "$schema": "https://github.com/fastfetch-cli/fastfetch/raw/dev/doc/json_schema.json",
    "logo": {
        "type": "auto",
        "padding": {
            "top": 1
        }
    },
    "display": {
        "separator": "  ",
        "color": {
            "keys": "blue"
        }
    },
    "modules": [
        "title", "separator", "os", "kernel", "uptime", "packages", "shell",
        "de", "wm", "terminal", "cpu", "gpu", "memory", "disk", "break", "colors"
    ]
}
"#;

const YAZI_QUICK: &[Quick] = &[
    q("mgr.show_hidden", "Show hidden files", TRUE_FALSE),
    q("mgr.show_symlink", "Show symlink targets", TRUE_FALSE),
    q("mgr.sort_by", "Sort by", choice(&["natural", "alphabetical", "mtime", "btime", "size", "extension"])),
    q("mgr.sort_dir_first", "Folders first", TRUE_FALSE),
    q("mgr.sort_reverse", "Reverse order", TRUE_FALSE),
    q("mgr.linemode", "Line info", choice(&["none", "size", "mtime", "btime", "permissions", "owner"])),
    q("preview.max_width", "Max preview width", int(100, 4000)),
    q("preview.max_height", "Max preview height", int(100, 4000)),
];

const YAZI_TEMPLATE: &str = "\
# yazi — see https://yazi-rs.github.io/docs/configuration/yazi

[mgr]
show_hidden = false
sort_by = \"natural\"
sort_dir_first = true
linemode = \"size\"
";

const RANGER_DIALECT: Dialect = prefixed("set ", Sep::Space, false);

const RANGER_QUICK: &[Quick] = &[
    q("show_hidden", "Show hidden files", TRUE_FALSE),
    q("preview_images", "Image previews", TRUE_FALSE),
    q("preview_images_method", "Image preview method", choice(&["kitty", "sixel", "ueberzug", "w3m", "iterm2"])),
    q("draw_borders", "Borders", choice(&["none", "both", "outline", "separators"])),
    q("vcs_aware", "Git status", TRUE_FALSE),
    q("column_ratios", "Column ratios", TEXT).help("e.g. 1,3,4"),
    q("sort_directories_first", "Folders first", TRUE_FALSE),
];

const RANGER_TEMPLATE: &str = "\
# ranger — see `ranger --copy-config=rc` for every option
set show_hidden false
set preview_images true
set preview_images_method kitty
set draw_borders both
";

const LAZYGIT_TEMPLATE: &str = "\
# lazygit — see https://github.com/jesseduffield/lazygit/blob/master/docs/Config.md
gui:
  nerdFontsVersion: \"3\"
  showIcons: true
git:
  paging:
    colorArg: always
";

const GIT_QUICK: &[Quick] = &[
    q("user.name", "Name", TEXT),
    q("user.email", "Email", TEXT),
    q("init.defaultBranch", "Default branch", choice(&["main", "master"])),
    q("core.editor", "Editor", choice(&["nvim", "vim", "nano", "hx", "code --wait"])),
    q("pull.rebase", "Rebase on pull", TRUE_FALSE),
    q("push.autoSetupRemote", "Push new branches automatically", TRUE_FALSE),
    q("merge.conflictstyle", "Conflict style", choice(&["merge", "diff3", "zdiff3"])),
    q("color.ui", "Colors", choice(&["auto", "always", "false"])),
];

const GIT_TEMPLATE: &str = "\
# Git configuration — see man git-config
[user]
\tname = Your Name
\temail = you@example.com
[init]
\tdefaultBranch = main
[pull]
\trebase = false
";

const SSH_TEMPLATE: &str = "\
# SSH client configuration — see man ssh_config

Host *
    AddKeysToAgent yes
    ServerAliveInterval 60

# Host myserver
#     HostName example.com
#     User me
#     IdentityFile ~/.ssh/id_ed25519
";

const BAT_DIALECT: Dialect = prefixed("--", Sep::Equals, true);

const BAT_QUICK: &[Quick] = &[
    q("theme", "Theme", TEXT).quoted().help("Run `bat --list-themes`."),
    q("style", "Style", TEXT).quoted().help("e.g. numbers,changes,header"),
    q("paging", "Paging", choice(&["auto", "never", "always"])),
    q("italic-text", "Italic text", choice(&["never", "always"])),
    q("tabs", "Tab width", int(0, 16)),
];

const BAT_TEMPLATE: &str = "\
# bat — each line is a command-line option, see `bat --help`
--theme=\"TwoDark\"
--style=\"numbers,changes,header\"
--italic-text=always
";

// ------------------------------------------------------------------ theming

const QTCT_QUICK: &[Quick] = &[
    q("Appearance.style", "Widget style", choice(&["Fusion", "kvantum", "Breeze", "Windows"])),
    q("Appearance.icon_theme", "Icon theme", TEXT),
    q("Appearance.custom_palette", "Custom color palette", TRUE_FALSE),
    q("Appearance.color_scheme_path", "Color scheme file", TEXT),
    q("Appearance.standard_dialogs", "File dialogs", choice(&["default", "xdgdesktopportal", "gtk3"])),
];

const QT6CT_TEMPLATE: &str = "\
[Appearance]
style=Fusion
icon_theme=Papirus-Dark
custom_palette=false
standard_dialogs=xdgdesktopportal
";

const GTK_QUICK: &[Quick] = &[
    q("Settings.gtk-theme-name", "Theme", TEXT),
    q("Settings.gtk-icon-theme-name", "Icon theme", TEXT),
    q("Settings.gtk-cursor-theme-name", "Cursor theme", TEXT),
    q("Settings.gtk-cursor-theme-size", "Cursor size", int(8, 128)),
    q("Settings.gtk-font-name", "Font", TEXT).help("e.g. Inter 11"),
    q("Settings.gtk-application-prefer-dark-theme", "Prefer dark", TRUE_FALSE),
];

const GTK_TEMPLATE: &str = "\
[Settings]
gtk-theme-name=Adwaita
gtk-icon-theme-name=Adwaita
gtk-cursor-theme-name=Adwaita
gtk-cursor-theme-size=24
gtk-font-name=Sans 11
gtk-application-prefer-dark-theme=true
";

const KVANTUM_QUICK: &[Quick] = &[
    q("General.theme", "Theme", TEXT).help("A folder name from ~/.config/Kvantum or /usr/share/Kvantum."),
];

const KVANTUM_TEMPLATE: &str = "\
[General]
theme=KvArcDark
";

const FONTCONFIG_TEMPLATE: &str = r#"<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<!-- Font preferences — see man fonts-conf. Run fc-cache after editing. -->
<fontconfig>
  <alias>
    <family>sans-serif</family>
    <prefer><family>Inter</family></prefer>
  </alias>
  <alias>
    <family>monospace</family>
    <prefer><family>JetBrains Mono</family></prefer>
  </alias>
  <match target="font">
    <edit name="antialias" mode="assign"><bool>true</bool></edit>
    <edit name="hintstyle" mode="assign"><const>hintslight</const></edit>
    <edit name="rgba" mode="assign"><const>rgb</const></edit>
  </match>
</fontconfig>
"#;

// ------------------------------------------------------------------- system

const PARU_QUICK: &[Quick] = &[
    q("options.BottomUp", "Best matches at the bottom", FLAG),
    q("options.SudoLoop", "Keep sudo alive", FLAG).help("Avoids repeated password prompts during long builds."),
    q("options.CleanAfter", "Clean build files after install", FLAG),
    q("options.NewsOnUpgrade", "Show Arch news on upgrade", FLAG),
    q("options.RemoveMake", "Remove make dependencies", FLAG),
    q("options.CombinedUpgrade", "Combined repo + AUR upgrade", FLAG),
    q("options.UpgradeMenu", "Upgrade menu", FLAG),
    q("options.SkipReview", "Skip PKGBUILD review", FLAG).help("Not recommended: AUR packages can contain anything."),
];

const PARU_TEMPLATE: &str = "\
#
# /etc/paru.conf — see man paru.conf
#
[options]
PgpFetch
Devel
Provides
DevelSuffixes = -git -cvs -svn -bzr -darcs -always -hg -fossil
BottomUp
#SudoLoop
#CleanAfter
#NewsOnUpgrade
";

const YAY_QUICK: &[Quick] = &[
    q("sudoloop", "Keep sudo alive", TRUE_FALSE),
    q("cleanAfter", "Clean build files after install", TRUE_FALSE),
    q("devel", "Check -git packages for updates", TRUE_FALSE),
    q("combinedupgrade", "Combined repo + AUR upgrade", TRUE_FALSE),
    q("diffmenu", "Show diffs", TRUE_FALSE),
    q("editmenu", "Offer to edit PKGBUILDs", TRUE_FALSE),
    q("batchinstall", "Batch install", TRUE_FALSE),
];

const YAY_TEMPLATE: &str = r#"{
    "sudoloop": false,
    "cleanAfter": false,
    "devel": false,
    "combinedupgrade": true,
    "diffmenu": true,
    "editmenu": false
}
"#;

const MAKEPKG_QUICK: &[Quick] = &[
    q("MAKEFLAGS", "Make flags", TEXT).quoted().help("e.g. -j8 to build with 8 jobs. Speeds up AUR builds."),
    q("PKGEXT", "Package compression", choice(&[".pkg.tar.zst", ".pkg.tar"])).quoted()
        .help(".pkg.tar skips compression for faster local builds."),
    q("BUILDDIR", "Build folder", TEXT).help("e.g. /tmp/makepkg to build in RAM."),
    q("PACKAGER", "Packager", TEXT).quoted().help("Name <email>"),
];

const MAKEPKG_TEMPLATE: &str = "\
# User overrides for /etc/makepkg.conf — see man makepkg.conf
MAKEFLAGS=\"-j$(nproc)\"
#BUILDDIR=/tmp/makepkg
#PKGEXT=\".pkg.tar\"
";

const ENV_QUICK: &[Quick] = &[
    q("EDITOR", "Editor", choice(&["nvim", "vim", "nano", "hx", "micro"])),
    q("QT_QPA_PLATFORMTHEME", "Qt platform theme", choice(&["qt6ct", "qt5ct", "gtk3", "kde"])),
    q("GTK_THEME", "GTK theme override", TEXT),
    q("XCURSOR_THEME", "Cursor theme", TEXT),
    q("XCURSOR_SIZE", "Cursor size", int(8, 128)),
    q("ELECTRON_OZONE_PLATFORM_HINT", "Electron apps", choice(&["auto", "wayland", "x11"])),
    q("MOZ_ENABLE_WAYLAND", "Firefox on Wayland", ONE_ZERO),
];

const ENV_TEMPLATE: &str = "\
# Session environment variables — see man environment.d
# Applied at your next login.
EDITOR=nvim
QT_QPA_PLATFORMTHEME=qt6ct
ELECTRON_OZONE_PLATFORM_HINT=auto
";

const USER_DIRS_QUICK: &[Quick] = &[
    q("XDG_DESKTOP_DIR", "Desktop", TEXT).quoted(),
    q("XDG_DOWNLOAD_DIR", "Downloads", TEXT).quoted(),
    q("XDG_DOCUMENTS_DIR", "Documents", TEXT).quoted(),
    q("XDG_MUSIC_DIR", "Music", TEXT).quoted(),
    q("XDG_PICTURES_DIR", "Pictures", TEXT).quoted(),
    q("XDG_VIDEOS_DIR", "Videos", TEXT).quoted(),
    q("XDG_TEMPLATES_DIR", "Templates", TEXT).quoted(),
    q("XDG_PUBLICSHARE_DIR", "Public", TEXT).quoted(),
];

const USER_DIRS_TEMPLATE: &str = "\
# Folders used by xdg-user-dirs. Paths must start with $HOME.
XDG_DESKTOP_DIR=\"$HOME/Desktop\"
XDG_DOWNLOAD_DIR=\"$HOME/Downloads\"
XDG_DOCUMENTS_DIR=\"$HOME/Documents\"
XDG_MUSIC_DIR=\"$HOME/Music\"
XDG_PICTURES_DIR=\"$HOME/Pictures\"
XDG_VIDEOS_DIR=\"$HOME/Videos\"
XDG_TEMPLATES_DIR=\"$HOME/Templates\"
XDG_PUBLICSHARE_DIR=\"$HOME/Public\"
";

const MIMEAPPS_QUICK: &[Quick] = &[
    q("Default Applications.x-scheme-handler/http", "Web links", TEXT).help("A .desktop file name, e.g. firefox.desktop"),
    q("Default Applications.x-scheme-handler/https", "Secure web links", TEXT),
    q("Default Applications.inode/directory", "Folders", TEXT),
    q("Default Applications.text/plain", "Text files", TEXT),
    q("Default Applications.application/pdf", "PDFs", TEXT),
    q("Default Applications.image/png", "PNG images", TEXT),
];

const MIMEAPPS_TEMPLATE: &str = "\
[Default Applications]
x-scheme-handler/http=firefox.desktop
x-scheme-handler/https=firefox.desktop
inode/directory=org.gnome.Nautilus.desktop

[Added Associations]
";

// ------------------------------------------------------------------ catalog

/// Rules here load after Material 3 Browser's own userChrome and survive
/// its updates. Restart the browser to apply.
const M3_BROWSER_CSS_TEMPLATE: &str = "/* Your own userChrome rules. Material 3 Browser never overwrites this file.\n   Restart the browser to apply. Example: wider vertical tabs\n\n#navigator-toolbox { --zen-sidebar-width: 280px !important; }\n*/\n";

pub static APPS: &[AppDef] = &[
    // Terminals
    AppDef::new("kitty", "kitty", ICON_TERMINAL, Category::Terminal,
        "Fast, GPU-accelerated terminal with tabs, splits and images.",
        "kitty", "https://sw.kovidgoyal.net/kitty/conf/",
        &[ConfigSpec::new("kitty.conf", &["~/.config/kitty/kitty.conf"], SPACE, KITTY_TEMPLATE, KITTY_QUICK)])
        .reload("pkill -USR1 -x kitty"),
    AppDef::new("foot", "foot", ICON_TERMINAL, Category::Terminal,
        "Lightweight, minimal Wayland terminal.",
        "foot", "https://codeberg.org/dnkl/foot/src/branch/master/foot.ini",
        &[ConfigSpec::new("foot.ini", &["~/.config/foot/foot.ini"], INI_COMPACT, FOOT_TEMPLATE, FOOT_QUICK)]),
    AppDef::new("alacritty", "Alacritty", ICON_TERMINAL, Category::Terminal,
        "Simple, OpenGL terminal. Reloads its config live.",
        "alacritty", "https://alacritty.org/config-alacritty.html",
        &[ConfigSpec::new("alacritty.toml", &["~/.config/alacritty/alacritty.toml"], Format::Toml, ALACRITTY_TEMPLATE, ALACRITTY_QUICK)]),
    AppDef::new("ghostty", "Ghostty", ICON_TERMINAL, Category::Terminal,
        "Fast, native-feeling terminal with built-in themes.",
        "ghostty", "https://ghostty.org/docs/config/reference",
        &[ConfigSpec::new("config", &["~/.config/ghostty/config", "~/.config/ghostty/config.ghostty"], EQUALS, GHOSTTY_TEMPLATE, GHOSTTY_QUICK)]),
    AppDef::new("wezterm", "WezTerm", ICON_TERMINAL, Category::Terminal,
        "Terminal and multiplexer configured in Lua.",
        "wezterm", "https://wezfurlong.org/wezterm/config/files.html",
        &[ConfigSpec::new("wezterm.lua", &["~/.config/wezterm/wezterm.lua", "~/.wezterm.lua"], LUA_TEXT, WEZTERM_TEMPLATE, &[])]),
    // Shells & prompts
    AppDef::new("fish", "fish", ICON_TERMINAL, Category::Shell,
        "Friendly interactive shell with autosuggestions.",
        "fish", "https://fishshell.com/docs/current/",
        &[ConfigSpec::new("config.fish", &["~/.config/fish/config.fish"], SHELL_TEXT, FISH_TEMPLATE, &[])]),
    AppDef::new("zsh", "Zsh", ICON_TERMINAL, Category::Shell,
        "The Z shell.",
        "zsh", "https://zsh.sourceforge.io/Doc/",
        &[ConfigSpec::new(".zshrc", &["~/.zshrc"], SHELL_TEXT, ZSH_TEMPLATE, &[])]),
    AppDef::new("bash", "Bash", ICON_TERMINAL, Category::Shell,
        "The default shell on Arch.",
        "bash", "https://wiki.archlinux.org/title/Bash",
        &[
            ConfigSpec::new(".bashrc", &["~/.bashrc"], SHELL_TEXT, BASH_TEMPLATE, &[]),
            ConfigSpec::new(".bash_profile", &["~/.bash_profile"], SHELL_TEXT, "#\n# ~/.bash_profile\n#\n\n[[ -f ~/.bashrc ]] && . ~/.bashrc\n", &[]),
        ]),
    AppDef::new("starship", "Starship", ICON_ROCKET_LAUNCH, Category::Shell,
        "Fast, customizable prompt for any shell.",
        "starship", "https://starship.rs/config/",
        &[ConfigSpec::new("starship.toml", &["~/.config/starship.toml"], Format::Toml, STARSHIP_TEMPLATE, STARSHIP_QUICK)]),
    AppDef::new("tmux", "tmux", ICON_SPLITSCREEN, Category::Shell,
        "Terminal multiplexer: sessions, windows and panes.",
        "tmux", "https://github.com/tmux/tmux/wiki/Getting-Started",
        &[ConfigSpec::new("tmux.conf", &["~/.config/tmux/tmux.conf", "~/.tmux.conf"], Format::Kv(TMUX_DIALECT), TMUX_TEMPLATE, TMUX_QUICK)])
        .reload("tmux source-file ~/.config/tmux/tmux.conf 2>/dev/null || tmux source-file ~/.tmux.conf"),
    // Desktop
    AppDef::new("hyprland", "Hyprland", ICON_WATER_DROP, Category::Desktop,
        "Dynamic tiling Wayland compositor. Common options are on the Hyprland page.",
        "hyprland", "https://wiki.hypr.land/Configuring/",
        &[
            ConfigSpec::new("hyprland.conf", &["~/.config/hypr/hyprland.conf"], HASH_TEXT, HYPRLAND_TEMPLATE, &[]),
            ConfigSpec::new("hyprland.lua", &["~/.config/hypr/hyprland.lua"], LUA_TEXT, HYPRLAND_LUA_TEMPLATE, &[]),
        ])
        .bin("Hyprland")
        .reload("hyprctl reload"),
    AppDef::new("hypridle", "hypridle", ICON_BEDTIME, Category::Desktop,
        "Idle daemon: dim, lock and turn off screens after a timeout.",
        "hypridle", "https://wiki.hypr.land/Hypr-Ecosystem/hypridle/",
        &[ConfigSpec::new("hypridle.conf", &["~/.config/hypr/hypridle.conf"], HASH_TEXT, HYPRIDLE_TEMPLATE, &[])])
        .reload("pkill -x hypridle; setsid hypridle >/dev/null 2>&1 &"),
    AppDef::new("hyprlock", "hyprlock", ICON_LOCK, Category::Desktop,
        "Hyprland's GPU-accelerated lock screen.",
        "hyprlock", "https://wiki.hypr.land/Hypr-Ecosystem/hyprlock/",
        &[ConfigSpec::new("hyprlock.conf", &["~/.config/hypr/hyprlock.conf"], HASH_TEXT, HYPRLOCK_TEMPLATE, &[])]),
    AppDef::new("hyprpaper", "hyprpaper", ICON_WALLPAPER, Category::Desktop,
        "Wallpaper utility for Hyprland.",
        "hyprpaper", "https://wiki.hypr.land/Hypr-Ecosystem/hyprpaper/",
        &[ConfigSpec::new("hyprpaper.conf", &["~/.config/hypr/hyprpaper.conf"], EQUALS, HYPRPAPER_TEMPLATE, HYPRPAPER_QUICK)])
        .reload("pkill -x hyprpaper; setsid hyprpaper >/dev/null 2>&1 &"),
    AppDef::new("sway", "Sway", ICON_VIEW_QUILT, Category::Desktop,
        "i3-compatible tiling Wayland compositor.",
        "sway", "https://github.com/swaywm/sway/wiki",
        &[ConfigSpec::new("config", &["~/.config/sway/config"], HASH_TEXT, SWAY_TEMPLATE, &[])])
        .reload("swaymsg reload"),
    AppDef::new("i3", "i3", ICON_VIEW_QUILT, Category::Desktop,
        "Tiling window manager for X11.",
        "i3-wm", "https://i3wm.org/docs/userguide.html",
        &[ConfigSpec::new("config", &["~/.config/i3/config", "~/.i3/config"], HASH_TEXT, I3_TEMPLATE, &[])])
        .bin("i3")
        .reload("i3-msg reload"),
    AppDef::new("niri", "niri", ICON_VIEW_WEEK, Category::Desktop,
        "Scrollable-tiling Wayland compositor. Reloads its config live.",
        "niri", "https://github.com/YaLTeR/niri/wiki/Configuration:-Introduction",
        &[ConfigSpec::new("config.kdl", &["~/.config/niri/config.kdl"], PLAIN_TEXT, NIRI_TEMPLATE, &[])]),
    AppDef::new("picom", "picom", ICON_BLUR_ON, Category::Desktop,
        "Compositor for X11: shadows, fading, rounded corners, blur.",
        "picom", "https://picom.app/",
        &[ConfigSpec::new("picom.conf", &["~/.config/picom/picom.conf", "~/.config/picom.conf"], HASH_TEXT, PICOM_TEMPLATE, &[])]),
    // Bars & widgets
    AppDef::new("waybar", "Waybar", ICON_TOOLBAR, Category::Bar,
        "Highly customizable Wayland status bar.",
        "waybar", "https://github.com/Alexays/Waybar/wiki/Configuration",
        &[
            ConfigSpec::new("Config", &["~/.config/waybar/config.jsonc", "~/.config/waybar/config"], Format::Json, WAYBAR_TEMPLATE, WAYBAR_QUICK),
            ConfigSpec::new("Style", &["~/.config/waybar/style.css"], CSS_TEXT, WAYBAR_CSS, &[]),
        ])
        .reload("pkill -SIGUSR2 -x waybar"),
    AppDef::new("cava", "cava", ICON_GRAPHIC_EQ, Category::Bar,
        "Console audio visualizer.",
        "cava", "https://github.com/karlstav/cava",
        &[ConfigSpec::new("config", &["~/.config/cava/config"], INI, CAVA_TEMPLATE, CAVA_QUICK)])
        .reload("pkill -USR1 -x cava"),
    // Launchers
    AppDef::new("rofi", "rofi", ICON_APPS, Category::Launcher,
        "Window switcher and app launcher (X11 and Wayland).",
        "rofi", "https://github.com/davatorium/rofi/blob/next/doc/rofi.1.markdown",
        &[ConfigSpec::new("config.rasi", &["~/.config/rofi/config.rasi"], CSS_TEXT, ROFI_TEMPLATE, &[])]),
    AppDef::new("wofi", "wofi", ICON_APPS, Category::Launcher,
        "GTK launcher for wlroots compositors.",
        "wofi", "https://man.archlinux.org/man/wofi.5",
        &[
            ConfigSpec::new("config", &["~/.config/wofi/config"], COMPACT, WOFI_TEMPLATE, WOFI_QUICK),
            ConfigSpec::new("Style", &["~/.config/wofi/style.css"], CSS_TEXT, WOFI_CSS, &[]),
        ]),
    AppDef::new("fuzzel", "fuzzel", ICON_SEARCH, Category::Launcher,
        "Fast app launcher and dmenu replacement for Wayland.",
        "fuzzel", "https://man.archlinux.org/man/fuzzel.ini.5",
        &[ConfigSpec::new("fuzzel.ini", &["~/.config/fuzzel/fuzzel.ini"], INI_COMPACT, FUZZEL_TEMPLATE, FUZZEL_QUICK)]),
    AppDef::new("tofi", "tofi", ICON_SEARCH, Category::Launcher,
        "Tiny, very fast launcher for wlroots compositors.",
        "tofi", "https://github.com/philj56/tofi",
        &[ConfigSpec::new("config", &["~/.config/tofi/config"], EQUALS, TOFI_TEMPLATE, TOFI_QUICK)])
        .aur(),
    // Notifications
    AppDef::new("dunst", "dunst", ICON_NOTIFICATIONS, Category::Notifications,
        "Lightweight, configurable notification daemon.",
        "dunst", "https://dunst-project.org/documentation/",
        &[ConfigSpec::new("dunstrc", &["~/.config/dunst/dunstrc"], INI, DUNST_TEMPLATE, DUNST_QUICK)])
        .reload("dunstctl reload"),
    AppDef::new("mako", "mako", ICON_NOTIFICATIONS, Category::Notifications,
        "Notification daemon for Wayland.",
        "mako", "https://man.archlinux.org/man/mako.5",
        &[ConfigSpec::new("config", &["~/.config/mako/config"], INI_COMPACT, MAKO_TEMPLATE, MAKO_QUICK)])
        .reload("makoctl reload"),
    AppDef::new("swaync", "SwayNC", ICON_NOTIFICATIONS_ACTIVE, Category::Notifications,
        "Notification daemon with a control center panel.",
        "swaync", "https://github.com/ErikReider/SwayNotificationCenter",
        &[
            ConfigSpec::new("config.json", &["~/.config/swaync/config.json"], Format::Json, SWAYNC_TEMPLATE, SWAYNC_QUICK),
            ConfigSpec::new("Style", &["~/.config/swaync/style.css"], CSS_TEXT, SWAYNC_CSS, &[]),
        ])
        .reload("swaync-client -R -rs"),
    // Editors
    AppDef::new("neovim", "Neovim", ICON_EDIT_NOTE, Category::Editor,
        "Hyperextensible Vim-based editor.",
        "neovim", "https://neovim.io/doc/user/lua-guide.html",
        &[ConfigSpec::new("init.lua", &["~/.config/nvim/init.lua"], LUA_TEXT, NVIM_TEMPLATE, &[])])
        .bin("nvim"),
    AppDef::new("helix", "Helix", ICON_EDIT_NOTE, Category::Editor,
        "Modal editor with LSP and tree-sitter built in.",
        "helix", "https://docs.helix-editor.com/configuration.html",
        &[ConfigSpec::new("config.toml", &["~/.config/helix/config.toml"], Format::Toml, HELIX_TEMPLATE, HELIX_QUICK)])
        .bin("helix")
        .reload("pkill -USR1 -x helix"),
    AppDef::new("micro", "micro", ICON_EDIT_NOTE, Category::Editor,
        "Terminal editor with familiar keybindings.",
        "micro", "https://github.com/zyedidia/micro/blob/master/runtime/help/options.md",
        &[ConfigSpec::new("settings.json", &["~/.config/micro/settings.json"], Format::Json, MICRO_TEMPLATE, MICRO_QUICK)]),
    AppDef::new("vscode", "VS Code", ICON_CODE, Category::Editor,
        "Code - OSS, VS Code and VSCodium user settings.",
        "code", "https://code.visualstudio.com/docs/getstarted/settings",
        &[
            ConfigSpec::new("Code - OSS", &["~/.config/Code - OSS/User/settings.json"], Format::Json, VSCODE_TEMPLATE, VSCODE_QUICK),
            ConfigSpec::new("VS Code", &["~/.config/Code/User/settings.json"], Format::Json, VSCODE_TEMPLATE, VSCODE_QUICK),
            ConfigSpec::new("VSCodium", &["~/.config/VSCodium/User/settings.json"], Format::Json, VSCODE_TEMPLATE, VSCODE_QUICK),
        ]),
    AppDef::new("zed", "Zed", ICON_CODE, Category::Editor,
        "Fast, collaborative code editor.",
        "zed", "https://zed.dev/docs/configuring-zed",
        &[ConfigSpec::new("settings.json", &["~/.config/zed/settings.json"], Format::Json, ZED_TEMPLATE, ZED_QUICK)])
        .bin("zeditor"),
    AppDef::new("m3-browser", "Material 3 Browser", ICON_LANGUAGE, Category::Tools,
        "Zen with a Material 3 look; themes websites with your colors.",
        "material-3-browser-bin", "https://github.com/bhimio1/material-3-browser",
        &[ConfigSpec::new("Custom browser CSS", &["~/.local/share/m3-browser/profile/chrome/m3-custom.css"],
            Format::Text(Syntax::Css), M3_BROWSER_CSS_TEMPLATE, &[])])
        .bin("m3-browser"),
    // Media
    AppDef::new("mpv", "mpv", ICON_MOVIE, Category::Media,
        "Minimal, powerful video player.",
        "mpv", "https://mpv.io/manual/stable/#options",
        &[
            ConfigSpec::new("mpv.conf", &["~/.config/mpv/mpv.conf"], INI_COMPACT, MPV_TEMPLATE, MPV_QUICK),
            ConfigSpec::new("input.conf", &["~/.config/mpv/input.conf"], HASH_TEXT, "# mpv key bindings — KEY command\n# WHEEL_UP add volume 2\n", &[]),
        ]),
    AppDef::new("zathura", "zathura", ICON_PICTURE_AS_PDF, Category::Media,
        "Keyboard-driven PDF and document viewer.",
        "zathura", "https://man.archlinux.org/man/zathurarc.5",
        &[ConfigSpec::new("zathurarc", &["~/.config/zathura/zathurarc"], Format::Kv(ZATHURA_DIALECT), ZATHURA_TEMPLATE, ZATHURA_QUICK)]),
    AppDef::new("mangohud", "MangoHud", ICON_SPORTS_ESPORTS, Category::Media,
        "In-game overlay for FPS, temperatures and load.",
        "mangohud", "https://github.com/flightlessmango/MangoHud#hud-configuration",
        &[ConfigSpec::new("MangoHud.conf", &["~/.config/MangoHud/MangoHud.conf"], COMPACT, MANGOHUD_TEMPLATE, MANGOHUD_QUICK)]),
    // Tools
    AppDef::new("btop", "btop", ICON_MONITOR_HEART, Category::Tools,
        "Resource monitor. Close btop before editing: it saves on exit.",
        "btop", "https://github.com/aristocratos/btop",
        &[ConfigSpec::new("btop.conf", &["~/.config/btop/btop.conf"], EQUALS, BTOP_TEMPLATE, BTOP_QUICK)]),
    AppDef::new("fastfetch", "fastfetch", ICON_INFO, Category::Tools,
        "System information at a glance (neofetch successor).",
        "fastfetch", "https://github.com/fastfetch-cli/fastfetch/wiki/Configuration",
        &[ConfigSpec::new("config.jsonc", &["~/.config/fastfetch/config.jsonc"], Format::Json, FASTFETCH_TEMPLATE, FASTFETCH_QUICK)]),
    AppDef::new("yazi", "yazi", ICON_FOLDER_OPEN, Category::Tools,
        "Blazing fast terminal file manager.",
        "yazi", "https://yazi-rs.github.io/docs/configuration/yazi",
        &[
            ConfigSpec::new("yazi.toml", &["~/.config/yazi/yazi.toml"], Format::Toml, YAZI_TEMPLATE, YAZI_QUICK),
            ConfigSpec::new("keymap.toml", &["~/.config/yazi/keymap.toml"], Format::Toml, "# yazi key bindings — see https://yazi-rs.github.io/docs/configuration/keymap\n", &[]),
        ]),
    AppDef::new("ranger", "ranger", ICON_FOLDER, Category::Tools,
        "Vim-like terminal file manager.",
        "ranger", "https://github.com/ranger/ranger/wiki",
        &[ConfigSpec::new("rc.conf", &["~/.config/ranger/rc.conf"], Format::Kv(RANGER_DIALECT), RANGER_TEMPLATE, RANGER_QUICK)]),
    AppDef::new("lazygit", "lazygit", ICON_ACCOUNT_TREE, Category::Tools,
        "Terminal UI for git.",
        "lazygit", "https://github.com/jesseduffield/lazygit/blob/master/docs/Config.md",
        &[ConfigSpec::new("config.yml", &["~/.config/lazygit/config.yml"], YAML_TEXT, LAZYGIT_TEMPLATE, &[])]),
    AppDef::new("git", "Git", ICON_ACCOUNT_TREE, Category::Tools,
        "Your name, email and global git defaults.",
        "git", "https://git-scm.com/docs/git-config",
        &[ConfigSpec::new(".gitconfig", &["~/.gitconfig", "~/.config/git/config"], INI, GIT_TEMPLATE, GIT_QUICK)]),
    AppDef::new("ssh", "SSH", ICON_KEY, Category::Tools,
        "Hosts, users and keys for the ssh client.",
        "openssh", "https://man.archlinux.org/man/ssh_config.5",
        &[ConfigSpec::new("config", &["~/.ssh/config"], HASH_TEXT, SSH_TEMPLATE, &[])])
        .bin("ssh"),
    AppDef::new("bat", "bat", ICON_DESCRIPTION, Category::Tools,
        "cat with syntax highlighting and git integration.",
        "bat", "https://github.com/sharkdp/bat#configuration-file",
        &[ConfigSpec::new("config", &["~/.config/bat/config"], Format::Kv(BAT_DIALECT), BAT_TEMPLATE, BAT_QUICK)]),
    // Theming
    AppDef::new("gtk", "GTK settings", ICON_PALETTE, Category::Theming,
        "Theme, icons, cursor and font for GTK 3 and GTK 4 apps.",
        "gtk3", "https://docs.gtk.org/gtk3/class.Settings.html",
        &[
            ConfigSpec::new("GTK 3", &["~/.config/gtk-3.0/settings.ini"], INI_COMPACT, GTK_TEMPLATE, GTK_QUICK),
            ConfigSpec::new("GTK 4", &["~/.config/gtk-4.0/settings.ini"], INI_COMPACT, GTK_TEMPLATE, GTK_QUICK),
        ])
        .bin("gtk3-demo"),
    AppDef::new("qt6ct", "qt6ct", ICON_STYLE, Category::Theming,
        "Style, icons and fonts for Qt 6 apps. Needs QT_QPA_PLATFORMTHEME=qt6ct.",
        "qt6ct", "https://wiki.archlinux.org/title/Uniform_look_for_Qt_and_GTK_applications",
        &[ConfigSpec::new("qt6ct.conf", &["~/.config/qt6ct/qt6ct.conf"], INI_COMPACT, QT6CT_TEMPLATE, QTCT_QUICK)]),
    AppDef::new("qt5ct", "qt5ct", ICON_STYLE, Category::Theming,
        "Style, icons and fonts for Qt 5 apps.",
        "qt5ct", "https://wiki.archlinux.org/title/Uniform_look_for_Qt_and_GTK_applications",
        &[ConfigSpec::new("qt5ct.conf", &["~/.config/qt5ct/qt5ct.conf"], INI_COMPACT, QT6CT_TEMPLATE, QTCT_QUICK)]),
    AppDef::new("kvantum", "Kvantum", ICON_BRUSH, Category::Theming,
        "SVG-based theme engine for Qt.",
        "kvantum", "https://github.com/tsujan/Kvantum",
        &[ConfigSpec::new("kvantum.kvconfig", &["~/.config/Kvantum/kvantum.kvconfig"], INI_COMPACT, KVANTUM_TEMPLATE, KVANTUM_QUICK)])
        .bin("kvantummanager"),
    AppDef::new("fontconfig", "Fontconfig", ICON_FONT_DOWNLOAD, Category::Theming,
        "Default fonts, antialiasing and hinting for every app.",
        "fontconfig", "https://wiki.archlinux.org/title/Font_configuration",
        &[ConfigSpec::new("fonts.conf", &["~/.config/fontconfig/fonts.conf"], PLAIN_TEXT, FONTCONFIG_TEMPLATE, &[])])
        .bin("fc-cache")
        .reload("fc-cache -f"),
    // System
    AppDef::new("paru", "paru", ICON_PACKAGE_2, Category::System,
        "AUR helper. System-wide settings, saved with your password.",
        "paru", "https://man.archlinux.org/man/paru.conf.5",
        &[ConfigSpec::new("paru.conf", &["/etc/paru.conf"], INI, PARU_TEMPLATE, PARU_QUICK).system()])
        .aur(),
    AppDef::new("yay", "yay", ICON_PACKAGE_2, Category::System,
        "AUR helper. Settings normally saved with `yay --save`.",
        "yay", "https://github.com/Jguer/yay",
        &[ConfigSpec::new("config.json", &["~/.config/yay/config.json"], Format::Json, YAY_TEMPLATE, YAY_QUICK)])
        .aur(),
    AppDef::new("makepkg", "makepkg", ICON_BUILD, Category::System,
        "Per-user build options: parallel jobs, compression, build folder.",
        "pacman", "https://wiki.archlinux.org/title/Makepkg",
        &[ConfigSpec::new("makepkg.conf", &["~/.config/pacman/makepkg.conf", "~/.makepkg.conf"], COMPACT, MAKEPKG_TEMPLATE, MAKEPKG_QUICK)])
        .bin("makepkg"),
    AppDef::new("environment", "Environment variables", ICON_DATA_OBJECT, Category::System,
        "Session-wide variables via systemd's environment.d.",
        "systemd", "https://man.archlinux.org/man/environment.d.5",
        &[ConfigSpec::new("envvars.conf", &["~/.config/environment.d/envvars.conf"], COMPACT, ENV_TEMPLATE, ENV_QUICK)])
        .bin("systemctl"),
    AppDef::new("user-dirs", "User folders", ICON_FOLDER_SPECIAL, Category::System,
        "Where Desktop, Downloads, Pictures and the other XDG folders live.",
        "xdg-user-dirs", "https://wiki.archlinux.org/title/XDG_user_directories",
        &[ConfigSpec::new("user-dirs.dirs", &["~/.config/user-dirs.dirs"], COMPACT, USER_DIRS_TEMPLATE, USER_DIRS_QUICK)])
        .bin("xdg-user-dirs-update"),
    AppDef::new("mimeapps", "File associations", ICON_LAUNCH, Category::System,
        "mimeapps.list: which app opens which file type. See also Default apps.",
        "xdg-utils", "https://wiki.archlinux.org/title/XDG_MIME_Applications",
        &[ConfigSpec::new("mimeapps.list", &["~/.config/mimeapps.list"], INI_COMPACT, MIMEAPPS_TEMPLATE, MIMEAPPS_QUICK)])
        .bin("xdg-mime"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats;
    use serde_json::Value;
    use std::collections::HashSet;

    /// A value of the right shape for each control.
    fn sample(quick: &Quick, format: Format) -> Value {
        let structured_scalar = matches!(format, Format::Toml | Format::Json);
        match quick.kind {
            Kind::Bool(_) if structured_scalar => Value::Bool(true),
            Kind::Bool(BoolStyle::YesNo) => Value::String("yes".into()),
            Kind::Bool(BoolStyle::Capitalized) => Value::String("True".into()),
            Kind::Bool(BoolStyle::OneZero) => Value::String("1".into()),
            Kind::Bool(_) => Value::String("true".into()),
            Kind::Int { min, max } if structured_scalar => serde_json::json!((min + max) / 2),
            Kind::Int { min, .. } => Value::String((min.max(1)).to_string()),
            Kind::Float { max, .. } if structured_scalar => serde_json::json!(max / 2.0),
            Kind::Float { max, .. } => Value::String(format!("{}", max / 2.0)),
            Kind::Color => Value::String("#123456".into()),
            Kind::ColorBare => Value::String("123456".into()),
            Kind::Choice(opts) => Value::String(opts[0].into()),
            Kind::Text | Kind::Font => Value::String("Sample Value".into()),
        }
    }

    #[test]
    fn ids_are_unique_and_specs_complete() {
        let mut ids = HashSet::new();
        for app in APPS {
            assert!(ids.insert(app.id), "duplicate id {}", app.id);
            assert!(!app.files.is_empty(), "{} has no files", app.id);
            assert!(app.docs.starts_with("https://"), "{} docs", app.id);
            for spec in app.files {
                assert!(!spec.paths.is_empty(), "{} has no paths", app.id);
                assert!(spec.template.ends_with('\n'), "{}: template needs a trailing newline", app.id);
                if !spec.quick.is_empty() {
                    assert!(spec.format.structured(), "{}: quick settings on a text-only file", app.id);
                }
            }
            assert!(find(app.id).is_some());
        }
        assert!(APPS.len() >= 45, "catalog has {} apps", APPS.len());
    }

    #[test]
    fn templates_parse() {
        for app in APPS {
            for spec in app.files {
                spec.format
                    .validate(spec.template)
                    .unwrap_or_else(|e| panic!("{} {}: {e}", app.id, spec.label));
            }
        }
    }

    #[test]
    fn quick_keys_round_trip() {
        for app in APPS {
            for spec in app.files {
                for quick in spec.quick {
                    let ctx = format!("{} / {} / {}", app.id, spec.label, quick.key);
                    if let (Kind::Bool(BoolStyle::Flag), Format::Kv(d)) = (quick.kind, spec.format) {
                        for on in [true, false, true] {
                            let t = formats::set_flag(spec.template, &d, quick.key, on);
                            assert_eq!(formats::get_flag(&t, &d, quick.key), on, "{ctx}");
                        }
                        continue;
                    }
                    let value = sample(quick, spec.format);
                    let value = match (&value, quick.quoted) {
                        (Value::String(s), true) => Value::String(format!("\"{s}\"")),
                        _ => value,
                    };
                    let text = formats::set(spec.format, spec.template, quick.key, &value)
                        .unwrap_or_else(|e| panic!("{ctx}: {e}"));
                    spec.format.validate(&text).unwrap_or_else(|e| panic!("{ctx}: {e}"));
                    let back = formats::get(spec.format, &text, quick.key)
                        .unwrap_or_else(|| panic!("{ctx}: not readable after set"));
                    let expected = match (&value, spec.format) {
                        (Value::String(s), Format::Kv(_)) => {
                            Value::String(formats::kv::unquote(s).to_string())
                        }
                        _ => value.clone(),
                    };
                    assert_eq!(back, expected, "{ctx}");
                    // Setting again replaces rather than duplicating.
                    let again = formats::set(spec.format, &text, quick.key, &value).unwrap();
                    assert_eq!(again, text, "{ctx}: not idempotent");
                }
            }
        }
    }

    #[test]
    fn templates_have_no_duplicate_quick_keys() {
        for app in APPS {
            for spec in app.files {
                let mut keys = HashSet::new();
                for quick in spec.quick {
                    assert!(keys.insert(quick.key), "{}: {} twice", app.id, quick.key);
                }
            }
        }
    }
}
