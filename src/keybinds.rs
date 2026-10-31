//! Suggested Hyprland keybinds for the Material 3 apps, which the end4-pC
//! fork's installer used to add. Only keys that are still free are bound,
//! in a marked block of our own:
//!
//! ```lua
//! -- >>> m3-keybinds >>>
//! hl.bind("SUPER + I", hl.dsp.exec_cmd("m3-settings"), {description = "System settings"})
//! -- <<< m3-keybinds <<<
//! ```
//!
//! On end-4's dots it goes in `custom/keybinds.lua` (or `.conf`), which
//! survives dots updates; otherwise at the end of the main config.

use crate::hypr::{self, Syntax};
use material_3_ui::sys::cmd;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const BEGIN: &str = ">>> m3-keybinds >>>";
const END: &str = "<<< m3-keybinds <<<";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Exec(&'static str),
    /// A Quickshell global shortcut (`quickshell:<name>`).
    Global(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Suggested {
    pub key: &'static str,
    pub action: Action,
    pub description: &'static str,
    /// Needs the game library the ii/end4-pC integration adds.
    pub games: bool,
}

pub const SUGGESTED: &[Suggested] = &[
    Suggested { key: "SUPER + I", action: Action::Exec("m3-settings"), description: "System settings", games: false },
    Suggested { key: "SUPER + E", action: Action::Exec("m3fm"), description: "Files", games: false },
    Suggested { key: "Print", action: Action::Exec("m3-screenshots output --active"), description: "Screenshot of this screen", games: false },
    Suggested { key: "SUPER + Print", action: Action::Exec("m3-screenshots window --active"), description: "Screenshot of this window", games: false },
    Suggested { key: "SUPER + SHIFT + R", action: Action::Exec("m3-screenshots record region --toggle"), description: "Record a region (again to stop)", games: false },
    Suggested { key: "SUPER + G", action: Action::Global("gamesToggle"), description: "Game library", games: true },
    Suggested { key: "SUPER + SHIFT + G", action: Action::Global("gameMode"), description: "Game Mode", games: true },
];

impl Suggested {
    pub fn line(&self, syntax: Syntax) -> String {
        match syntax {
            Syntax::Lua => {
                let dsp = match self.action {
                    Action::Exec(c) => format!("hl.dsp.exec_cmd(\"{c}\")"),
                    Action::Global(g) => format!("hl.dsp.global(\"quickshell:{g}\")"),
                };
                format!("hl.bind(\"{}\", {dsp}, {{description = \"{}\"}})", self.key, self.description)
            }
            Syntax::Conf => {
                let (mods, key) = split_key(self.key);
                let (dsp, arg) = match self.action {
                    Action::Exec(c) => ("exec", c.to_string()),
                    Action::Global(g) => ("global", format!("quickshell:{g}")),
                };
                format!("bindd = {}, {key}, {}, {dsp}, {arg}", mods.join(" "), self.description)
            }
        }
    }
}

fn split_key(key: &str) -> (Vec<String>, String) {
    let mut parts: Vec<String> = key.split('+').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
    let k = parts.pop().unwrap_or_default();
    (parts, k)
}

/// One spelling for a key combination: `super+shift+r` and `Super + Shift +
/// R` and hyprlang's `SUPER SHIFT, R` are the same.
pub fn normalize(combo: &str) -> String {
    let combo = combo.replace(',', "+");
    let mut parts: Vec<String> = combo.split(['+', ' ']).map(|p| p.trim().to_uppercase()).filter(|p| !p.is_empty()).collect();
    let Some(key) = parts.pop() else { return String::new() };
    let mut mods: Vec<String> = parts
        .into_iter()
        .map(|m| match m.as_str() {
            "WIN" | "MOD4" | "META" | "LOGO" => "SUPER".into(),
            "CONTROL" => "CTRL".into(),
            "MOD1" => "ALT".into(),
            _ => m,
        })
        .collect();
    mods.sort();
    mods.dedup();
    mods.push(key);
    mods.join("+")
}

/// The key combinations a config file binds, ignoring our own block.
pub fn bound_in(text: &str) -> Vec<String> {
    let lua = regex::Regex::new(r#"hl\.bind\(\s*"([^"]*)""#).unwrap();
    let conf = regex::Regex::new(r"^\s*bind[a-z]*\s*=\s*([^,]*),([^,]*),").unwrap();
    let mut out = Vec::new();
    let mut ours = false;
    for line in text.lines() {
        if line.contains(BEGIN) {
            ours = true;
        } else if line.contains(END) {
            ours = false;
        } else if !ours {
            let t = line.trim_start();
            if t.starts_with("--") || t.starts_with('#') {
                continue;
            }
            if let Some(c) = lua.captures(line) {
                out.push(normalize(&c[1]));
            } else if let Some(c) = conf.captures(line) {
                out.push(normalize(&format!("{}+{}", &c[1], &c[2])));
            }
        }
    }
    out
}

/// The lines of our block, if the file has one.
pub fn our_block(text: &str) -> Option<Vec<String>> {
    let start = text.lines().position(|l| l.contains(BEGIN))?;
    Some(text.lines().skip(start + 1).take_while(|l| !l.contains(END)).map(String::from).collect())
}

/// `text` with our block replaced by `lines` (taken out when empty).
pub fn with_block(text: &str, syntax: Syntax, lines: &[String]) -> String {
    let c = if syntax == Syntax::Lua { "--" } else { "#" };
    let mut kept: Vec<&str> = Vec::new();
    let mut at = None;
    let mut ours = false;
    for line in text.lines() {
        if line.contains(BEGIN) {
            ours = true;
            at = Some(kept.len());
        } else if line.contains(END) && ours {
            ours = false;
        } else if !ours {
            kept.push(line);
        }
    }
    let mut block = Vec::new();
    if !lines.is_empty() {
        block.push(format!("{c} {BEGIN}"));
        block.push(format!("{c} Material 3 apps' keybinds, added by Material 3 Settings; remove them there."));
        block.extend(lines.iter().cloned());
        block.push(format!("{c} {END}"));
    }
    let mut out: Vec<String> = kept.iter().map(|s| s.to_string()).collect();
    match at {
        Some(i) => {
            out.splice(i..i, block);
        }
        None if !block.is_empty() => {
            if out.last().is_some_and(|l| !l.trim().is_empty()) {
                out.push(String::new());
            }
            out.extend(block);
        }
        None => {}
    }
    let mut s = out.join("\n");
    if !s.is_empty() {
        s.push('\n');
    }
    s
}

pub fn target(home: &Path) -> (PathBuf, Syntax) {
    let t = hypr::target(home);
    let file = if t.end4 {
        home.join(".config/hypr/custom").join(if t.syntax == Syntax::Lua { "keybinds.lua" } else { "keybinds.conf" })
    } else {
        t.path
    };
    (file, t.syntax)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// Every suggestion that applies, with whether it's free (or ours).
    pub binds: Vec<(Suggested, bool)>,
    /// How many of them our block has now.
    pub added: usize,
}

/// What we'd bind. `live` is `hyprctl binds`' keys (includes ours).
pub fn plan(home: &Path, live: &[String], games: bool) -> Plan {
    let (file, syntax) = target(home);
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let ours: Vec<String> = our_block(&text).unwrap_or_default();
    let our_keys: BTreeSet<String> = SUGGESTED.iter().filter(|s| ours.contains(&s.line(syntax))).map(|s| normalize(s.key)).collect();
    let mut taken: BTreeSet<String> = live.iter().map(|k| normalize(k)).filter(|k| !our_keys.contains(k)).collect();
    // Other files of the config, too: binds load before Hyprland runs them.
    let hypr = home.join(".config/hypr");
    let mut stack = vec![hypr];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "lua" || x == "conf") {
                taken.extend(bound_in(&std::fs::read_to_string(&p).unwrap_or_default()));
            }
        }
    }
    let binds = SUGGESTED
        .iter()
        .filter(|s| games || !s.games)
        .map(|s| (*s, !taken.contains(&normalize(s.key))))
        .collect();
    Plan { binds, added: our_keys.len() }
}

