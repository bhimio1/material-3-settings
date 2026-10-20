//! Arch Linux specifics: distro detection, the local pacman database, AUR
//! helpers, and building install commands.

use super::{cmd, term};
use std::collections::HashSet;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct OsRelease {
    pub id: String,
    pub id_like: String,
    pub pretty_name: String,
}

impl OsRelease {
    pub fn load() -> Self {
        let text = std::fs::read_to_string("/etc/os-release")
            .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
            .unwrap_or_default();
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Self {
        let mut os = OsRelease::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.trim().trim_matches('"').to_string();
            match k.trim() {
                "ID" => os.id = v,
                "ID_LIKE" => os.id_like = v,
                "PRETTY_NAME" => os.pretty_name = v,
                _ => {}
            }
        }
        os
    }

    pub fn is_cachyos(&self) -> bool {
        self.id == "cachyos"
    }

    /// Arch itself or a derivative (EndeavourOS, CachyOS, Manjaro, …).
    pub fn is_arch_based(&self) -> bool {
        self.id == "arch" || self.id_like.split_whitespace().any(|w| w == "arch")
    }
}

/// Names of every installed package, read straight from the pacman database
/// (`/var/lib/pacman/local/<name>-<version>-<rel>`), which is much faster
/// than spawning pacman.
pub fn installed_packages() -> HashSet<String> {
    installed_packages_in(Path::new("/var/lib/pacman/local"))
}

pub fn installed_packages_in(db: &Path) -> HashSet<String> {
    let Ok(entries) = std::fs::read_dir(db) else {
        return HashSet::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            // Strip "-pkgver-pkgrel".
            let mut parts = name.rsplitn(3, '-');
            let _rel = parts.next()?;
            let _ver = parts.next()?;
            parts.next().map(str::to_string)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AurHelper {
    #[default]
    Auto,
    Paru,
    Yay,
    None,
}

impl AurHelper {
    /// The helper binary that will actually be used, if any.
    pub fn resolve(self) -> Option<&'static str> {
        match self {
            AurHelper::Paru => Some("paru"),
            AurHelper::Yay => Some("yay"),
            AurHelper::None => None,
            AurHelper::Auto => ["paru", "yay"].into_iter().find(|h| cmd::exists(h)),
        }
    }
}

/// Command line that installs `packages`. Each package comes from the
/// configured repos when they have it (Arch's, or CachyOS's own repos, which
/// carry many AUR packages), and from the AUR helper otherwise. The check
/// runs in the terminal, so the list is always current. POSIX `sh` only, in
/// a subshell so `exit` doesn't close the terminal before it can be read.
pub fn install_command(packages: &[&str], helper: AurHelper) -> String {
    let list = packages
        .iter()
        .map(|p| term::shell_quote(p))
        .collect::<Vec<_>>()
        .join(" ");
    let aur_step = match helper.resolve() {
        Some(h) => format!("{h} -S --needed $aur"),
        None => "echo \"Not in your repos: $aur. Install paru or yay (Packages page) for AUR packages.\"; exit 1".into(),
    };
    format!(
        "(repo=; aur=; for p in {list}; do if pacman -Si \"$p\" >/dev/null 2>&1; then repo=\"$repo $p\"; else aur=\"$aur $p\"; fi; done; \
         if [ -n \"$repo\" ]; then sudo pacman -S --needed $repo || exit $?; fi; \
         if [ -n \"$aur\" ]; then {aur_step}; fi)"
    )
}

/// Full system upgrade, through the AUR helper when there is one.
pub fn upgrade_command(helper: AurHelper) -> String {
    match helper.resolve() {
        Some(h) => format!("{h} -Syu"),
        None => "sudo pacman -Syu".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_release_detection() {
        let arch = OsRelease::parse("NAME=\"Arch Linux\"\nID=arch\nPRETTY_NAME=\"Arch Linux\"\n");
        assert!(arch.is_arch_based());
        let cachy = OsRelease::parse("NAME=\"CachyOS Linux\"\nID=cachyos\nID_LIKE=arch\n");
        assert!(cachy.is_arch_based());
        assert!(cachy.is_cachyos());
        assert!(!arch.is_cachyos());
        let endeavour = OsRelease::parse("ID=endeavouros\nID_LIKE=arch\n");
        assert!(endeavour.is_arch_based());
        let debian = OsRelease::parse("ID=debian\n");
        assert!(!debian.is_arch_based());
    }

    #[test]
    fn local_db_names() {
        let dir = tempfile::tempdir().unwrap();
        for d in ["kitty-0.36.4-1", "lib32-mesa-24.2.1-2", "python-pip-24.0-1"] {
            std::fs::create_dir(dir.path().join(d)).unwrap();
        }
        std::fs::write(dir.path().join("ALPM_DB_VERSION"), "9").unwrap();
        let names = installed_packages_in(dir.path());
        assert!(names.contains("kitty"));
        assert!(names.contains("lib32-mesa"));
        assert!(names.contains("python-pip"));
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn install_commands() {
        let cmd = install_command(&["kitty", "it's"], AurHelper::Paru);
        assert!(cmd.contains("for p in kitty 'it'\\''s'; do"));
        assert!(cmd.contains("sudo pacman -S --needed $repo"));
        assert!(cmd.contains("paru -S --needed $aur"));
        assert!(install_command(&["x"], AurHelper::None).contains("exit 1"));
    }

    /// Runs the generated script with stub pacman/sudo/paru to check routing.
    #[test]
    fn install_command_routes_repo_and_aur_packages() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path();
        let log = bin.join("log");
        let stub = |name: &str, body: &str| {
            let p = bin.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        // "kitty" is in the repos, "paru-only" isn't.
        stub("pacman", &format!("if [ \"$1\" = -Si ]; then [ \"$2\" = kitty ]; exit $?; fi; echo \"pacman $*\" >> {}", log.display()));
        stub("sudo", "exec \"$@\"");
        stub("paru", &format!("echo \"paru $*\" >> {}", log.display()));
        let script = install_command(&["kitty", "paru-only"], AurHelper::Paru);
        let path = format!("{}:/usr/bin:/bin", bin.display());
        let status = std::process::Command::new("sh")
            .args(["-c", &script])
            .env("PATH", path)
            .status()
            .unwrap();
        assert!(status.success());
        let logged = std::fs::read_to_string(&log).unwrap();
        assert_eq!(logged, "pacman -S --needed kitty\nparu -S --needed paru-only\n");
    }
}
