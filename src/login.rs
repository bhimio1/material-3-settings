//! The login screen (an SDDM theme) and boot splash (a Plymouth theme) in
//! the shell's colors. They used to ship with the end4-pC fork; now they
//! follow whichever shell Material 3 Settings integrates with, through the
//! shell's `$XDG_STATE_HOME/m3/style.json`.
//!
//! The themes and their install scripts live in `login/` (installed to
//! `/usr/share/m3-settings/login`). The scripts ask for root themselves
//! (pkexec, else sudo), once per run.
//!
//! SDDM can't read your home, so `sddm/sync.sh` copies the wallpaper,
//! colors, fonts and avatar into `/var/lib/end4-material-sddm` (owned by
//! you). A systemd user path unit runs it again whenever style.json changes.
//! The Plymouth frames are baked into the initramfs, so refreshing their
//! colors stays a button.

use crate::inject::watcher::{self, Watch};
use material_3_ui::sys::cmd;
use std::path::{Path, PathBuf};

/// The theme ids are the fork's, so existing installs carry on.
pub const SDDM_THEME: &str = "end4-material";
pub const SDDM_SYNC_DIR: &str = "/var/lib/end4-material-sddm";
pub const PLYMOUTH_THEME: &str = "end4-pc";

const INSTALLED_DIR: &str = "/usr/share/m3-settings/login";

/// Where the `login/` scripts are: `$M3_LOGIN_DIR`, the package's copy, or
/// the source tree for development builds.
pub fn scripts_dir() -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("M3_LOGIN_DIR").map(PathBuf::from),
        Some(PathBuf::from(INSTALLED_DIR)),
        Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("login")),
    ];
    candidates.into_iter().flatten().find(|d| d.join("sddm/install.sh").is_file())
}

fn script(rel: &str) -> Result<PathBuf, String> {
    scripts_dir()
        .map(|d| d.join(rel))
        .filter(|p| p.is_file())
        .ok_or_else(|| format!("{rel} wasn't found (reinstall Material 3 Settings, or set M3_LOGIN_DIR)"))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub sddm_available: bool,
    pub sddm_installed: bool,
    /// The theme SDDM will show (the last `[Theme] Current=` it reads).
    pub sddm_current: Option<String>,
    /// The display manager systemd starts, when it isn't SDDM.
    pub other_dm: Option<String>,
    pub plymouth_available: bool,
    pub plymouth_installed: bool,
    pub plymouth_current: Option<String>,
}

impl Status {
    pub fn sddm_selected(&self) -> bool {
        self.sddm_current.as_deref() == Some(SDDM_THEME)
    }
    pub fn plymouth_selected(&self) -> bool {
        self.plymouth_current.as_deref() == Some(PLYMOUTH_THEME)
    }
}

/// The `Current=` of a `[Theme]` section, if the file sets one.
pub fn theme_current(text: &str) -> Option<String> {
    let mut section = String::new();
    let mut found = None;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            section = t.chars().filter(|c| !c.is_whitespace()).collect();
        } else if section == "[Theme]" {
            if let Some(v) = t.strip_prefix("Current").map(str::trim_start).and_then(|r| r.strip_prefix('=')) {
                found = Some(v.trim().to_string());
            }
        }
    }
    found
}

/// SDDM's effective theme, reading its config files in its own order.
pub fn sddm_current(root: &Path) -> Option<String> {
    let mut files = Vec::new();
    for dir in ["usr/lib/sddm/sddm.conf.d", "etc/sddm.conf.d"] {
        let mut in_dir: Vec<PathBuf> = std::fs::read_dir(root.join(dir))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "conf"))
            .collect();
        in_dir.sort();
        files.extend(in_dir);
    }
    files.push(root.join("etc/sddm.conf"));
    files
        .iter()
        .rev()
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .filter_map(|t| theme_current(&t))
        .find(|v| !v.is_empty())
}

pub fn status() -> Status {
    let root = Path::new("/");
    let dm = std::fs::read_link("/etc/systemd/system/display-manager.service")
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()));
    let plymouth_current = cmd::run("plymouth-set-default-theme", &[])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Status {
        sddm_available: cmd::exists("sddm") || Path::new("/usr/bin/sddm").exists(),
        sddm_installed: root.join("usr/share/sddm/themes").join(SDDM_THEME).join("Main.qml").is_file(),
        sddm_current: sddm_current(root),
        other_dm: dm.filter(|d| d != "sddm"),
        plymouth_available: cmd::exists("plymouth-set-default-theme"),
        plymouth_installed: root.join("usr/share/plymouth/themes").join(PLYMOUTH_THEME).join(format!("{PLYMOUTH_THEME}.plymouth")).is_file(),
        plymouth_current,
    }
}

fn run_script(rel: &str, args: &[&str]) -> Result<String, String> {
    let path = script(rel)?;
    let path = path.to_string_lossy();
    let mut all = vec![path.as_ref()];
    all.extend_from_slice(args);
    cmd::run("bash", &all)
}

pub fn install_sddm(home: &Path) -> Result<String, String> {
    run_script("sddm/install.sh", &[])?;
    sync_watcher(home)?;
    Ok("Login screen installed".into())
}

