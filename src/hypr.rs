//! Hyprland: live option values through `hyprctl` and persisting changes in
//! a managed block of the user's config (Lua or hyprlang).
//!
//! The block is the only part of the file we write:
//!
//! ```lua
//! -- >>> m3-settings >>>
//! hl.config({
//!     general = {
//!         gaps_in = 4,
//!     },
//! })
//! -- <<< m3-settings <<<
//! ```
//!
//! On end-4 installs it goes in `custom/general.lua`, which is loaded after
//! the dots' defaults, so our values win and survive dots updates.

use crate::sys::cmd;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syntax {
    Lua,
    Conf,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub syntax: Syntax,
    pub path: PathBuf,
    /// The end-4 dots' `custom/` override file.
    pub end4: bool,
}

/// Where our managed block lives.
pub fn target(home: &Path) -> Target {
    let hypr = home.join(".config/hypr");
    let lua = hypr.join("hyprland.lua");
    let conf = hypr.join("hyprland.conf");
    let read = |p: &Path| std::fs::read_to_string(p).unwrap_or_default();
    if lua.is_file() {
        let main = read(&lua);
        if main.contains("custom.general") || main.contains("custom/general") {
            return Target {
                syntax: Syntax::Lua,
                path: hypr.join("custom/general.lua"),
                end4: true,
            };
        }
        return Target {
            syntax: Syntax::Lua,
            path: lua,
            end4: false,
        };
    }
    if conf.is_file() && read(&conf).contains("custom/general.conf") {
        return Target {
            syntax: Syntax::Conf,
            path: hypr.join("custom/general.conf"),
            end4: true,
        };
    }
    Target {
        syntax: Syntax::Conf,
        path: conf,
        end4: false,
    }
}

/// Hyprland is running and `hyprctl` can reach it.
pub fn running() -> bool {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() && cmd::exists("hyprctl")
}

// --------------------------------------------------------------------- values

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl Val {
    fn render(&self, syntax: Syntax) -> String {
        match self {
            Val::Bool(b) => b.to_string(),
            Val::Int(i) => i.to_string(),
            Val::Float(f) => {
                let s = format!("{f:.3}");
                let s = s.trim_end_matches('0').trim_end_matches('.');
                if s.is_empty() || s == "-" {
                    "0".into()
                } else {
                    s.to_string()
                }
            }
            Val::Str(s) => match syntax {
                Syntax::Lua => lua_string(s),
                Syntax::Conf => s.clone(),
            },
        }
    }

    fn parse(raw: &str, syntax: Syntax) -> Val {
        let raw = raw.trim().trim_end_matches(',').trim();
        if syntax == Syntax::Lua {
            if let Some(s) = parse_lua_string(raw) {
                return Val::Str(s);
            }
        }
        match raw {
            "true" | "yes" | "on" => return Val::Bool(true),
            "false" | "no" | "off" => return Val::Bool(false),
            _ => {}
        }
        if let Ok(i) = raw.parse::<i64>() {
            return Val::Int(i);
        }
        if let Ok(f) = raw.parse::<f64>() {
            return Val::Float(f);
        }
        Val::Str(raw.to_string())
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Val::Bool(b) => Some(*b),
            Val::Int(i) => Some(*i != 0),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Val::Int(i) => Some(*i as f64),
            Val::Float(f) => Some(*f),
            Val::Bool(b) => Some(*b as i64 as f64),
            Val::Str(s) => s.split_whitespace().next()?.parse().ok(),
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            Val::Str(s) => s.clone(),
            other => other.render(Syntax::Conf),
        }
    }
}

fn lua_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn parse_lua_string(raw: &str) -> Option<String> {
    let q = raw.chars().next()?;
    if (q == '"' || q == '\'') && raw.len() >= 2 && raw.ends_with(q) {
        Some(raw[1..raw.len() - 1].replace("\\\"", "\"").replace("\\\\", "\\"))
    } else {
        None
    }
}

