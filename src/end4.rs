//! illogical-impulse (end-4) and its fork end4-pC: detection, the shared
//! `~/.config/illogical-impulse/config.json`, and its option schema.
//!
//! Both shells are Quickshell configs living in `~/.config/quickshell/<name>`
//! and read the same config file. Which one Hyprland starts is decided by the
//! `qsConfig` variable (`hl.env("qsConfig", "ii")` in the Lua dots,
//! `$qsConfig = ii` in older hyprlang installs); the user's override goes in
//! `~/.config/hypr/custom/variables.lua`, which is loaded after the defaults.

use material_3_ui::sys::cmd;
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const UPSTREAM: &str = "ii";
pub const FORK: &str = "end4-pC";
/// bhimio1's end4-pC, a fork of pctrade's that uses Material 3 Settings,
/// the Material 3 File Manager and Material 3 Music.
pub const FORK_REPO: &str = "https://github.com/bhimio1/end4-pC.git";
pub const FORK_URL: &str = "https://github.com/bhimio1/end4-pC";
pub const UPSTREAM_URL: &str = "https://github.com/end-4/dots-hyprland";

pub fn quickshell_dir(home: &Path) -> PathBuf {
    home.join(".config/quickshell")
}

pub fn shell_dir(home: &Path, name: &str) -> PathBuf {
    quickshell_dir(home).join(name)
}

pub fn config_path(home: &Path) -> PathBuf {
    home.join(".config/illogical-impulse/config.json")
}

// ------------------------------------------------------------------ detection

#[derive(Clone, Debug, Default)]
pub struct Install {
    pub installed: bool,
    /// `abc1234 · 3 days ago` when the folder is a git checkout.
    pub commit: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub ii: Install,
    pub fork: Install,
    /// The `qsConfig` Hyprland will start, and the file it was read from.
    pub active: Option<(String, PathBuf)>,
    /// Quickshell configs currently running (`qs -c <name>`).
    pub running: Vec<String>,
}

impl Status {
    pub fn active_name(&self) -> Option<&str> {
        self.active.as_ref().map(|(n, _)| n.as_str())
    }

    /// The shell to restart or send IPC to: running one, else the active one.
    pub fn current_shell(&self) -> String {
        self.running
            .first()
            .cloned()
            .or_else(|| self.active_name().map(str::to_string))
            .unwrap_or_else(|| UPSTREAM.to_string())
    }
}

fn install(home: &Path, name: &str) -> Install {
    let dir = shell_dir(home, name);
    let installed = dir.join("shell.qml").is_file();
    let commit = if installed && dir.join(".git").exists() {
        let d = dir.to_string_lossy().into_owned();
        cmd::run("git", &["-C", &d, "log", "-1", "--format=%h · %cr"])
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    };
    Install { installed, commit }
}

/// Files that may set `qsConfig`, most specific first.
pub fn variable_files(home: &Path) -> Vec<PathBuf> {
    let hypr = home.join(".config/hypr");
    vec![
        hypr.join("custom/variables.lua"),
        hypr.join("hyprland/variables.lua"),
        hypr.join("custom/variables.conf"),
        hypr.join("hyprland/variables.conf"),
        hypr.join("hyprland.conf"),
    ]
}

pub fn is_lua(home: &Path) -> bool {
    home.join(".config/hypr/hyprland.lua").is_file()
}

/// The (last, uncommented) `qsConfig` value set in a Lua or hyprlang file.
pub fn parse_qs_config(text: &str) -> Option<String> {
    let mut found = None;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("--") || t.starts_with('#') {
            continue;
        }
        // Lua: hl.env("qsConfig", "ii")
        if let Some(rest) = t.strip_prefix("hl.env(") {
            let args: Vec<&str> = rest.split(',').collect();
            if args.len() >= 2 && unquote_lua(args[0]) == Some("qsConfig") {
                if let Some(v) = unquote_lua(args[1].trim().trim_end_matches(')').trim()) {
                    found = Some(v.to_string());
                }
            }
            continue;
        }
        // hyprlang: $qsConfig = ii   or   env = qsConfig,ii
        if let Some(rest) = t.strip_prefix("$qsConfig") {
            if let Some(v) = rest.trim_start().strip_prefix('=') {
                found = Some(strip_hash_comment(v).to_string());
            }
        } else if let Some(rest) = t.strip_prefix("env") {
            if let Some(v) = rest.trim_start().strip_prefix('=') {
                if let Some((k, v)) = v.split_once(',') {
                    if k.trim() == "qsConfig" {
                        found = Some(strip_hash_comment(v).to_string());
                    }
                }
            }
        }
    }
    found.filter(|s| !s.is_empty())
}

