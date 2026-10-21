//! The first-run welcome: its steps, the "greeted" marker it shares with the
//! end4-pC shell, and the keybind cheat sheet it shows.
//!
//! The shell greets a new user when `$XDG_STATE_HOME/quickshell/user/first_run.txt`
//! is missing (see end4-pC's `services/FirstRunExperience.qml`). Its own welcome
//! window writes that file to stay quiet and deletes it for "Show next time";
//! this flow does the same, so either one can greet and both agree.

use crate::hypr::Bind;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// What end4-pC writes into the marker. Only the file's existence matters.
pub const MARKER_TEXT: &str = "This file is just here to confirm you've been greeted :>";

/// The marker under a state directory (`~/.local/state`).
pub fn marker_in(state_home: &Path) -> PathBuf {
    state_home.join("quickshell/user/first_run.txt")
}

pub fn marker_path() -> PathBuf {
    marker_in(&dirs::state_dir().unwrap_or_else(|| material_3_ui::sys::paths::home().join(".local/state")))
}

/// Whether the shell considers the user greeted.
pub fn greeted(marker: &Path) -> bool {
    marker.exists()
}

/// Record the outcome: write the marker so nothing greets again, or remove
/// it so the shell greets at the next login.
pub fn set_show_next_time(marker: &Path, show: bool) -> std::io::Result<()> {
    if show {
        match std::fs::remove_file(marker) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    } else {
        if let Some(dir) = marker.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(marker, format!("{MARKER_TEXT}\n"))
    }
}

// ---------------------------------------------------------------------- steps

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Hello,
    Look,
    Apps,
    Defaults,
    Keys,
    Done,
}

impl Step {
    pub const ALL: [Step; 6] = [Step::Hello, Step::Look, Step::Apps, Step::Defaults, Step::Keys, Step::Done];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    pub fn title(self) -> &'static str {
        match self {
            Step::Hello => "Hello",
            Step::Look => "Look",
            Step::Apps => "Apps",
            Step::Defaults => "Defaults",
            Step::Keys => "Keys",
            Step::Done => "Done",
        }
    }

    pub fn next(self) -> Step {
        Self::ALL.get(self.index() + 1).copied().unwrap_or(Step::Done)
    }

    pub fn prev(self) -> Step {
        Self::ALL[self.index().saturating_sub(1)]
    }
}

/// Where the user is in the flow and what they picked along the way.
#[derive(Clone, Debug, PartialEq)]
pub struct Flow {
    pub step: Step,
    /// Optional Material 3 apps ticked on the Apps step, by short name.
    pub selected: BTreeSet<&'static str>,
    /// The "Show this at next login" switch on the last step.
    pub show_next_time: bool,
    /// The outcome has been recorded (marker and config written).
    pub finished: bool,
}

impl Default for Flow {
    fn default() -> Self {
        Self {
            step: Step::Hello,
            selected: BTreeSet::new(),
            show_next_time: false,
            finished: false,
        }
    }
}

impl Flow {
    pub fn next(&mut self) {
        self.step = self.step.next();
    }

    pub fn back(&mut self) {
        self.step = self.step.prev();
    }

    /// Leave the rest out and go straight to the end.
    pub fn skip_all(&mut self) {
        self.step = Step::Done;
    }

    pub fn can_go_back(&self) -> bool {
        self.step != Step::Hello
    }

    pub fn toggle(&mut self, app: &'static str) {
        if !self.selected.remove(app) {
            self.selected.insert(app);
        }
    }
}

// ----------------------------------------------------------------- keybinds

/// One line of the cheat sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyHint {
    /// `Super + Shift + S`.
    pub keys: String,
    pub label: &'static str,
}

/// The most useful binds: the description end-4's keybinds.lua gives each
/// (which `hyprctl binds` reports, so rebinds show up), the default keys,
/// and what to call it here.
const HINTS: &[(&str, &str, &str)] = &[
    ("Shell: Toggle search", "Super", "Search apps, files and the web"),
    ("Shell: Toggle cheatsheet", "Super + /", "Every keybind (cheat sheet)"),
    ("App: Settings app", "Super + I", "Settings"),
    ("Shell: Toggle overview", "Super + Tab", "Overview of your workspaces"),
    ("App: Terminal", "Super + Enter", "Terminal"),
    ("App: File manager", "Super + E", "Files"),
    ("App: Browser", "Super + W", "Browser"),
    ("Window: Close", "Super + Q", "Close the window"),
    ("Window: Float/Tile", "Super + Alt + Space", "Float or tile the window"),
    ("Utilities: Screen snip", "Super + Shift + S", "Screenshot a region"),
    ("Shell: Change wallpaper", "Ctrl + Super + T", "Change the wallpaper"),
    ("Shell: Toggle right sidebar", "Super + N", "Quick settings and notifications"),
    ("Shell: Toggle left sidebar", "Super + A", "Sidebar with AI chat and translator"),
    ("Session: Lock", "Super + L", "Lock the screen"),
    ("Shell: Toggle session menu", "Ctrl + Alt + Delete", "Log out, restart or shut down"),
];

