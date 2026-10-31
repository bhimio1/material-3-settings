//! The systemd user path unit that re-applies the integration after a
//! shell updates itself (and replaces our edited files with upstream ones).
//!
//! It watches each injected shell's `shell.qml` (and iNiR's
//! `.inir-manifest`), and runs `m3-settings inject reapply`. Our own writes
//! only happen when contents change, so the run it triggers is a no-op.

use super::{status, State, Target};
use material_3_ui::sys::cmd;
use std::path::{Path, PathBuf};

pub const UNIT: &str = "m3-shell-inject";

pub fn unit_dir(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && home == material_3_ui::sys::paths::home())
        .unwrap_or_else(|| home.join(".config"))
        .join("systemd/user")
}

/// Paths to watch: those of every shell we're in.
pub fn watched(home: &Path) -> Vec<PathBuf> {
    Target::ALL
        .into_iter()
        .filter(|t| matches!(status(home, *t).state, State::Applied { .. } | State::NeedsReapply))
        .flat_map(|t| t.watch_paths(home))
        .collect()
}

pub fn path_unit(paths: &[PathBuf]) -> String {
    let mut s = String::from(
        "# Written by Material 3 Settings: puts the Material 3 integration back\n\
         # into your shell after the shell updates. Remove it from Settings.\n\
         [Unit]\n\
         Description=Re-apply Material 3 changes after shell updates\n\n\
         [Path]\n",
    );
    for p in paths {
        s.push_str(&format!("PathChanged={}\n", p.display()));
    }
    s.push_str("TriggerLimitIntervalSec=10\nTriggerLimitBurst=5\n\n[Install]\nWantedBy=default.target\n");
    s
}

pub fn service_unit(exe: &Path) -> String {
    format!(
        "[Unit]\n\
         Description=Re-apply Material 3 changes after shell updates\n\n\
         [Service]\n\
         Type=oneshot\n\
         # Updaters write many files; let them finish.\n\
         ExecStartPre=/bin/sleep 3\n\
         ExecStart={} inject reapply\n",
        exe.display()
    )
}

/// Write the units for the current set of shells and (re)start the path
/// unit, or take them away when there's nothing left to watch.
pub fn sync(home: &Path) -> Result<(), String> {
    let dir = unit_dir(home);
    let path_file = dir.join(format!("{UNIT}.path"));
    let service_file = dir.join(format!("{UNIT}.service"));
    let paths = watched(home);
    let has_systemctl = cmd::exists("systemctl");
    if paths.is_empty() {
        if has_systemctl {
            let _ = cmd::run("systemctl", &["--user", "disable", "--now", &format!("{UNIT}.path")]);
        }
        let _ = std::fs::remove_file(&path_file);
        let _ = std::fs::remove_file(&service_file);
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    std::fs::write(&path_file, path_unit(&paths)).map_err(|e| e.to_string())?;
    std::fs::write(&service_file, service_unit(&exe)).map_err(|e| e.to_string())?;
    if has_systemctl {
        // Without a user session bus (a TTY, a container) the units are still
        // written and start at the next login; that's not worth failing over.
        let unit = format!("{UNIT}.path");
        let steps: [&[&str]; 3] = [&["--user", "daemon-reload"], &["--user", "enable", &unit], &["--user", "restart", &unit]];
        for args in steps {
            if let Err(e) = cmd::run("systemctl", args) {
                log::warn!("systemctl {}: {e}", args.join(" "));
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_name_every_path_and_the_binary() {
        let p = path_unit(&[PathBuf::from("/h/.config/quickshell/ii/shell.qml"), PathBuf::from("/h/.config/quickshell/inir/.inir-manifest")]);
        assert!(p.contains("PathChanged=/h/.config/quickshell/ii/shell.qml\n"));
        assert!(p.contains("PathChanged=/h/.config/quickshell/inir/.inir-manifest\n"));
        assert!(p.contains("WantedBy=default.target"));
        assert!(service_unit(Path::new("/usr/bin/m3-settings")).contains("ExecStart=/usr/bin/m3-settings inject reapply\n"));
    }
}