fn strip_hash_comment(v: &str) -> &str {
    v.split(" #").next().unwrap_or(v).trim()
}

fn unquote_lua(s: &str) -> Option<&str> {
    let s = s.trim();
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return Some(&s[1..s.len() - 1]);
        }
    }
    None
}

/// Config names from `pgrep -a` output (`1234 qs -c end4-pC`).
pub fn parse_running(pgrep: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in pgrep.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some(prog) = words.get(1) else { continue };
        let base = prog.rsplit('/').next().unwrap_or(prog);
        if base != "qs" && base != "quickshell" {
            continue;
        }
        // IPC calls (`qs -c ii ipc call …`) are short-lived clients.
        if words.contains(&"ipc") {
            continue;
        }
        let mut name = None;
        for (i, w) in words.iter().enumerate() {
            if *w == "-c" || *w == "--config" {
                name = words.get(i + 1).map(|s| s.to_string());
            } else if let Some(v) = w.strip_prefix("--config=") {
                name = Some(v.to_string());
            } else if *w == "-p" || *w == "--path" {
                name = words.get(i + 1).and_then(|p| {
                    Path::new(p.trim_end_matches("/shell.qml"))
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                });
            }
        }
        let name = name.unwrap_or_else(|| "default".into());
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

pub fn detect(home: &Path) -> Status {
    let active = variable_files(home).into_iter().find_map(|f| {
        let text = std::fs::read_to_string(&f).ok()?;
        parse_qs_config(&text).map(|n| (n, f))
    });
    let mut pgrep = String::new();
    for name in ["qs", "quickshell"] {
        if let Ok(out) = cmd::run("pgrep", &["-a", "-x", name]) {
            pgrep.push_str(&out);
        }
    }
    Status {
        ii: install(home, UPSTREAM),
        fork: install(home, FORK),
        active,
        running: parse_running(&pgrep),
    }
}

// --------------------------------------------------------------- qsConfig edit

const LUA_HEADER: &str = "-- This file will not be overwritten across dots-hyprland updates.\n";

/// Set `qsConfig` in a custom variables file, replacing existing
/// assignments or appending one.
pub fn rewrite_qs_config(text: &str, name: &str, lua: bool) -> String {
    let new_line = if lua {
        format!("hl.env(\"qsConfig\", \"{name}\")")
    } else {
        format!("$qsConfig = {name}")
    };
    let mut replaced = false;
    let mut lines: Vec<String> = text
        .lines()
        .map(|l| {
            let t = l.trim();
            let is_comment = t.starts_with("--") || t.starts_with('#');
            if !is_comment && parse_qs_config(t).is_some() {
                replaced = true;
                let indent = &l[..l.len() - l.trim_start().len()];
                format!("{indent}{new_line}")
            } else {
                l.to_string()
            }
        })
        .collect();
    if !replaced {
        if text.is_empty() && lua {
            lines.push(LUA_HEADER.trim_end().to_string());
        }
        if lines.last().is_some_and(|l| !l.trim().is_empty()) {
            lines.push(String::new());
        }
        if lua {
            lines.push("-- The folder within ~/.config/quickshell containing the shell".into());
        } else {
            lines.push("# The folder within ~/.config/quickshell containing the shell".into());
        }
        lines.push(new_line);
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// The file our override goes to.
pub fn custom_variables_path(home: &Path, lua: bool) -> PathBuf {
    home.join(if lua {
        ".config/hypr/custom/variables.lua"
    } else {
        ".config/hypr/custom/variables.conf"
    })
}

/// Make Hyprland start `name` from now on.
pub fn set_default_shell(home: &Path, name: &str, backups: bool) -> Result<PathBuf, String> {
    let lua = is_lua(home);
    let path = custom_variables_path(home, lua);
    let mut file = crate::files::ConfigFile::open(
        path.clone(),
        crate::formats::Format::Text(if lua {
            crate::formats::Syntax::Lua
        } else {
            crate::formats::Syntax::Hash
        }),
    );
    if let Some(e) = &file.load_error {
        return Err(e.clone());
    }
    file.text = rewrite_qs_config(&file.text, name, lua);
    file.save(backups)?;
    Ok(path)
}

/// Stop every Quickshell instance and start `name`.
pub fn restart_shell(name: &str) -> Result<(), String> {
    if !cmd::exists("qs") {
        return Err("`qs` (quickshell) is not installed".into());
    }
    let _ = cmd::run("pkill", &["-x", "qs"]);
    let _ = cmd::run("pkill", &["-x", "quickshell"]);
    std::thread::sleep(std::time::Duration::from_millis(400));
    cmd::spawn_detached("qs", &["-c", name])
}

/// Open the shell's own settings: end4-pC has an overlay toggled through a
/// Hyprland global; upstream ii ships a separate `settings.qml` window.
pub fn open_shell_settings(home: &Path, name: &str) -> Result<(), String> {
    let settings_qml = shell_dir(home, name).join("settings.qml");
    if settings_qml.is_file() {
        cmd::spawn_detached("qs", &["-p", &settings_qml.to_string_lossy()])
    } else {
        cmd::run("hyprctl", &["dispatch", "global", "quickshell:settingsToggle"]).map(|_| ())
    }
}

pub fn clone_fork(home: &Path) -> Result<String, String> {
    let dir = shell_dir(home, FORK);
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }
    std::fs::create_dir_all(quickshell_dir(home)).map_err(|e| e.to_string())?;
    cmd::run(
        "git",
        &["clone", "--depth", "1", FORK_REPO, &dir.to_string_lossy()],
    )
    .map(|_| "end4-pC installed".to_string())
}

pub fn update_shell(home: &Path, name: &str) -> Result<String, String> {
    let dir = shell_dir(home, name);
    let out = cmd::run("git", &["-C", &dir.to_string_lossy(), "pull", "--ff-only"])?;
    let out = out.trim();
    Ok(if out.contains("Already up to date") {
        format!("{name} is already up to date")
    } else {
        format!("{name} updated")
    })
}

// ------------------------------------------------------------------ wallpaper

/// Image types the shell's wallpaper script and our thumbnails handle.
const WALLPAPER_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp"];

/// Installed shells, the one Hyprland starts first, then end4-pC, then ii.
fn shells_by_preference(home: &Path) -> Vec<String> {
    let active = variable_files(home).into_iter().find_map(|f| parse_qs_config(&std::fs::read_to_string(f).ok()?));
    let mut names: Vec<String> = active.into_iter().collect();
    for n in [FORK, UPSTREAM] {
        if !names.iter().any(|x| x == n) {
            names.push(n.to_string());
        }
    }
    names
        .into_iter()
        .filter(|n| shell_dir(home, n).join("shell.qml").is_file())
        .collect()
}

/// A file inside the preferred shell that has it.
pub fn shell_file(home: &Path, rel: &str) -> Option<PathBuf> {
    shells_by_preference(home)
        .into_iter()
        .map(|n| shell_dir(home, &n).join(rel))
        .find(|p| p.is_file())
}

/// The shell's `switchwall.sh`, which sets the wallpaper, light/dark mode
/// and accent color, and regenerates every color file from them.
pub fn switchwall(home: &Path) -> Option<PathBuf> {
    shell_file(home, "scripts/colors/switchwall.sh")
}

/// Arguments for `switchwall.sh`.
#[derive(Clone, Debug, PartialEq)]
pub enum WallChange {
    Image(PathBuf),
    /// Let the script ask for a file (it opens a file dialog).
    Pick,
    Mode { dark: bool },
    /// Colors from a fixed accent instead of the wallpaper (`None` = back
    /// to the wallpaper).
    Accent(Option<String>),
}

impl WallChange {
    pub fn args(&self) -> Vec<String> {
        match self {
            WallChange::Image(p) => vec!["--image".into(), p.to_string_lossy().into_owned()],
            WallChange::Pick => Vec::new(),
            WallChange::Mode { dark } => vec!["--mode".into(), if *dark { "dark" } else { "light" }.into(), "--noswitch".into()],
            WallChange::Accent(Some(hex)) => vec!["--color".into(), hex.clone(), "--noswitch".into()],
            WallChange::Accent(None) => vec!["--color".into(), "clear".into(), "--noswitch".into()],
        }
    }
}

/// Run `switchwall.sh`. It takes a few seconds (it generates the palettes).
pub fn run_switchwall(home: &Path, change: &WallChange) -> Result<(), String> {
    let script = switchwall(home).ok_or("The end-4 shell's switchwall.sh wasn't found")?;
    let args = change.args();
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    cmd::run(&script.to_string_lossy(), &refs).map(|_| ())
}

/// The wallpaper and accent color in config.json.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    pub wallpaper: Option<PathBuf>,
    pub accent: Option<String>,
}

pub fn parse_look(config: &Value) -> Look {
    let s = |key: &str| {
        lookup(config, key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    Look {
        wallpaper: s("background.wallpaperPath").map(PathBuf::from),
        accent: s("appearance.palette.accentColor"),
    }
}

pub fn read_look(home: &Path) -> Look {
    std::fs::read_to_string(config_path(home))
        .ok()
        .and_then(|t| crate::formats::jsonc::parse(&t).ok())
        .map(|v| parse_look(&v))
        .unwrap_or_default()
}

fn is_image(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| WALLPAPER_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Images in `dir` and its subfolders (one level, like `Wallpapers/showcase`),
/// sorted by name.
pub fn images_in(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    let mut subdirs = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            subdirs.push(p);
        } else if is_image(&p) {
            out.push(p);
        }
    }
    subdirs.sort();
    for d in subdirs {
        if let Ok(entries) = std::fs::read_dir(&d) {
            out.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_file() && is_image(p)));
        }
    }
    out.sort_by_key(|p| p.file_name().map(|n| n.to_ascii_lowercase()));
    out
}