/// Hyprland key names as people write them.
fn pretty_key(key: &str) -> String {
    match key.to_ascii_lowercase().as_str() {
        "slash" => "/".into(),
        "period" => ".".into(),
        "comma" => ",".into(),
        "minus" => "-".into(),
        "equal" => "=".into(),
        "semicolon" => ";".into(),
        "apostrophe" => "'".into(),
        "backslash" => "\\".into(),
        "return" => "Enter".into(),
        "escape" => "Esc".into(),
        "space" => "Space".into(),
        "tab" => "Tab".into(),
        "delete" => "Delete".into(),
        "print" => "Print".into(),
        _ if key.chars().count() == 1 => key.to_uppercase(),
        _ => key.to_string(),
    }
}

/// `Super + Slash` → `Super + /`; `Super + Super_L` → `Super`.
pub fn pretty_keys(keys: &str) -> String {
    let parts: Vec<&str> = keys.split('+').map(str::trim).filter(|s| !s.is_empty()).collect();
    let mut out: Vec<String> = Vec::new();
    for part in parts {
        let pretty = match part.to_ascii_uppercase().as_str() {
            "SUPER" | "SUPER_L" | "SUPER_R" | "META" => "Super".to_string(),
            "CTRL" | "CONTROL" => "Ctrl".to_string(),
            "ALT" => "Alt".to_string(),
            "SHIFT" => "Shift".to_string(),
            _ => pretty_key(part),
        };
        if !out.contains(&pretty) {
            out.push(pretty);
        }
    }
    out.join(" + ")
}

/// The cheat sheet, with keys from the running Hyprland where it has the
/// bind (the last one wins, as user binds load after the defaults) and the
/// end-4 defaults otherwise.
pub fn cheat_sheet(live: &[Bind]) -> Vec<KeyHint> {
    HINTS
        .iter()
        .map(|(description, default, label)| {
            let keys = live
                .iter()
                .rev()
                .find(|b| b.description == *description)
                .map(|b| pretty_keys(&b.keys))
                .unwrap_or_else(|| default.to_string());
            KeyHint { keys, label }
        })
        .collect()
}

/// How many cheat sheet lines came from the live config.
pub fn live_count(live: &[Bind]) -> usize {
    HINTS
        .iter()
        .filter(|(d, _, _)| live.iter().any(|b| b.description == *d))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_matches_the_shell() {
        let dir = tempfile::tempdir().unwrap();
        let marker = marker_in(dir.path());
        assert!(marker.ends_with("quickshell/user/first_run.txt"));
        assert!(!greeted(&marker));
        set_show_next_time(&marker, false).unwrap();
        assert!(greeted(&marker));
        assert_eq!(std::fs::read_to_string(&marker).unwrap().trim(), MARKER_TEXT);
        // Writing again is fine, and so is removing twice.
        set_show_next_time(&marker, false).unwrap();
        set_show_next_time(&marker, true).unwrap();
        assert!(!greeted(&marker));
        set_show_next_time(&marker, true).unwrap();
    }

    #[test]
    fn steps_walk_forward_and_back() {
        let mut f = Flow::default();
        assert_eq!(f.step, Step::Hello);
        assert!(!f.can_go_back());
        let mut seen = vec![f.step];
        while f.step != Step::Done {
            f.next();
            seen.push(f.step);
        }
        assert_eq!(seen, Step::ALL.to_vec());
        f.next();
        assert_eq!(f.step, Step::Done, "Next on the last step stays there");
        f.back();
        assert_eq!(f.step, Step::Keys);
        f.step = Step::Hello;
        f.back();
        assert_eq!(f.step, Step::Hello);
        f.skip_all();
        assert_eq!(f.step, Step::Done);
    }

    #[test]
    fn app_selection_toggles() {
        let mut f = Flow::default();
        assert!(!f.show_next_time, "the switch starts off");
        f.toggle("music");
        f.toggle("games");
        f.toggle("music");
        assert_eq!(f.selected.iter().copied().collect::<Vec<_>>(), vec!["games"]);
    }

    #[test]
    fn key_names_read_like_the_shell() {
        assert_eq!(pretty_keys("Super + Slash"), "Super + /");
        assert_eq!(pretty_keys("Super + SUPER_L"), "Super");
        assert_eq!(pretty_keys("Super + Shift + s"), "Super + Shift + S");
        assert_eq!(pretty_keys("Super + Return"), "Super + Enter");
        assert_eq!(pretty_keys("Ctrl + Alt + Delete"), "Ctrl + Alt + Delete");
    }

    #[test]
    fn cheat_sheet_prefers_live_binds() {
        let defaults = cheat_sheet(&[]);
        assert_eq!(defaults.len(), HINTS.len());
        let settings = defaults.iter().find(|h| h.label == "Settings").unwrap();
        assert_eq!(settings.keys, "Super + I");
        assert_eq!(live_count(&[]), 0);

        let bind = |keys: &str, d: &str| Bind { keys: keys.into(), action: String::new(), description: d.into() };
        let live = vec![
            bind("Super + I", "App: Settings app"),
            bind("Super + Shift + I", "App: Settings app"),
            bind("Super + Slash", "Shell: Toggle cheatsheet"),
            bind("Super + SUPER_L", "Shell: Toggle search"),
        ];
        let sheet = cheat_sheet(&live);
        let keys = |label: &str| sheet.iter().find(|h| h.label == label).unwrap().keys.clone();
        assert_eq!(keys("Settings"), "Super + Shift + I");
        assert_eq!(keys("Every keybind (cheat sheet)"), "Super + /");
        assert_eq!(keys("Search apps, files and the web"), "Super");
        assert_eq!(keys("Files"), "Super + E");
        assert_eq!(live_count(&live), 3);
    }
}