/// Parse `hyprctl getoption <opt> -j`.
pub fn parse_getoption(json: &str) -> Option<Val> {
    let v: Value = serde_json::from_str(json).ok()?;
    if let Some(i) = v.get("int").and_then(Value::as_i64) {
        return Some(Val::Int(i));
    }
    if let Some(f) = v.get("float").and_then(Value::as_f64) {
        return Some(Val::Float(f));
    }
    if let Some(s) = v.get("str").and_then(Value::as_str) {
        return Some(Val::Str(s.trim_matches('"').to_string()));
    }
    if let Some(s) = v.get("custom").and_then(Value::as_str) {
        // Gaps come back as "5 5 5 5"; a single number is enough here.
        let parts: Vec<&str> = s.split_whitespace().collect();
        if !parts.is_empty() && parts.iter().all(|p| *p == parts[0]) {
            if let Ok(i) = parts[0].parse::<i64>() {
                return Some(Val::Int(i));
            }
        }
        return Some(Val::Str(s.to_string()));
    }
    if let Some(arr) = v.get("vec2").and_then(Value::as_array) {
        let s: Vec<String> = arr.iter().map(|x| x.to_string()).collect();
        return Some(Val::Str(s.join(" ")));
    }
    None
}

pub fn getoption(key: &str) -> Option<Val> {
    cmd::run("hyprctl", &["getoption", key, "-j"])
        .ok()
        .and_then(|out| parse_getoption(&out))
}

/// Re-read the config files.
pub fn reload() -> Result<(), String> {
    cmd::run("hyprctl", &["reload"]).map(|_| ())
}

// ------------------------------------------------------------------- settings

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Bool,
    Int { min: i64, max: i64 },
    Float { min: f64, max: f64 },
    Choice(&'static [(&'static str, &'static str)]),
    IntChoice(&'static [(i64, &'static str)]),
    Text,
}

#[derive(Clone, Copy, Debug)]
pub struct Setting {
    /// `section:key` (or `a:b:c`) as hyprctl names it.
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub kind: Kind,
}

impl Setting {
    /// Hyprland's built-in default, used when nothing else is known.
    pub fn default_value(&self) -> Val {
        match self.key {
            "general:gaps_in" => Val::Int(5),
            "general:gaps_out" => Val::Int(20),
            "general:border_size" => Val::Int(1),
            "general:layout" => Val::Str("dwindle".into()),
            "decoration:active_opacity" | "decoration:inactive_opacity" => Val::Float(1.0),
            "decoration:blur:size" => Val::Int(8),
            "decoration:blur:passes" => Val::Int(1),
            "decoration:shadow:range" => Val::Int(4),
            "input:kb_layout" => Val::Str("us".into()),
            "input:follow_mouse" => Val::Int(1),
            "input:sensitivity" => Val::Float(0.0),
            "cursor:no_hardware_cursors" => Val::Int(2),
            "decoration:blur:enabled"
            | "decoration:shadow:enabled"
            | "animations:enabled"
            | "input:touchpad:tap-to-click"
            | "input:touchpad:disable_while_typing"
            | "misc:vfr" => Val::Bool(true),
            _ => match self.kind {
                Kind::Bool => Val::Bool(false),
                Kind::Int { .. } | Kind::IntChoice(_) => Val::Int(0),
                Kind::Float { .. } => Val::Float(0.0),
                Kind::Choice(c) => Val::Str(c.first().map(|(v, _)| v.to_string()).unwrap_or_default()),
                Kind::Text => Val::Str(String::new()),
            },
        }
    }

    /// Coerce a value read from hyprctl or the file to this setting's type.
    pub fn coerce(&self, v: Val) -> Val {
        match self.kind {
            Kind::Bool => v.as_bool().map(Val::Bool).unwrap_or(v),
            Kind::Int { .. } | Kind::IntChoice(_) => match v.as_f64() {
                Some(f) => Val::Int(f.round() as i64),
                None => v,
            },
            Kind::Float { .. } => v.as_f64().map(Val::Float).unwrap_or(v),
            Kind::Choice(_) | Kind::Text => Val::Str(v.as_str()),
        }
    }
}

const fn s(key: &'static str, label: &'static str, help: &'static str, kind: Kind) -> Setting {
    Setting { key, label, help, kind }
}

