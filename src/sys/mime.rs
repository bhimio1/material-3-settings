//! Desktop entries and default applications (`xdg-mime`).

use super::cmd;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DesktopEntry {
    /// Desktop file ID, e.g. `firefox.desktop`.
    pub id: String,
    pub name: String,
    pub exec: String,
    pub icon: String,
    pub mime_types: Vec<String>,
    pub categories: Vec<String>,
    pub no_display: bool,
    pub hidden: bool,
    pub terminal: bool,
}

/// Parse the `[Desktop Entry]` group of a .desktop file. Localized keys
/// (`Name[de]`) and other groups (actions) are ignored.
pub fn parse_desktop(id: &str, text: &str) -> Option<DesktopEntry> {
    let mut entry = DesktopEntry {
        id: id.to_string(),
        ..Default::default()
    };
    let mut in_main = false;
    let mut is_app = true;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if !in_main {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        let list = || {
            value
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        match key {
            "Name" => entry.name = value.to_string(),
            "Exec" => entry.exec = value.to_string(),
            "Icon" => entry.icon = value.to_string(),
            "MimeType" => entry.mime_types = list(),
            "Categories" => entry.categories = list(),
            "NoDisplay" => entry.no_display = value == "true",
            "Hidden" => entry.hidden = value == "true",
            "Terminal" => entry.terminal = value == "true",
            "Type" => is_app = value == "Application",
            _ => {}
        }
    }
    (is_app && !entry.name.is_empty()).then_some(entry)
}

/// `applications` directories, highest priority first.
pub fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| super::paths::home().join(".local/share"));
    dirs.push(data_home.join("applications"));
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    for d in data_dirs.split(':').filter(|d| !d.is_empty()) {
        let p = Path::new(d).join("applications");
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    }
    dirs
}

fn collect(dir: &Path, prefix: &str, out: &mut BTreeMap<String, DesktopEntry>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            // Subdirectories map to IDs with '-' (kde4/foo.desktop → kde4-foo.desktop).
            collect(&path, &format!("{prefix}{name}-"), out);
        } else if name.ends_with(".desktop") {
            let id = format!("{prefix}{name}");
            if out.contains_key(&id) {
                continue; // a higher-priority dir already defined it
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Some(entry) = parse_desktop(&id, &text) {
                    out.insert(id, entry);
                }
            }
        }
    }
}

/// Every visible application, by desktop ID.
pub fn scan_in(dirs: &[PathBuf]) -> Vec<DesktopEntry> {
    let mut all = BTreeMap::new();
    for d in dirs {
        collect(d, "", &mut all);
    }
    let mut apps: Vec<DesktopEntry> = all
        .into_values()
        .filter(|e| !e.hidden)
        .collect();
    apps.sort_by_key(|e| e.name.to_lowercase());
    apps
}

pub fn scan() -> Vec<DesktopEntry> {
    scan_in(&application_dirs())
}

/// A kind of default application and the MIME types it covers.
pub struct Category {
    pub key: &'static str,
    pub label: &'static str,
    pub icon: egui_material_icons::MaterialIcon,
    pub types: &'static [&'static str],
}

use egui_material_icons::icons::*;

pub const CATEGORIES: &[Category] = &[
    Category {
        key: "browser",
        label: "Web browser",
        icon: ICON_LANGUAGE,
        types: &["x-scheme-handler/http", "x-scheme-handler/https", "text/html"],
    },
    Category {
        key: "mail",
        label: "Email",
        icon: ICON_MAIL,
        types: &["x-scheme-handler/mailto"],
    },
    Category {
        key: "files",
        label: "File manager",
        icon: ICON_FOLDER,
        types: &["inode/directory"],
    },
    Category {
        key: "text",
        label: "Text editor",
        icon: ICON_EDIT_NOTE,
        types: &["text/plain"],
    },
    Category {
        key: "image",
        label: "Image viewer",
        icon: ICON_IMAGE,
        types: &["image/png", "image/jpeg", "image/gif", "image/webp"],
    },
    Category {
        key: "video",
        label: "Video player",
        icon: ICON_MOVIE,
        types: &["video/mp4", "video/x-matroska", "video/webm"],
    },
    Category {
        key: "music",
        label: "Music player",
        icon: ICON_MUSIC_NOTE,
        types: &["audio/mpeg", "audio/flac", "audio/ogg"],
    },
    Category {
        key: "pdf",
        label: "PDF viewer",
        icon: ICON_PICTURE_AS_PDF,
        types: &["application/pdf"],
    },
    Category {
        key: "archive",
        label: "Archives",
        icon: ICON_FOLDER_ZIP,
        types: &["application/zip", "application/x-tar", "application/x-7z-compressed"],
    },
];

/// Apps declaring any of the category's types. Hidden-from-menu apps are
/// still offered when they handle the type (many viewers set NoDisplay).
pub fn candidates<'a>(apps: &'a [DesktopEntry], category: &Category) -> Vec<&'a DesktopEntry> {
    apps.iter()
        .filter(|a| a.mime_types.iter().any(|m| category.types.contains(&m.as_str())))
        .collect()
}