pub fn remove_sddm(home: &Path) -> Result<String, String> {
    run_script("sddm/install.sh", &["--uninstall"])?;
    // The sync folder is gone, so the watcher would have nothing to do.
    let _ = watcher::sync_watch(home, &LOGIN_WATCH, &[]);
    Ok("Login screen removed; SDDM uses its previous theme".into())
}

/// Copy the current look to the login screen. Quiet when it isn't installed.
pub fn sync() -> Result<String, String> {
    run_script("sddm/sync.sh", &["--if-installed"]).map(|_| "Login screen updated".into())
}

/// Render the boot splash in the current colors, install it and rebuild the
/// initramfs. The script prints what's still missing (the mkinitcpio hook,
/// `splash` on the kernel command line).
pub fn install_plymouth() -> Result<String, String> {
    let out = run_script("plymouth/install.sh", &[])?;
    let notes: Vec<&str> = out.lines().filter(|l| l.starts_with("Note:")).collect();
    Ok(if notes.is_empty() {
        "Boot animation installed; you'll see it on the next boot".into()
    } else {
        format!("Boot animation installed. {}", notes.join(" "))
    })
}

pub fn style_path(home: &Path) -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && home == material_3_ui::sys::paths::home())
        .unwrap_or_else(|| home.join(".local/state"))
        .join("m3/style.json")
}

pub const LOGIN_WATCH: Watch = Watch {
    unit: "m3-login-sync",
    description: "Keep the Material 3 login screen in your shell's colors",
    what: "copies your shell's look to the login\n# screen whenever it changes",
    comment: "# Wallpaper switches rewrite the style a few times; wait for the last.",
    delay: 2,
    args: "login sync",
};

/// Watch style.json while the login theme is installed.
pub fn sync_watcher(home: &Path) -> Result<(), String> {
    let installed = Path::new(SDDM_SYNC_DIR).is_dir();
    let paths = if installed { vec![style_path(home)] } else { vec![] };
    if let Some(dir) = paths.first().and_then(|p| p.parent()) {
        // PathChanged on a file in a missing folder never fires.
        let _ = std::fs::create_dir_all(dir);
    }
    watcher::sync_watch(home, &LOGIN_WATCH, &paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_theme_line() {
        assert_eq!(theme_current("[General]\nCurrent=no\n[Theme]\nCurrent = breeze\n"), Some("breeze".into()));
        assert_eq!(theme_current("[ Theme ]\n#Current=x\nCursorTheme=y\n"), None);
        assert_eq!(theme_current("[Theme]\nCurrent=a\n[Users]\nCurrent=b\n"), Some("a".into()));
    }

    #[test]
    fn later_files_win() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        std::fs::create_dir_all(r.join("usr/lib/sddm/sddm.conf.d")).unwrap();
        std::fs::create_dir_all(r.join("etc/sddm.conf.d")).unwrap();
        std::fs::write(r.join("usr/lib/sddm/sddm.conf.d/default.conf"), "[Theme]\nCurrent=\n").unwrap();
        assert_eq!(sddm_current(r), None);
        std::fs::write(r.join("etc/sddm.conf.d/zz-end4-material.conf"), "[Theme]\nCurrent=end4-material\n").unwrap();
        assert_eq!(sddm_current(r).as_deref(), Some(SDDM_THEME));
        std::fs::write(r.join("etc/sddm.conf"), "[Theme]\nCurrent=breeze\n").unwrap();
        assert_eq!(sddm_current(r).as_deref(), Some("breeze"));
    }

    #[test]
    fn the_scripts_ship_with_the_source() {
        let d = scripts_dir().expect("login/ next to Cargo.toml");
        for f in ["sddm/install.sh", "sddm/sync.sh", "sddm/theme/Main.qml", "sddm/theme/fonts/MaterialSymbolsLite.ttf", "plymouth/install.sh", "plymouth/render.py"] {
            assert!(d.join(f).is_file(), "{f}");
        }
    }

    #[test]
    fn sync_reads_any_shells_style() {
        if !cmd::exists("jq") {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let out = tmp.path().join("out");
        std::fs::create_dir_all(home.join(".local/state/m3")).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(
            home.join(".local/state/m3/style.json"),
            r##"{"version":1,"shell":"dux","colors":{"primary":"#d0bcff","on_primary":"#381e72"},"fonts":{"main":"Inter"}}"##,
        )
        .unwrap();
        let status = std::process::Command::new("bash")
            .arg(scripts_dir().unwrap().join("sddm/sync.sh"))
            .args(["--sync-dir", &out.to_string_lossy()])
            .env("HOME", &home)
            .env_remove("XDG_STATE_HOME")
            .env_remove("XDG_CONFIG_HOME")
            .status()
            .unwrap();
        assert!(status.success());
        let conf = std::fs::read_to_string(out.join("theme.conf.user")).unwrap();
        assert!(conf.contains("color_primary=\"#d0bcff\"\n"), "{conf}");
        assert!(conf.contains("color_on_primary=\"#381e72\"\n"));
        assert!(conf.contains("fontFamily=\"Inter\"\n"));
    }
}