pub const GROUPS: &[(&str, &[Setting])] = &[
    (
        "Windows & layout",
        &[
            s("general:gaps_in", "Inner gaps", "Space between windows, in pixels.", Kind::Int { min: 0, max: 40 }),
            s("general:gaps_out", "Outer gaps", "Space between windows and the screen edges.", Kind::Int { min: 0, max: 60 }),
            s("general:border_size", "Border width", "", Kind::Int { min: 0, max: 10 }),
            s(
                "general:layout",
                "Layout",
                "How tiled windows are arranged.",
                Kind::Choice(&[("dwindle", "Dwindle"), ("master", "Master")]),
            ),
            s("general:resize_on_border", "Resize by dragging borders", "", Kind::Bool),
        ],
    ),
    (
        "Decoration",
        &[
            s("decoration:rounding", "Corner radius", "", Kind::Int { min: 0, max: 30 }),
            s("decoration:active_opacity", "Focused window opacity", "", Kind::Float { min: 0.1, max: 1.0 }),
            s("decoration:inactive_opacity", "Unfocused window opacity", "", Kind::Float { min: 0.1, max: 1.0 }),
            s("decoration:dim_inactive", "Dim unfocused windows", "", Kind::Bool),
            s("decoration:blur:enabled", "Blur", "Blur behind translucent windows and layers.", Kind::Bool),
            s("decoration:blur:size", "Blur size", "", Kind::Int { min: 1, max: 20 }),
            s("decoration:blur:passes", "Blur passes", "More passes look smoother but cost GPU time.", Kind::Int { min: 1, max: 6 }),
            s("decoration:shadow:enabled", "Shadows", "", Kind::Bool),
            s("decoration:shadow:range", "Shadow size", "", Kind::Int { min: 0, max: 50 }),
            s("animations:enabled", "Animations", "", Kind::Bool),
        ],
    ),
    (
        "Keyboard & mouse",
        &[
            s("input:kb_layout", "Keyboard layout", "XKB layouts, comma separated (us,de).", Kind::Text),
            s("input:kb_variant", "Layout variant", "e.g. dvorak, intl. Comma separated per layout.", Kind::Text),
            s("input:kb_options", "XKB options", "e.g. grp:alt_shift_toggle,caps:escape", Kind::Text),
            s(
                "input:follow_mouse",
                "Focus follows mouse",
                "",
                Kind::IntChoice(&[(0, "Off"), (1, "Always"), (2, "Detached"), (3, "On click")]),
            ),
            s("input:sensitivity", "Pointer speed", "-1 (slow) to 1 (fast).", Kind::Float { min: -1.0, max: 1.0 }),
            s(
                "input:accel_profile",
                "Acceleration",
                "",
                Kind::Choice(&[("", "Default"), ("adaptive", "Adaptive"), ("flat", "Flat")]),
            ),
            s("input:touchpad:natural_scroll", "Natural scrolling (touchpad)", "", Kind::Bool),
            s("input:touchpad:tap-to-click", "Tap to click", "", Kind::Bool),
            s("input:touchpad:disable_while_typing", "Disable touchpad while typing", "", Kind::Bool),
        ],
    ),
    (
        "Miscellaneous",
        &[
            s("misc:disable_hyprland_logo", "Hide the Hyprland wallpaper logo", "", Kind::Bool),
            s("misc:vfr", "Variable frame rate", "Saves power by not rendering unchanged frames.", Kind::Bool),
            s(
                "cursor:no_hardware_cursors",
                "Software cursor",
                "Fixes invisible or glitchy cursors on some GPUs.",
                Kind::IntChoice(&[(0, "Off"), (1, "On"), (2, "Auto")]),
            ),
        ],
    ),
];

pub fn all_settings() -> impl Iterator<Item = &'static Setting> {
    GROUPS.iter().flat_map(|(_, s)| s.iter())
}

#[cfg(test)]
pub fn find_setting(key: &str) -> Option<&'static Setting> {
    all_settings().find(|s| s.key == key)
}

/// Live values for every setting (Hyprland must be running).
pub fn live_values() -> BTreeMap<String, Val> {
    all_settings()
        .filter_map(|s| getoption(s.key).map(|v| (s.key.to_string(), s.coerce(v))))
        .collect()
}

// ------------------------------------------------------------------ the block