/// Wallpapers to offer: the shell's default, then `~/Pictures/Wallpapers`,
/// without duplicates. The current one is always included (first, when it
/// isn't among them), and the order doesn't change when it does.
pub fn wallpaper_choices(current: Option<&Path>, default: Option<&Path>, folder: &Path, limit: usize) -> Vec<PathBuf> {
    let usable = |p: &&Path| p.is_file() && is_image(p);
    let mut all: Vec<PathBuf> = Vec::new();
    for p in default.filter(usable).map(Path::to_path_buf).into_iter().chain(images_in(folder)) {
        if !all.contains(&p) {
            all.push(p);
        }
    }
    let mut out: Vec<PathBuf> = all.into_iter().take(limit).collect();
    if let Some(cur) = current.filter(usable) {
        if !out.iter().any(|p| p == cur) && limit > 0 {
            out.truncate(limit - 1);
            out.insert(0, cur.to_path_buf());
        }
    }
    out
}

pub fn wallpapers_dir() -> PathBuf {
    dirs::picture_dir()
        .unwrap_or_else(|| material_3_ui::sys::paths::home().join("Pictures"))
        .join("Wallpapers")
}

// --------------------------------------------------------------------- schema

#[derive(Deserialize)]
struct RawOption {
    key: String,
    #[serde(rename = "type")]
    ty: String,
    default: Value,
    comment: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    Bool,
    Int { min: i64, max: i64, slider: bool },
    Real { min: f64, max: f64, slider: bool },
    Text,
    IntEnum(Vec<(i64, String)>),
    StrEnum(Vec<String>),
    /// Lists and other JSON values, edited as JSON text.
    Json,
}

