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

/// Command line that installs `packages`. AUR packages need a helper;
/// official ones use pacman directly.
pub fn install_command(packages: &[&str], aur: bool, helper: AurHelper) -> Option<String> {
    let list = packages
        .iter()
        .map(|p| term::shell_quote(p))
        .collect::<Vec<_>>()
        .join(" ");
    if aur {
        helper.resolve().map(|h| format!("{h} -S --needed {list}"))
    } else {
        Some(format!("sudo pacman -S --needed {list}"))
    }
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
        assert_eq!(
            install_command(&["kitty", "foot"], false, AurHelper::None).unwrap(),
            "sudo pacman -S --needed kitty foot"
        );
        assert_eq!(
            install_command(&["x"], true, AurHelper::Paru).unwrap(),
            "paru -S --needed x"
        );
        assert!(install_command(&["x"], true, AurHelper::None).is_none());
    }
}