const BEGIN: &str = ">>> m3-settings >>>";
const END: &str = "<<< m3-settings <<<";

/// A monitor rule we persist.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MonitorRule {
    pub output: String,
    pub mode: String,
    pub position: String,
    pub scale: String,
    pub transform: i64,
    pub disabled: bool,
}

impl MonitorRule {
    /// The hyprlang / `hyprctl keyword monitor` form.
    pub fn conf_value(&self) -> String {
        if self.disabled {
            return format!("{},disable", self.output);
        }
        let mut s = format!("{},{},{},{}", self.output, self.mode, self.position, self.scale);
        if self.transform != 0 {
            s.push_str(&format!(",transform,{}", self.transform));
        }
        s
    }

    fn parse_conf(v: &str) -> Option<MonitorRule> {
        let parts: Vec<&str> = v.split(',').map(str::trim).collect();
        let output = parts.first()?.to_string();
        if parts.get(1) == Some(&"disable") {
            return Some(MonitorRule {
                output,
                disabled: true,
                ..Default::default()
            });
        }
        let mut rule = MonitorRule {
            output,
            mode: parts.get(1).unwrap_or(&"preferred").to_string(),
            position: parts.get(2).unwrap_or(&"auto").to_string(),
            scale: parts.get(3).unwrap_or(&"1").to_string(),
            ..Default::default()
        };
        if let Some(i) = parts.iter().position(|p| *p == "transform") {
            rule.transform = parts.get(i + 1).and_then(|t| t.parse().ok()).unwrap_or(0);
        }
        Some(rule)
    }

    fn lua(&self) -> String {
        if self.disabled {
            return format!("hl.monitor({{ output = {}, disabled = true }})", lua_string(&self.output));
        }
        let scale = Val::parse(&self.scale, Syntax::Conf).render(Syntax::Lua);
        let mut s = format!(
            "hl.monitor({{ output = {}, mode = {}, position = {}, scale = {}",
            lua_string(&self.output),
            lua_string(&self.mode),
            lua_string(&self.position),
            scale
        );
        if self.transform != 0 {
            s.push_str(&format!(", transform = {}", self.transform));
        }
        s.push_str(" })");
        s
    }

    fn parse_lua(line: &str) -> Option<MonitorRule> {
        let inner = line.trim().strip_prefix("hl.monitor(")?;
        let inner = inner.trim_end().strip_suffix(')')?.trim();
        let inner = inner.strip_prefix('{')?.strip_suffix('}')?;
        let mut rule = MonitorRule::default();
        for field in split_top_level(inner) {
            let Some((k, v)) = field.split_once('=') else { continue };
            let v = Val::parse(v, Syntax::Lua);
            match k.trim() {
                "output" => rule.output = v.as_str(),
                "mode" => rule.mode = v.as_str(),
                "position" => rule.position = v.as_str(),
                "scale" => rule.scale = v.as_str(),
                "transform" => rule.transform = v.as_f64().unwrap_or(0.0) as i64,
                "disabled" => rule.disabled = v.as_bool().unwrap_or(false),
                _ => {}
            }
        }
        (!rule.output.is_empty()).then_some(rule)
    }
}