#[derive(Clone, Debug)]
pub struct Opt {
    /// Dotted path, e.g. `appearance.palette.type`.
    pub key: String,
    pub default: Value,
    pub comment: String,
    pub label: String,
    /// Humanized parent path ("Widgets › Clock"), empty for top-level keys.
    pub group: String,
    /// First path segment, or "general" for top-level keys.
    pub section: String,
    pub control: Control,
}

impl Opt {
    /// `/`-separated path for [`crate::formats::jsonc`].
    pub fn path(&self) -> String {
        self.key.replace('.', "/")
    }
}

const SCHEMA_JSON: &str = include_str!("../assets/end4/schema.json");

pub fn schema() -> &'static [Opt] {
    static SCHEMA: OnceLock<Vec<Opt>> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        let raw: Vec<RawOption> = serde_json::from_str(SCHEMA_JSON).unwrap_or_default();
        raw.into_iter().map(build_opt).collect()
    })
}

pub fn find(key: &str) -> Option<&'static Opt> {
    schema().iter().find(|o| o.key == key)
}

/// Top-level sections in schema order.
pub fn sections() -> Vec<&'static str> {
    let mut out: Vec<&str> = Vec::new();
    for o in schema() {
        if !out.contains(&o.section.as_str()) {
            out.push(&o.section);
        }
    }
    out
}

