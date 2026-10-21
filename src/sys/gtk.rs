//! GTK / desktop appearance: the `org.gnome.desktop.interface` GSettings
//! keys, mirrored into GTK 3/4 `settings.ini` so apps that don't read
//! GSettings (and Hyprland's cursor) agree.

use material_3_ui::sys::{cmd, paths};
use crate::formats::kv::{self, Dialect};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const SCHEMA: &str = "org.gnome.desktop.interface";

/// One appearance setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    GtkTheme,
    IconTheme,
    CursorTheme,
    CursorSize,
    Font,
    MonospaceFont,
    ColorScheme,
}

impl Field {
    pub const ALL: [Field; 7] = [
        Field::GtkTheme,
        Field::IconTheme,
        Field::CursorTheme,
        Field::CursorSize,
        Field::Font,
        Field::MonospaceFont,
        Field::ColorScheme,
    ];

    pub fn gsettings_key(self) -> &'static str {
        match self {
            Field::GtkTheme => "gtk-theme",
            Field::IconTheme => "icon-theme",
            Field::CursorTheme => "cursor-theme",
            Field::CursorSize => "cursor-size",
            Field::Font => "font-name",
            Field::MonospaceFont => "monospace-font-name",
            Field::ColorScheme => "color-scheme",
        }
    }

    /// The `[Settings]` key in GTK's settings.ini, if it has one.
    fn ini_key(self) -> Option<&'static str> {
        match self {
            Field::GtkTheme => Some("gtk-theme-name"),
            Field::IconTheme => Some("gtk-icon-theme-name"),
            Field::CursorTheme => Some("gtk-cursor-theme-name"),
            Field::CursorSize => Some("gtk-cursor-theme-size"),
            Field::Font => Some("gtk-font-name"),
            Field::ColorScheme => Some("gtk-application-prefer-dark-theme"),
            Field::MonospaceFont => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    pub gtk_theme: String,
    pub icon_theme: String,
    pub cursor_theme: String,
    pub cursor_size: i32,
    /// "Family Size", e.g. "Inter 11".
    pub font: String,
    pub monospace_font: String,
    /// `default`, `prefer-dark` or `prefer-light`.
    pub color_scheme: String,
    /// Read from GSettings (true) or only from settings.ini.
    pub from_gsettings: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            gtk_theme: "Adwaita".into(),
            icon_theme: "Adwaita".into(),
            cursor_theme: "Adwaita".into(),
            cursor_size: 24,
            font: "Cantarell 11".into(),
            monospace_font: "Monospace 11".into(),
            color_scheme: "default".into(),
            from_gsettings: false,
        }
    }
}

impl Appearance {
    pub fn get(&self, field: Field) -> String {
        match field {
            Field::GtkTheme => self.gtk_theme.clone(),
            Field::IconTheme => self.icon_theme.clone(),
            Field::CursorTheme => self.cursor_theme.clone(),
            Field::CursorSize => self.cursor_size.to_string(),
            Field::Font => self.font.clone(),
            Field::MonospaceFont => self.monospace_font.clone(),
            Field::ColorScheme => self.color_scheme.clone(),
        }
    }

    fn set(&mut self, field: Field, value: String) {
        match field {
            Field::GtkTheme => self.gtk_theme = value,
            Field::IconTheme => self.icon_theme = value,
            Field::CursorTheme => self.cursor_theme = value,
            Field::CursorSize => self.cursor_size = value.parse().unwrap_or(self.cursor_size),
            Field::Font => self.font = value,
            Field::MonospaceFont => self.monospace_font = value,
            Field::ColorScheme => self.color_scheme = value,
        }
    }
}

/// Parse a GVariant text value as printed by `gsettings get`:
/// `'Adwaita'`, `"it's"`, `uint32 24`, `24`.
pub fn parse_gvariant(s: &str) -> String {
    let s = s.trim();
    let s = s
        .strip_prefix("uint32 ")
        .or_else(|| s.strip_prefix("int32 "))
        .unwrap_or(s);
    for q in ['\'', '"'] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            let inner = &s[1..s.len() - 1];
            let mut out = String::with_capacity(inner.len());
            let mut chars = inner.chars();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                } else {
                    out.push(c);
                }
            }
            return out;
        }
    }
    s.to_string()
}