/// Split `a = "x,y", b = 2` on commas outside quotes.
fn split_top_level(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote = None;
    for c in s.chars() {
        match (quote, c) {
            (None, '"' | '\'') => {
                quote = Some(c);
                cur.push(c);
            }
            (Some(q), _) if c == q => {
                quote = None;
                cur.push(c);
            }
            (None, ',') => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Block {
    /// `a:b:c` → value.
    pub settings: BTreeMap<String, Val>,
    pub monitors: Vec<MonitorRule>,
}

impl Block {
    pub fn set_monitor(&mut self, rule: MonitorRule) {
        match self.monitors.iter_mut().find(|m| m.output == rule.output) {
            Some(m) => *m = rule,
            None => self.monitors.push(rule),
        }
    }
}

fn comment_prefix(syntax: Syntax) -> &'static str {
    match syntax {
        Syntax::Lua => "--",
        Syntax::Conf => "#",
    }
}

/// Byte range of the block's inner lines (between the markers), and the
/// full range including markers.
fn find_block(text: &str, syntax: Syntax) -> Option<(usize, usize, usize, usize)> {
    let c = comment_prefix(syntax);
    let begin = format!("{c} {BEGIN}");
    let end = format!("{c} {END}");
    let start = text.find(&begin)?;
    let inner_start = text[start..].find('\n').map(|i| start + i + 1)?;
    let end_pos = text[inner_start..].find(&end).map(|i| inner_start + i)?;
    let full_end = text[end_pos..]
        .find('\n')
        .map(|i| end_pos + i + 1)
        .unwrap_or(text.len());
    Some((start, inner_start, end_pos, full_end))
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !s.chars().next().is_some_and(|c| c.is_ascii_digit())
}

pub fn parse_block(text: &str, syntax: Syntax) -> Block {
    let mut block = Block::default();
    let Some((_, inner_start, inner_end, _)) = find_block(text, syntax) else {
        return block;
    };
    let inner = &text[inner_start..inner_end];
    match syntax {
        Syntax::Conf => {
            for line in inner.lines() {
                let t = line.trim();
                if t.starts_with('#') || t.is_empty() {
                    continue;
                }
                let Some((k, v)) = t.split_once('=') else { continue };
                let k = k.trim();
                if k == "monitor" {
                    if let Some(m) = MonitorRule::parse_conf(v) {
                        block.set_monitor(m);
                    }
                } else {
                    block.settings.insert(k.to_string(), Val::parse(v, syntax));
                }
            }
        }
        Syntax::Lua => {
            let mut stack: Vec<String> = Vec::new();
            let mut in_config = false;
            for line in inner.lines() {
                let t = line.trim();
                if t.starts_with("--") || t.is_empty() {
                    continue;
                }
                if t.starts_with("hl.monitor(") {
                    if let Some(m) = MonitorRule::parse_lua(t) {
                        block.set_monitor(m);
                    }
                    continue;
                }
                if t.starts_with("hl.config(") {
                    in_config = true;
                    continue;
                }
                if !in_config {
                    continue;
                }
                if t.starts_with('}') {
                    if stack.pop().is_none() {
                        in_config = false;
                    }
                    continue;
                }
                let Some((k, v)) = t.split_once('=') else { continue };
                let k = k.trim();
                let k = k
                    .strip_prefix("[\"")
                    .and_then(|k| k.strip_suffix("\"]"))
                    .unwrap_or(k)
                    .to_string();
                let v = v.trim();
                if v.starts_with('{') {
                    stack.push(k);
                } else {
                    let mut path = stack.clone();
                    path.push(k);
                    block.settings.insert(path.join(":"), Val::parse(v, syntax));
                }
            }
        }
    }
    block
}

fn lua_key(k: &str) -> String {
    if is_ident(k) {
        k.to_string()
    } else {
        format!("[{}]", lua_string(k))
    }
}

/// Nested map for rendering Lua tables.
#[derive(Default)]
struct Tree {
    leaves: BTreeMap<String, Val>,
    children: BTreeMap<String, Tree>,
}

impl Tree {
    fn insert(&mut self, path: &[&str], v: Val) {
        match path {
            [last] => {
                self.leaves.insert(last.to_string(), v);
            }
            [first, rest @ ..] => self.children.entry(first.to_string()).or_default().insert(rest, v),
            [] => {}
        }
    }

    fn render(&self, out: &mut String, depth: usize) {
        let pad = "    ".repeat(depth);
        for (k, v) in &self.leaves {
            out.push_str(&format!("{pad}{} = {},\n", lua_key(k), v.render(Syntax::Lua)));
        }
        for (k, child) in &self.children {
            out.push_str(&format!("{pad}{} = {{\n", lua_key(k)));
            child.render(out, depth + 1);
            out.push_str(&format!("{pad}}},\n"));
        }
    }
}

pub fn render_block(block: &Block, syntax: Syntax) -> String {
    let c = comment_prefix(syntax);
    let mut out = format!(
        "{c} {BEGIN}\n{c} Written by Material 3 Settings. Changes inside this block are overwritten;\n{c} put your own settings outside it.\n"
    );
    match syntax {
        Syntax::Conf => {
            for (k, v) in &block.settings {
                out.push_str(&format!("{k} = {}\n", v.render(syntax)));
            }
            for m in &block.monitors {
                out.push_str(&format!("monitor = {}\n", m.conf_value()));
            }
        }
        Syntax::Lua => {
            if !block.settings.is_empty() {
                let mut tree = Tree::default();
                for (k, v) in &block.settings {
                    let parts: Vec<&str> = k.split(':').collect();
                    tree.insert(&parts, v.clone());
                }
                out.push_str("hl.config({\n");
                tree.render(&mut out, 1);
                out.push_str("})\n");
            }
            for m in &block.monitors {
                out.push_str(&m.lua());
                out.push('\n');
            }
        }
    }
    out.push_str(&format!("{c} {END}\n"));
    out
}

/// Put `block` into `text`, replacing an existing block or appending at the
/// end (so it overrides earlier settings). An empty block is removed.
pub fn replace_block(text: &str, syntax: Syntax, block: &Block) -> String {
    let empty = block.settings.is_empty() && block.monitors.is_empty();
    let rendered = if empty { String::new() } else { render_block(block, syntax) };
    match find_block(text, syntax) {
        Some((start, _, _, full_end)) => format!("{}{}{}", &text[..start], rendered, &text[full_end..]),
        None if empty => text.to_string(),
        None => {
            let mut out = text.to_string();
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&rendered);
            out
        }
    }
}

/// Read the block from the target file.
pub fn load_block(target: &Target) -> Block {
    std::fs::read_to_string(&target.path)
        .map(|t| parse_block(&t, target.syntax))
        .unwrap_or_default()
}

/// Write the block into the target file (creating it if needed).
pub fn save_block(target: &Target, block: &Block, backups: bool) -> Result<(), String> {
    let syntax = match target.syntax {
        Syntax::Lua => crate::formats::Syntax::Lua,
        Syntax::Conf => crate::formats::Syntax::Hash,
    };
    let mut file = crate::files::ConfigFile::open(target.path.clone(), crate::formats::Format::Text(syntax));
    if let Some(e) = &file.load_error {
        return Err(e.clone());
    }
    if !file.exists() && target.end4 && target.syntax == Syntax::Lua {
        file.text = "-- This file will not be overwritten across dots-hyprland updates.\n".into();
    }
    file.text = replace_block(&file.text, target.syntax, block);
    file.save(backups)
}

// ------------------------------------------------------------------- keybinds

#[derive(Clone, Debug, PartialEq)]
pub struct Bind {
    pub keys: String,
    pub action: String,
    pub description: String,
}

pub fn mods_label(mask: u64) -> Vec<&'static str> {
    let mut out = Vec::new();
    for (bit, name) in [(64, "Super"), (4, "Ctrl"), (8, "Alt"), (1, "Shift"), (2, "Caps"), (16, "Mod2"), (32, "Mod3"), (128, "Mod5")] {
        if mask & bit != 0 {
            out.push(name);
        }
    }
    out
}