fn build_opt(r: RawOption) -> Opt {
    let parts: Vec<&str> = r.key.split('.').collect();
    let (last, parents) = parts.split_last().expect("non-empty key");
    let label = humanize(last);
    let section = if parents.is_empty() {
        "general".to_string()
    } else {
        parents[0].to_string()
    };
    let group = parents
        .iter()
        .skip(1)
        .map(|p| humanize(p))
        .collect::<Vec<_>>()
        .join(" › ");
    let control = control_for(&r.ty, last, &section, &r.default, &r.comment);
    Opt {
        key: r.key,
        default: r.default,
        comment: r.comment,
        label,
        group,
        section,
        control,
    }
}

fn control_for(ty: &str, name: &str, section: &str, default: &Value, comment: &str) -> Control {
    let lname = name.to_lowercase();
    match ty {
        "bool" => Control::Bool,
        "int" => {
            if let Some(opts) = parse_int_enum(comment) {
                return Control::IntEnum(opts);
            }
            let d = default.as_i64().unwrap_or(0);
            let percent = lname.contains("threshold")
                || (section == "battery" && !lname.contains("suspend"))
                || lname.contains("percent");
            if percent && (0..=101).contains(&d) {
                Control::Int { min: 0, max: if d == 101 { 101 } else { 100 }, slider: true }
            } else {
                Control::Int {
                    min: if d < 0 { i64::MIN / 2 } else { 0 },
                    max: i64::MAX / 2,
                    slider: false,
                }
            }
        }
        "real" => {
            let d = default.as_f64().unwrap_or(0.0);
            let unit = [
                "transparency",
                "opacity",
                "alpha",
                "ratio",
                "harmony",
                "boost",
                "damping",
                "roundness",
            ]
            .iter()
            .any(|w| lname.contains(w));
            if (unit || lname == "scale") && (0.0..=1.0).contains(&d) {
                Control::Real { min: 0.0, max: 1.0, slider: true }
            } else {
                Control::Real {
                    min: if d < 0.0 { f64::MIN } else { 0.0 },
                    max: f64::MAX,
                    slider: false,
                }
            }
        }
        "string" => match parse_str_enum(comment) {
            Some(mut opts) => {
                if let Some(d) = default.as_str() {
                    if !opts.iter().any(|o| o == d) {
                        opts.insert(0, d.to_string());
                    }
                }
                Control::StrEnum(opts)
            }
            None => Control::Text,
        },
        _ => Control::Json,
    }
}

/// `0: No | 1: Yes | 2: Local` → [(0, "No"), …].
pub fn parse_int_enum(comment: &str) -> Option<Vec<(i64, String)>> {
    let mut out = Vec::new();
    for piece in comment.split('|') {
        let (n, label) = piece.split_once(':')?;
        let n: i64 = n.trim().parse().ok()?;
        let label = label.trim();
        if label.is_empty() {
            return None;
        }
        out.push((n, label.to_string()));
    }
    (out.len() >= 2).then_some(out)
}

fn is_word(s: &str) -> bool {
    !s.is_empty()
        && s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// String options from a comment: quoted values (`"cookie", "digital"`),
/// `Allowed: a, b, c`, or bare alternatives (`float | hug`, `rect - dot`).
pub fn parse_str_enum(comment: &str) -> Option<Vec<String>> {
    // Quoted values.
    let mut quoted = Vec::new();
    let mut rest = comment;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('"') else { break };
        let v = &after[..end];
        if !v.is_empty() && !quoted.iter().any(|q| q == v) {
            quoted.push(v.to_string());
        }
        rest = &after[end + 1..];
    }
    if quoted.len() >= 2 {
        return Some(quoted);
    }
    // Allowed: a, b, c
    if let Some(idx) = comment.find("Allowed:") {
        let list: Vec<String> = comment[idx + "Allowed:".len()..]
            .split(',')
            .map(|s| s.trim().trim_end_matches('.').to_string())
            .collect();
        if list.len() >= 2 && list.iter().all(|s| is_word(s)) {
            return Some(list);
        }
        return None;
    }
    // Bare alternatives.
    for sep in [" | ", " - ", " or "] {
        if comment.contains(sep) {
            let list: Vec<String> = comment.split(sep).map(|s| s.trim().to_string()).collect();
            if list.len() >= 2 && list.iter().all(|s| is_word(s) && !s.chars().all(|c| c.is_ascii_digit())) {
                return Some(list);
            }
        }
    }
    None
}