pub fn live_keys() -> Vec<String> {
    if !hypr::running() {
        return Vec::new();
    }
    hypr::binds().unwrap_or_default().into_iter().map(|b| b.keys).collect()
}

/// Does any shell have the game library we bind keys for?
pub fn have_games(home: &Path) -> bool {
    use crate::inject::{status, Target};
    [Target::Ii, Target::End4Pc]
        .into_iter()
        .any(|t| status(home, t).record.is_some_and(|r| r.features.iter().any(|f| f == "games")))
}

/// Bind every free suggestion. Returns how many are bound.
pub fn apply(home: &Path, backups: bool) -> Result<String, String> {
    let p = plan(home, &live_keys(), have_games(home));
    let (file, syntax) = target(home);
    let lines: Vec<String> = p.binds.iter().filter(|(_, free)| *free).map(|(s, _)| s.line(syntax)).collect();
    let skipped = p.binds.len() - lines.len();
    write(&file, syntax, &lines, backups)?;
    Ok(match (lines.len(), skipped) {
        (0, _) => "Every suggested key is already bound to something else".into(),
        (n, 0) => format!("Added {n} keybinds"),
        (n, s) => format!("Added {n} keybinds; {s} keys were already bound and left alone"),
    })
}

pub fn remove(home: &Path, backups: bool) -> Result<String, String> {
    let (file, syntax) = target(home);
    write(&file, syntax, &[], backups)?;
    Ok("Removed the Material 3 keybinds".into())
}