pub fn parse_binds(json: &str) -> Vec<Bind> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|b| {
            let mask = b.get("modmask").and_then(Value::as_u64).unwrap_or(0);
            let mut key = b.get("key").and_then(Value::as_str).unwrap_or("").to_string();
            if key.is_empty() {
                key = b
                    .get("keycode")
                    .and_then(Value::as_i64)
                    .filter(|c| *c != 0)
                    .map(|c| format!("code:{c}"))
                    .unwrap_or_default();
            }
            if key.is_empty() && mask == 0 {
                return None;
            }
            let mut parts: Vec<String> = mods_label(mask).into_iter().map(String::from).collect();
            if !key.is_empty() {
                parts.push(key);
            }
            Some(make_bind(b, parts.join(" + ")))
        })
        .collect()
}

fn make_bind(b: &Value, keys: String) -> Bind {
    let dispatcher = b.get("dispatcher").and_then(Value::as_str).unwrap_or("");
    let arg = b.get("arg").and_then(Value::as_str).unwrap_or("");
    let action = if arg.is_empty() {
        dispatcher.to_string()
    } else {
        format!("{dispatcher} {arg}")
    };
    Bind {
        keys,
        action,
        description: b.get("description").and_then(Value::as_str).unwrap_or("").to_string(),
    }
}