const ACRONYMS: &[&str] = &[
    "ai", "ui", "osk", "osd", "gps", "cpu", "ram", "gpu", "vrr", "id", "url", "api", "uscs", "si",
];

/// `fakeScreenRounding` → "Fake screen rounding", `enableGPS` → "Enable GPS".
pub fn humanize(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };
        let next = chars.get(i + 1).copied();
        let boundary = match prev {
            None => false,
            Some(p) if c == '_' || c == '-' => {
                let _ = p;
                true
            }
            Some(p) if c.is_uppercase() => {
                p.is_lowercase()
                    || p.is_ascii_digit()
                    || (p.is_uppercase() && next.is_some_and(|n| n.is_lowercase()))
            }
            Some(p) if c.is_ascii_digit() => p.is_alphabetic(),
            _ => false,
        };
        if boundary && !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
        if c != '_' && c != '-' {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let lower = w.to_lowercase();
            if lower == "qt" {
                "Qt".to_string()
            } else if ACRONYMS.contains(&lower.as_str()) || (w.len() > 1 && w.chars().all(|c| c.is_uppercase())) {
                w.to_uppercase()
            } else if i == 0 {
                let mut cs = lower.chars();
                cs.next()
                    .map(|f| f.to_uppercase().collect::<String>() + cs.as_str())
                    .unwrap_or_default()
            } else {
                lower
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A full config built from the schema defaults (for creating the file).
pub fn default_config() -> Value {
    let mut root = Value::Object(Default::default());
    for o in schema() {
        let mut cur = &mut root;
        let parts: Vec<&str> = o.key.split('.').collect();
        let (last, parents) = parts.split_last().expect("non-empty");
        for p in parents {
            cur = cur
                .as_object_mut()
                .expect("objects only")
                .entry(p.to_string())
                .or_insert_with(|| Value::Object(Default::default()));
        }
        cur.as_object_mut()
            .expect("objects only")
            .insert(last.to_string(), o.default.clone());
    }
    root
}

/// Look a dotted key up in a parsed config.
pub fn lookup<'a>(root: &'a Value, key: &str) -> Option<&'a Value> {
    let mut v = root;
    for part in key.split('.') {
        v = v.as_object()?.get(part)?;
    }
    Some(v)
}

/// Values compare equal across int/float spellings (`1` vs `1.0`).
pub fn same_value(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => (x - y).abs() < 1e-9,
        _ => a == b,
    }
}