fn write(file: &Path, syntax: Syntax, lines: &[String], backups: bool) -> Result<(), String> {
    let old = std::fs::read_to_string(file).unwrap_or_default();
    let new = with_block(&old, syntax, lines);
    if new == old {
        return Ok(());
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    if backups && file.exists() {
        let _ = std::fs::copy(file, file.with_extension(format!("{}.m3-bak", file.extension().unwrap_or_default().to_string_lossy())));
    }
    std::fs::write(file, new).map_err(|e| format!("{}: {e}", file.display()))?;
    if hypr::running() && syntax == Syntax::Conf {
        let _ = cmd::run("hyprctl", &["reload"]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spellings_normalize_alike() {
        assert_eq!(normalize("SUPER + SHIFT + R"), normalize("Shift + Super + r"));
        assert_eq!(normalize("SUPER SHIFT, R"), "SHIFT+SUPER+R");
        assert_eq!(normalize("Print"), "PRINT");
        assert_eq!(normalize("Super + Print"), normalize("SUPER, Print"));
    }

    #[test]
    fn lines_in_both_syntaxes() {
        let s = SUGGESTED[0];
        assert_eq!(s.line(Syntax::Lua), r#"hl.bind("SUPER + I", hl.dsp.exec_cmd("m3-settings"), {description = "System settings"})"#);
        assert_eq!(s.line(Syntax::Conf), "bindd = SUPER, I, System settings, exec, m3-settings");
        let g = SUGGESTED.iter().find(|s| s.key == "SUPER + SHIFT + G").unwrap();
        assert_eq!(g.line(Syntax::Conf), "bindd = SUPER SHIFT, G, Game Mode, global, quickshell:gameMode");
    }

    #[test]
    fn finds_binds_outside_our_block() {
        let text = "hl.bind(\"SUPER + E\", hl.dsp.exec_cmd(\"dolphin\"))\n-- hl.bind(\"SUPER + X\", x)\n-- >>> m3-keybinds >>>\nhl.bind(\"SUPER + I\", y)\n-- <<< m3-keybinds <<<\nbind = SUPER, Q, killactive,\n";
        assert_eq!(bound_in(text), vec![normalize("SUPER+E"), normalize("SUPER+Q")]);
    }

    #[test]
    fn block_is_added_replaced_and_removed() {
        let base = "hl.bind(\"SUPER + Q\", q)\n";
        let one = with_block(base, Syntax::Lua, &["a".into()]);
        assert!(one.starts_with(base));
        assert!(one.contains("\n-- >>> m3-keybinds >>>\n"));
        assert!(one.ends_with("a\n-- <<< m3-keybinds <<<\n"));
        let two = with_block(&format!("{one}tail\n"), Syntax::Lua, &["b".into()]);
        assert!(two.contains("b\n-- <<< m3-keybinds <<<\ntail\n"), "{two}");
        assert!(!two.contains("\na\n"));
        assert_eq!(with_block(&two, Syntax::Lua, &[]), format!("{base}\ntail\n"));
    }

    #[test]
    fn plan_skips_taken_keys_and_keeps_ours() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let hypr = home.join(".config/hypr");
        std::fs::create_dir_all(hypr.join("custom")).unwrap();
        std::fs::write(hypr.join("hyprland.lua"), "require(\"custom.general\")\n").unwrap();
        std::fs::write(hypr.join("keybinds.lua"), "hl.bind(\"SUPER + E\", hl.dsp.exec_cmd(\"dolphin\"))\n").unwrap();
        let p = plan(home, &[], false);
        assert!(p.binds.iter().all(|(s, _)| !s.games));
        let free: Vec<&str> = p.binds.iter().filter(|(_, f)| *f).map(|(s, _)| s.key).collect();
        assert!(!free.contains(&"SUPER + E"));
        assert!(free.contains(&"SUPER + I"));

        // Once added, our own keys (live in Hyprland now) still count as ours.
        let (file, syntax) = target(home);
        assert_eq!(file, hypr.join("custom/keybinds.lua"));
        write(&file, syntax, &[SUGGESTED[0].line(syntax)], false).unwrap();
        let p = plan(home, &["Super + I".into()], true);
        assert_eq!(p.added, 1);
        assert!(p.binds.iter().any(|(s, f)| s.key == "SUPER + I" && *f));
        assert!(p.binds.iter().any(|(s, _)| s.games));
    }
}