pub fn binds() -> Result<Vec<Bind>, String> {
    cmd::run("hyprctl", &["binds", "-j"]).map(|s| parse_binds(&s))
}

// ------------------------------------------------------------------- monitors

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Monitor {
    pub name: String,
    pub description: String,
    pub width: i64,
    pub height: i64,
    pub refresh: f64,
    pub x: i64,
    pub y: i64,
    pub scale: f64,
    pub transform: i64,
    pub disabled: bool,
    pub vrr: bool,
    pub modes: Vec<String>,
}

impl Monitor {
    /// `2560x1440@143.97` (the form monitor rules take).
    pub fn mode(&self) -> String {
        format!("{}x{}@{:.2}", self.width, self.height, self.refresh)
    }
}

/// `1920x1080@60.00Hz` → `1920x1080@60.00`.
pub fn mode_rule(mode: &str) -> String {
    mode.trim().trim_end_matches("Hz").to_string()
}

pub fn parse_monitors(json: &str) -> Vec<Monitor> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let i = |v: &Value, k: &str| v.get(k).and_then(Value::as_i64).unwrap_or(0);
    items
        .iter()
        .map(|m| Monitor {
            name: m.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
            description: m.get("description").and_then(Value::as_str).unwrap_or("").to_string(),
            width: i(m, "width"),
            height: i(m, "height"),
            refresh: m.get("refreshRate").and_then(Value::as_f64).unwrap_or(60.0),
            x: i(m, "x"),
            y: i(m, "y"),
            scale: m.get("scale").and_then(Value::as_f64).unwrap_or(1.0),
            transform: i(m, "transform"),
            disabled: m.get("disabled").and_then(Value::as_bool).unwrap_or(false),
            vrr: m.get("vrr").and_then(Value::as_bool).unwrap_or(false),
            modes: m
                .get("availableModes")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|x| x.as_str().map(mode_rule)).collect())
                .unwrap_or_default(),
        })
        .collect()
}

pub fn monitors() -> Result<Vec<Monitor>, String> {
    cmd::run("hyprctl", &["monitors", "all", "-j"]).map(|s| parse_monitors(&s))
}