/// Options picked for the Essentials card, in display order.
pub const ESSENTIALS: &[&str] = &[
    "panelFamily",
    "appearance.palette.type",
    "appearance.transparency.enable",
    "appearance.fakeScreenRounding",
    "appearance.fonts.main",
    "policies.ai",
    "policies.weeb",
    "bar.bottom",
    "bar.vertical",
    "bar.cornerStyle",
    "bar.autoHide.enable",
    "bar.workspaces.shown",
    "bar.weather.enable",
    "time.format",
    "time.dateFormat",
    "dock.enable",
    "dock.position",
    "background.widgets.clock.style",
    "battery.low",
    "battery.critical",
    "battery.automaticSuspend",
    "lock.useHyprlock",
    "light.night.automatic",
];

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn humanizer() {
        assert_eq!(humanize("fakeScreenRounding"), "Fake screen rounding");
        assert_eq!(humanize("enableGPS"), "Enable GPS");
        assert_eq!(humanize("useUSCS"), "Use USCS");
        assert_eq!(humanize("aiUsage"), "AI usage");
        assert_eq!(humanize("panelFamily"), "Panel family");
        assert_eq!(humanize("x"), "X");
        assert_eq!(humanize("show_hidden"), "Show hidden");
    }

    #[test]
    fn int_enums() {
        assert_eq!(
            parse_int_enum("0: No | 1: Yes | 2: Local"),
            Some(vec![(0, "No".into()), (1, "Yes".into()), (2, "Local".into())])
        );
        assert_eq!(parse_int_enum("milliseconds"), None);
        assert_eq!(parse_int_enum("0: only"), None);
    }

    #[test]
    fn string_enums() {
        assert_eq!(
            parse_str_enum(r#"Options: "cookie", "digital", "daily""#),
            Some(vec!["cookie".into(), "digital".into(), "daily".into()])
        );
        assert_eq!(
            parse_str_enum("Allowed: auto, scheme-content, scheme-tonal-spot"),
            Some(vec!["auto".into(), "scheme-content".into(), "scheme-tonal-spot".into()])
        );
        assert_eq!(
            parse_str_enum(r#""" = from wallpaper. Allowed: any file name in scripts/colors/schemes (gruvbox)"#),
            None
        );
        assert_eq!(parse_str_enum("float | hug"), Some(vec!["float".into(), "hug".into()]));
        assert_eq!(
            parse_str_enum("rect - dot - space"),
            Some(vec!["rect".into(), "dot".into(), "space".into()])
        );
        assert_eq!(parse_str_enum("25 50 100"), None);
        assert_eq!(parse_str_enum("Instead of top"), None);
    }

    #[test]
    fn schema_loads() {
        let s = schema();
        assert!(s.len() > 500);
        let fam = find("panelFamily").unwrap();
        assert_eq!(fam.section, "general");
        assert!(matches!(fam.control, Control::StrEnum(ref v) if v.contains(&"waffle".to_string())));
        assert!(matches!(find("policies.ai").unwrap().control, Control::IntEnum(_)));
        assert!(matches!(
            find("appearance.transparency.backgroundTransparency").unwrap().control,
            Control::Real { slider: true, .. }
        ));
        assert_eq!(find("background.widgets.clock.style").unwrap().group, "Widgets › Clock");
        for key in ESSENTIALS {
            assert!(find(key).is_some(), "essential {key} missing from schema");
        }
        let defaults = default_config();
        assert_eq!(lookup(&defaults, "panelFamily"), Some(&json!("ii")));
    }

    #[test]
    fn qs_config_parsing() {
        let lua = "-- Default\nhl.env(\"qsConfig\", \"ii\")\n-- hl.env(\"qsConfig\", \"x\")\n";
        assert_eq!(parse_qs_config(lua).as_deref(), Some("ii"));
        assert_eq!(parse_qs_config("$qsConfig = end4-pC # mine\n").as_deref(), Some("end4-pC"));
        assert_eq!(parse_qs_config("env = qsConfig,ii\n").as_deref(), Some("ii"));
        assert_eq!(parse_qs_config("terminal = \"kitty\"\n"), None);
    }

    #[test]
    fn qs_config_rewriting() {
        // Fresh custom file.
        let out = rewrite_qs_config("", "end4-pC", true);
        assert!(out.starts_with("-- This file"));
        assert_eq!(parse_qs_config(&out).as_deref(), Some("end4-pC"));
        // Existing assignment is replaced in place, other lines kept.
        let text = "terminal = \"kitty\"\n  hl.env(\"qsConfig\", \"ii\")\n";
        let out = rewrite_qs_config(text, "end4-pC", true);
        assert_eq!(out, "terminal = \"kitty\"\n  hl.env(\"qsConfig\", \"end4-pC\")\n");
        // Comments are left alone, assignment appended.
        let out = rewrite_qs_config("-- hl.env(\"qsConfig\", \"ii\")\n", "ii", true);
        assert!(out.starts_with("-- hl.env(\"qsConfig\", \"ii\")\n"));
        assert!(out.ends_with("hl.env(\"qsConfig\", \"ii\")\n"));
        // hyprlang
        let out = rewrite_qs_config("$qsConfig = ii\n", "end4-pC", false);
        assert_eq!(out, "$qsConfig = end4-pC\n");
    }

    #[test]
    fn running_configs() {
        let out = "123 qs -c end4-pC\n456 qs -c ii ipc call cliphistService update\n789 /usr/bin/quickshell --config ii\n";
        assert_eq!(parse_running(out), vec!["end4-pC".to_string(), "ii".to_string()]);
        assert!(parse_running("").is_empty());
    }

    #[test]
    fn detection_on_fake_home() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        std::fs::create_dir_all(shell_dir(h, FORK)).unwrap();
        std::fs::write(shell_dir(h, FORK).join("shell.qml"), "").unwrap();
        std::fs::create_dir_all(h.join(".config/hypr/hyprland")).unwrap();
        std::fs::write(h.join(".config/hypr/hyprland.lua"), "").unwrap();
        std::fs::write(
            h.join(".config/hypr/hyprland/variables.lua"),
            "hl.env(\"qsConfig\", \"ii\")\n",
        )
        .unwrap();
        let s = detect(h);
        assert!(s.fork.installed && !s.ii.installed);
        assert_eq!(s.active_name(), Some("ii"));
        set_default_shell(h, FORK, false).unwrap();
        let s = detect(h);
        assert_eq!(s.active_name(), Some(FORK));
        assert!(s.active.unwrap().1.ends_with("custom/variables.lua"));
    }

    #[test]
    fn switchwall_from_the_preferred_shell() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        assert_eq!(switchwall(h), None);
        for name in [FORK, UPSTREAM] {
            let dir = shell_dir(h, name);
            std::fs::create_dir_all(dir.join("scripts/colors")).unwrap();
            std::fs::write(dir.join("shell.qml"), "").unwrap();
            std::fs::write(dir.join("scripts/colors/switchwall.sh"), "").unwrap();
        }
        assert_eq!(switchwall(h), Some(shell_dir(h, FORK).join("scripts/colors/switchwall.sh")));
        std::fs::create_dir_all(h.join(".config/hypr/custom")).unwrap();
        std::fs::write(h.join(".config/hypr/custom/variables.lua"), "hl.env(\"qsConfig\", \"ii\")\n").unwrap();
        assert_eq!(switchwall(h), Some(shell_dir(h, UPSTREAM).join("scripts/colors/switchwall.sh")));
    }

    #[test]
    fn switchwall_arguments() {
        assert_eq!(WallChange::Image("/w/a b.png".into()).args(), vec!["--image", "/w/a b.png"]);
        assert!(WallChange::Pick.args().is_empty());
        assert_eq!(WallChange::Mode { dark: true }.args(), vec!["--mode", "dark", "--noswitch"]);
        assert_eq!(WallChange::Accent(Some("#6750A4".into())).args(), vec!["--color", "#6750A4", "--noswitch"]);
        assert_eq!(WallChange::Accent(None).args(), vec!["--color", "clear", "--noswitch"]);
    }

    #[test]
    fn look_and_wallpaper_choices() {
        let look = parse_look(&json!({"background": {"wallpaperPath": "/w/cur.jpg"}, "appearance": {"palette": {"accentColor": ""}}}));
        assert_eq!(look, Look { wallpaper: Some("/w/cur.jpg".into()), accent: None });
        assert_eq!(parse_look(&json!({"appearance": {"palette": {"accentColor": "#FF0000"}}})).accent.as_deref(), Some("#FF0000"));

        let dir = tempfile::tempdir().unwrap();
        let walls = dir.path().join("Wallpapers");
        std::fs::create_dir_all(walls.join("showcase")).unwrap();
        for f in ["b.jpg", "a.PNG", "notes.txt", "showcase/c.webp"] {
            std::fs::write(walls.join(f), "").unwrap();
        }
        let default = dir.path().join("default_wallpaper.png");
        std::fs::write(&default, "").unwrap();
        let names = |v: Vec<PathBuf>| v.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>();
        assert_eq!(names(images_in(&walls)), vec!["a.PNG", "b.jpg", "c.webp"]);
        let current = walls.join("b.jpg");
        assert_eq!(
            names(wallpaper_choices(Some(&current), Some(&default), &walls, 10)),
            vec!["default_wallpaper.png", "a.PNG", "b.jpg", "c.webp"]
        );
        // A current wallpaper past the limit, or elsewhere, still shows.
        assert_eq!(names(wallpaper_choices(Some(&walls.join("showcase/c.webp")), None, &walls, 2)), vec!["c.webp", "a.PNG"]);
        let elsewhere = dir.path().join("elsewhere.jpg");
        std::fs::write(&elsewhere, "").unwrap();
        assert_eq!(names(wallpaper_choices(Some(&elsewhere), Some(&default), &walls, 10))[..2], ["elsewhere.jpg", "default_wallpaper.png"]);
        assert_eq!(wallpaper_choices(None, None, &walls, 2).len(), 2);
        assert!(wallpaper_choices(Some(Path::new("/missing.png")), None, &dir.path().join("none"), 5).is_empty());
    }
}