/// Quote a string as a GVariant text literal.
pub fn gvariant_string(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// Split "Inter Medium 11" into ("Inter Medium", 11.0).
pub fn split_font(name: &str) -> (String, f32) {
    let name = name.trim();
    if let Some((family, size)) = name.rsplit_once(' ') {
        if let Ok(size) = size.parse::<f32>() {
            return (family.trim().to_string(), size);
        }
    }
    (name.to_string(), 11.0)
}

pub fn join_font(family: &str, size: f32) -> String {
    let size = if size.fract() == 0.0 {
        format!("{}", size as i32)
    } else {
        format!("{size:.1}")
    };
    format!("{} {size}", family.trim())
}

fn ini_paths() -> [PathBuf; 2] {
    let c = paths::config_home();
    [c.join("gtk-3.0/settings.ini"), c.join("gtk-4.0/settings.ini")]
}

/// Values from a GTK settings.ini (fallback when gsettings is missing).
fn read_ini(text: &str, into: &mut Appearance) {
    for field in Field::ALL {
        let Some(key) = field.ini_key() else { continue };
        if let Some(v) = kv::get(text, &Dialect::INI, &format!("Settings.{key}")) {
            let v = kv::unquote(&v).to_string();
            let v = if field == Field::ColorScheme {
                match v.as_str() {
                    "1" | "true" => "prefer-dark".into(),
                    _ => "default".into(),
                }
            } else {
                v
            };
            into.set(field, v);
        }
    }
}

/// Read the current appearance. Blocking; call from a background task.
pub fn read() -> Appearance {
    let mut out = Appearance::default();
    if cmd::exists("gsettings") {
        let mut ok = false;
        for field in Field::ALL {
            if let Ok(v) = cmd::run("gsettings", &["get", SCHEMA, field.gsettings_key()]) {
                out.set(field, parse_gvariant(&v));
                ok = true;
            }
        }
        out.from_gsettings = ok;
        if ok {
            return out;
        }
    }
    if let Ok(text) = std::fs::read_to_string(&ini_paths()[0]) {
        read_ini(&text, &mut out);
    }
    out
}

/// Update one key in a settings.ini text.
pub fn ini_with(text: &str, field: Field, value: &str) -> Option<String> {
    let key = field.ini_key()?;
    let value = match field {
        Field::ColorScheme => {
            if value == "prefer-dark" {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        _ => value.to_string(),
    };
    let base = if text.trim().is_empty() {
        "[Settings]\n".to_string()
    } else {
        text.to_string()
    };
    Some(kv::set(&base, &Dialect::INI, &format!("Settings.{key}"), &value))
}

/// Apply one setting everywhere: GSettings, both settings.ini files, and the
/// live Hyprland cursor. Blocking; returns the first hard error.
pub fn apply(field: Field, value: &str, cursor: Option<(&str, i32)>) -> Result<(), String> {
    let mut errors = Vec::new();
    if cmd::exists("gsettings") {
        let v = match field {
            Field::CursorSize => value.to_string(),
            _ => gvariant_string(value),
        };
        if let Err(e) = cmd::run("gsettings", &["set", SCHEMA, field.gsettings_key(), &v]) {
            errors.push(e);
        }
    }
    for path in ini_paths() {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        if let Some(new) = ini_with(&text, field, value) {
            if new != text {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if let Err(e) = std::fs::write(&path, new) {
                    errors.push(format!("{}: {e}", path.display()));
                }
            }
        }
    }
    if let Some((theme, size)) = cursor {
        if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() && cmd::exists("hyprctl") {
            let _ = cmd::run("hyprctl", &["setcursor", theme, &size.to_string()]);
        }
    }
    match errors.into_iter().next() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn theme_roots(kind: &str) -> Vec<PathBuf> {
    let home = paths::home();
    let data = dirs::data_dir().unwrap_or_else(|| home.join(".local/share"));
    let mut roots = vec![
        PathBuf::from(format!("/usr/share/{kind}")),
        home.join(format!(".{kind}")),
        data.join(kind),
    ];
    if let Ok(dirs) = std::env::var("XDG_DATA_DIRS") {
        for d in dirs.split(':').filter(|d| !d.is_empty()) {
            let p = Path::new(d).join(kind);
            if !roots.contains(&p) {
                roots.push(p);
            }
        }
    }
    roots
}

fn subdirs(root: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(root)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default()
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// GTK themes: folders with a gtk-3.0 or gtk-4.0 subfolder.
pub fn gtk_themes_in(roots: &[PathBuf]) -> Vec<String> {
    let mut set: BTreeSet<String> = ["Adwaita".to_string(), "Adwaita-dark".to_string()].into();
    for root in roots {
        for d in subdirs(root) {
            if d.join("gtk-3.0").is_dir() || d.join("gtk-4.0").is_dir() {
                set.insert(name_of(&d));
            }
        }
    }
    set.into_iter().collect()
}

/// Icon themes: folders with an index.theme that aren't cursor-only.
pub fn icon_themes_in(roots: &[PathBuf]) -> Vec<String> {
    let mut set = BTreeSet::new();
    for root in roots {
        for d in subdirs(root) {
            if !d.join("index.theme").is_file() {
                continue;
            }
            let cursor_only = subdirs(&d).iter().all(|s| name_of(s) == "cursors");
            if !cursor_only && name_of(&d) != "default" {
                set.insert(name_of(&d));
            }
        }
    }
    set.into_iter().collect()
}

/// Cursor themes: folders with a `cursors` subfolder.
pub fn cursor_themes_in(roots: &[PathBuf]) -> Vec<String> {
    let mut set = BTreeSet::new();
    for root in roots {
        for d in subdirs(root) {
            if d.join("cursors").is_dir() && name_of(&d) != "default" {
                set.insert(name_of(&d));
            }
        }
    }
    set.into_iter().collect()
}

#[derive(Clone, Debug, Default)]
pub struct Choices {
    pub gtk_themes: Vec<String>,
    pub icon_themes: Vec<String>,
    pub cursor_themes: Vec<String>,
    pub fonts: Vec<String>,
}

/// Parse `fc-list : family`: one line per font, with comma-separated
/// localized aliases. Deduped and sorted case-insensitively.
pub fn parse_fc_families(out: &str) -> Vec<String> {
    let mut set = BTreeSet::new();
    for line in out.lines() {
        for name in line.split(',') {
            let name = name.trim().replace("\\-", "-");
            if !name.is_empty() && !name.starts_with('.') {
                set.insert(name);
            }
        }
    }
    let mut v: Vec<String> = set.into_iter().collect();
    v.sort_by_key(|s| s.to_lowercase());
    v
}

/// Everything selectable. Blocking.
pub fn choices() -> Choices {
    let fonts = cmd::run("fc-list", &[":", "family"])
        .map(|o| parse_fc_families(&o))
        .unwrap_or_default();
    Choices {
        gtk_themes: gtk_themes_in(&theme_roots("themes")),
        icon_themes: icon_themes_in(&theme_roots("icons")),
        cursor_themes: cursor_themes_in(&theme_roots("icons")),
        fonts,
    }
}

/// The font file fontconfig picks for a family (for previews).
pub fn font_file(family: &str) -> Option<(PathBuf, u32)> {
    let out = cmd::run("fc-match", &["-f", "%{file}\n%{index}", family]).ok()?;
    let mut lines = out.lines();
    let file = PathBuf::from(lines.next()?.trim());
    let index = lines.next().and_then(|i| i.trim().parse().ok()).unwrap_or(0);
    let ext = file
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc")
        .then_some((file, index))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gvariant_values() {
        assert_eq!(parse_gvariant("'Adwaita'\n"), "Adwaita");
        assert_eq!(parse_gvariant("'it\\'s'"), "it's");
        assert_eq!(parse_gvariant("\"Papirus-Dark\""), "Papirus-Dark");
        assert_eq!(parse_gvariant("uint32 24"), "24");
        assert_eq!(parse_gvariant("24"), "24");
        assert_eq!(gvariant_string("it's"), "'it\\'s'");
    }

    #[test]
    fn fonts() {
        assert_eq!(split_font("Inter Medium 11"), ("Inter Medium".into(), 11.0));
        assert_eq!(split_font("Cantarell"), ("Cantarell".into(), 11.0));
        assert_eq!(join_font("Inter", 10.0), "Inter 10");
        assert_eq!(join_font("Inter", 10.5), "Inter 10.5");
        let fc = "Noto Sans,Noto Sans Display\nInter\nJetBrains Mono\nInter\nDejaVu Sans Mono\n";
        assert_eq!(
            parse_fc_families(fc),
            vec!["DejaVu Sans Mono", "Inter", "JetBrains Mono", "Noto Sans", "Noto Sans Display"]
        );
    }

    #[test]
    fn settings_ini_mirroring() {
        let t = ini_with("", Field::GtkTheme, "adw-gtk3-dark").unwrap();
        assert_eq!(t, "[Settings]\ngtk-theme-name = adw-gtk3-dark\n");
        let t = ini_with(&t, Field::ColorScheme, "prefer-dark").unwrap();
        assert!(t.contains("gtk-application-prefer-dark-theme = 1"));
        let t = ini_with(&t, Field::GtkTheme, "Adwaita").unwrap();
        assert!(t.contains("gtk-theme-name = Adwaita") && !t.contains("adw-gtk3"));
        assert!(ini_with(&t, Field::MonospaceFont, "x").is_none());
        let mut a = Appearance::default();
        read_ini(&t, &mut a);
        assert_eq!(a.gtk_theme, "Adwaita");
        assert_eq!(a.color_scheme, "prefer-dark");
    }

    #[test]
    fn theme_scanning() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        for d in [
            "themes/Nordic/gtk-3.0",
            "themes/Broken",
            "icons/Papirus/48x48",
            "icons/Bibata/cursors",
            "icons/Breeze/cursors",
            "icons/Breeze/actions",
        ] {
            std::fs::create_dir_all(r.join(d)).unwrap();
        }
        for t in ["icons/Papirus", "icons/Bibata", "icons/Breeze"] {
            std::fs::write(r.join(t).join("index.theme"), "[Icon Theme]\n").unwrap();
        }
        let themes = gtk_themes_in(&[r.join("themes")]);
        assert!(themes.contains(&"Nordic".to_string()));
        assert!(!themes.contains(&"Broken".to_string()));
        assert_eq!(icon_themes_in(&[r.join("icons")]), vec!["Breeze", "Papirus"]);
        assert_eq!(cursor_themes_in(&[r.join("icons")]), vec!["Bibata", "Breeze"]);
    }
}