/// Apply a monitor rule for the running session.
pub fn apply_monitor(rule: &MonitorRule) -> Result<(), String> {
    let out = cmd::run("hyprctl", &["keyword", "monitor", &rule.conf_value()])?;
    let out = out.trim();
    if out.is_empty() || out == "ok" {
        Ok(())
    } else {
        Err(out.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_block() -> Block {
        let mut b = Block::default();
        b.settings.insert("general:gaps_in".into(), Val::Int(4));
        b.settings.insert("general:layout".into(), Val::Str("master".into()));
        b.settings.insert("decoration:blur:enabled".into(), Val::Bool(false));
        b.settings.insert("decoration:active_opacity".into(), Val::Float(0.95));
        b.settings.insert("input:touchpad:tap-to-click".into(), Val::Bool(true));
        b.monitors.push(MonitorRule {
            output: "DP-1".into(),
            mode: "2560x1440@143.97".into(),
            position: "0x0".into(),
            scale: "1.25".into(),
            transform: 1,
            disabled: false,
        });
        b
    }

    #[test]
    fn lua_round_trip() {
        let b = sample_block();
        let text = replace_block("-- my stuff\nhl.config({ general = { gaps_out = 8 } })\n", Syntax::Lua, &b);
        assert!(text.starts_with("-- my stuff\n"));
        assert!(text.contains("[\"tap-to-click\"] = true,"));
        assert!(text.contains("blur = {"));
        assert!(text.contains("hl.monitor({ output = \"DP-1\", mode = \"2560x1440@143.97\", position = \"0x0\", scale = 1.25, transform = 1 })"));
        assert_eq!(parse_block(&text, Syntax::Lua), b);
        // Replacing again doesn't duplicate.
        let mut b2 = b.clone();
        b2.settings.insert("general:gaps_in".into(), Val::Int(10));
        let text2 = replace_block(&text, Syntax::Lua, &b2);
        assert_eq!(text2.matches(BEGIN).count(), 1);
        assert_eq!(parse_block(&text2, Syntax::Lua), b2);
        // Removing everything removes the block.
        let cleared = replace_block(&text2, Syntax::Lua, &Block::default());
        assert!(!cleared.contains(BEGIN));
        assert!(cleared.starts_with("-- my stuff\n"));
    }

    #[test]
    fn conf_round_trip() {
        let b = sample_block();
        let text = replace_block("source = ~/x.conf", Syntax::Conf, &b);
        assert!(text.starts_with("source = ~/x.conf\n\n# >>> m3-settings >>>\n"));
        assert!(text.contains("general:gaps_in = 4\n"));
        assert!(text.contains("monitor = DP-1,2560x1440@143.97,0x0,1.25,transform,1\n"));
        assert_eq!(parse_block(&text, Syntax::Conf), b);
    }

    #[test]
    fn disabled_monitor() {
        let mut b = Block::default();
        b.set_monitor(MonitorRule {
            output: "HDMI-A-1".into(),
            disabled: true,
            ..Default::default()
        });
        for syntax in [Syntax::Lua, Syntax::Conf] {
            let t = replace_block("", syntax, &b);
            assert_eq!(parse_block(&t, syntax), b, "{t}");
        }
    }

    #[test]
    fn getoption_json() {
        assert_eq!(parse_getoption(r#"{"option":"general:border_size","int":2,"set":true}"#), Some(Val::Int(2)));
        assert_eq!(parse_getoption(r#"{"option":"x","float":0.5,"set":false}"#), Some(Val::Float(0.5)));
        assert_eq!(parse_getoption(r#"{"option":"general:layout","str":"dwindle","set":true}"#), Some(Val::Str("dwindle".into())));
        assert_eq!(parse_getoption(r#"{"option":"general:gaps_in","custom":"5 5 5 5","set":true}"#), Some(Val::Int(5)));
        assert_eq!(parse_getoption(r#"{"option":"general:gaps_out","custom":"10 20 10 20"}"#), Some(Val::Str("10 20 10 20".into())));
        assert_eq!(parse_getoption("no such option"), None);
        let s = find_setting("input:touchpad:natural_scroll").unwrap();
        assert_eq!(s.coerce(Val::Int(1)), Val::Bool(true));
        let s = find_setting("decoration:active_opacity").unwrap();
        assert_eq!(s.coerce(Val::Int(1)), Val::Float(1.0));
    }

    #[test]
    fn target_detection() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        assert_eq!(target(h).syntax, Syntax::Conf);
        std::fs::create_dir_all(h.join(".config/hypr")).unwrap();
        std::fs::write(h.join(".config/hypr/hyprland.lua"), "require(\"custom.general\")\n").unwrap();
        let t = target(h);
        assert!(t.end4 && t.syntax == Syntax::Lua);
        assert!(t.path.ends_with("custom/general.lua"));
        save_block(&t, &sample_block(), false).unwrap();
        assert_eq!(load_block(&t), sample_block());
        let text = std::fs::read_to_string(&t.path).unwrap();
        assert!(text.starts_with("-- This file will not be overwritten"));
    }

    #[test]
    fn binds_and_monitors() {
        let binds = parse_binds(
            r#"[{"modmask":65,"key":"Q","keycode":0,"dispatcher":"killactive","arg":"","description":"Close"},
                {"modmask":64,"key":"Return","keycode":0,"dispatcher":"exec","arg":"kitty","description":""}]"#,
        );
        assert_eq!(binds[0].keys, "Super + Shift + Q");
        assert_eq!(binds[0].description, "Close");
        assert_eq!(binds[1].action, "exec kitty");

        let mons = parse_monitors(
            r#"[{"name":"eDP-1","description":"BOE 0x095F","width":2256,"height":1504,"refreshRate":59.999,
                 "x":0,"y":0,"scale":1.5,"transform":0,"disabled":false,"vrr":false,
                 "availableModes":["2256x1504@60.00Hz","1920x1200@60.00Hz"]}]"#,
        );
        assert_eq!(mons[0].mode(), "2256x1504@60.00");
        assert_eq!(mons[0].modes[1], "1920x1200@60.00");
        assert_eq!(mons[0].scale, 1.5);
    }
}