/// Terminal emulators, for xdg-terminal-exec.
pub fn terminals(apps: &[DesktopEntry]) -> Vec<&DesktopEntry> {
    apps.iter()
        .filter(|a| a.categories.iter().any(|c| c == "TerminalEmulator") && !a.no_display)
        .collect()
}

pub fn query_default(mime: &str) -> Option<String> {
    cmd::run("xdg-mime", &["query", "default", mime])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// The current default for each category (by its first type).
pub fn load_defaults() -> BTreeMap<&'static str, String> {
    let mut out = BTreeMap::new();
    for c in CATEGORIES {
        if let Some(id) = query_default(c.types[0]) {
            out.insert(c.key, id);
        }
    }
    if let Some(t) = default_terminal() {
        out.insert("terminal", t);
    }
    out
}

pub fn set_default(desktop_id: &str, types: &[&str]) -> Result<(), String> {
    let mut args = vec!["default", desktop_id];
    args.extend_from_slice(types);
    cmd::run("xdg-mime", &args).map(|_| ())
}

/// `~/.config/xdg-terminals.list`: one desktop ID per line, first wins.
pub fn terminals_list_path() -> PathBuf {
    super::paths::config_home().join("xdg-terminals.list")
}

pub fn parse_terminals_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

pub fn default_terminal() -> Option<String> {
    let text = std::fs::read_to_string(terminals_list_path()).ok()?;
    parse_terminals_list(&text).into_iter().next()
}

/// Put `id` first in xdg-terminals.list, keeping the other entries.
pub fn prefer_terminal_text(text: &str, id: &str) -> String {
    let mut lines: Vec<&str> = text
        .lines()
        .filter(|l| l.trim() != id)
        .collect();
    let first_entry = lines
        .iter()
        .position(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
        .unwrap_or(lines.len());
    lines.insert(first_entry, id);
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

pub fn set_default_terminal(id: &str) -> Result<(), String> {
    let path = terminals_list_path();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, prefer_terminal_text(&text, id)).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX: &str = "[Desktop Entry]\nVersion=1.0\nName=Firefox\nName[de]=Firefox-Browser\nExec=/usr/lib/firefox/firefox %u\nIcon=firefox\nType=Application\nMimeType=text/html;text/xml;x-scheme-handler/http;x-scheme-handler/https;\nCategories=Network;WebBrowser;\n\n[Desktop Action new-private-window]\nName=New Private Window\nExec=/usr/lib/firefox/firefox --private-window %u\n";

    #[test]
    fn desktop_parsing() {
        let e = parse_desktop("firefox.desktop", FIREFOX).unwrap();
        assert_eq!(e.name, "Firefox");
        assert_eq!(e.exec, "/usr/lib/firefox/firefox %u");
        assert!(e.mime_types.contains(&"x-scheme-handler/https".to_string()));
        assert_eq!(e.categories, ["Network", "WebBrowser"]);
        assert!(!e.no_display);
        assert!(parse_desktop("link.desktop", "[Desktop Entry]\nType=Link\nName=X\n").is_none());
    }

    #[test]
    fn scanning_dirs_and_priority() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        std::fs::write(a.path().join("firefox.desktop"), FIREFOX.replace("Name=Firefox\n", "Name=My Firefox\n")).unwrap();
        std::fs::write(b.path().join("firefox.desktop"), FIREFOX).unwrap();
        std::fs::create_dir(b.path().join("kde4")).unwrap();
        std::fs::write(
            b.path().join("kde4/okular.desktop"),
            "[Desktop Entry]\nType=Application\nName=Okular\nMimeType=application/pdf;\n",
        )
        .unwrap();
        std::fs::write(
            b.path().join("kitty.desktop"),
            "[Desktop Entry]\nType=Application\nName=kitty\nCategories=System;TerminalEmulator;\n",
        )
        .unwrap();
        let apps = scan_in(&[a.path().to_path_buf(), b.path().to_path_buf()]);
        assert_eq!(apps.len(), 3);
        assert_eq!(apps[0].name, "kitty");
        assert!(apps.iter().any(|e| e.name == "My Firefox"));
        assert!(apps.iter().any(|e| e.id == "kde4-okular.desktop"));
        let browser = &CATEGORIES[0];
        assert_eq!(candidates(&apps, browser).len(), 1);
        assert_eq!(terminals(&apps)[0].id, "kitty.desktop");
    }

    #[test]
    fn terminals_list() {
        let t = "# comment\nfoot.desktop\nkitty.desktop\n";
        assert_eq!(parse_terminals_list(t), ["foot.desktop", "kitty.desktop"]);
        assert_eq!(
            prefer_terminal_text(t, "kitty.desktop"),
            "# comment\nkitty.desktop\nfoot.desktop\n"
        );
        assert_eq!(prefer_terminal_text("", "foot.desktop"), "foot.desktop\n");
    }
}
