//! Reading and writing individual settings inside config files, without
//! disturbing the rest of the file.

pub mod jsonc;
pub mod kv;
pub mod tomlfmt;

use serde_json::Value;

/// How a config file is structured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Kv(kv::Dialect),
    Toml,
    Json,
    /// Edited as text only (Lua, YAML, rasi, shell scripts…).
    Text(Syntax),
}

/// Syntax highlighting flavour for the text editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syntax {
    Hash,
    Ini,
    Toml,
    Json,
    Lua,
    Yaml,
    Css,
    Shell,
    Plain,
}

impl Format {
    pub fn syntax(self) -> Syntax {
        match self {
            Format::Kv(d) if d.sections => Syntax::Ini,
            Format::Kv(_) => Syntax::Hash,
            Format::Toml => Syntax::Toml,
            Format::Json => Syntax::Json,
            Format::Text(s) => s,
        }
    }

    /// Whether individual keys can be read and written.
    pub fn structured(self) -> bool {
        !matches!(self, Format::Text(_))
    }

    /// Check that `text` parses, for formats with a real grammar.
    pub fn validate(self, text: &str) -> Result<(), String> {
        match self {
            Format::Toml => tomlfmt::parse(text).map(|_| ()),
            Format::Json => jsonc::parse(text).map(|_| ()),
            _ => Ok(()),
        }
    }
}

/// How booleans are spelled in a key/value file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoolStyle {
    TrueFalse,
    YesNo,
    /// btop: `True` / `False`.
    Capitalized,
    /// `1` / `0`.
    OneZero,
    /// A bare flag line (`Color` in pacman.conf, MangoHud toggles):
    /// present = on, commented out = off.
    Flag,
}

/// Read a setting as a JSON value. Key/value files give strings, with
/// surrounding quotes removed.
pub fn get(format: Format, text: &str, key: &str) -> Option<Value> {
    match format {
        Format::Kv(d) => kv::get(text, &d, key).map(|v| Value::String(kv::unquote(&v).to_string())),
        Format::Toml => tomlfmt::get(text, key),
        Format::Json => jsonc::get(text, key),
        Format::Text(_) => None,
    }
}

pub fn set(format: Format, text: &str, key: &str, value: &Value) -> Result<String, String> {
    match format {
        Format::Kv(d) => {
            let raw = match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            Ok(kv::set(text, &d, key, &raw))
        }
        Format::Toml => tomlfmt::set(text, key, value),
        Format::Json => jsonc::set(text, key, value),
        Format::Text(_) => Err("this file can only be edited as text".into()),
    }
}

pub fn unset(format: Format, text: &str, key: &str) -> Result<String, String> {
    match format {
        Format::Kv(d) => Ok(kv::unset(text, &d, key)),
        Format::Toml => tomlfmt::unset(text, key),
        Format::Json => jsonc::unset(text, key),
        Format::Text(_) => Err("this file can only be edited as text".into()),
    }
}

/// A bare flag line such as `Color` in pacman.conf: on when present and
/// not commented out.
pub fn get_flag(text: &str, dialect: &kv::Dialect, key: &str) -> bool {
    let flag = kv::Dialect {
        sep: kv::Sep::Space,
        ..*dialect
    };
    kv::get(text, &flag, key).is_some_and(|v| v.is_empty())
}

pub fn set_flag(text: &str, dialect: &kv::Dialect, key: &str, on: bool) -> String {
    let flag = kv::Dialect {
        sep: kv::Sep::Space,
        ..*dialect
    };
    if on {
        let bare = if dialect.sections {
            key.split_once('.').map_or(key, |(_, k)| k)
        } else {
            key
        };
        let set = kv::set(text, &flag, key, "");
        let mut out: Vec<&str> = set
            .lines()
            .map(|l| if l.trim() == bare { l.trim_end() } else { l })
            .collect();
        if set.ends_with('\n') {
            out.push("");
        }
        out.join("\n")
    } else {
        kv::unset(text, &flag, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags() {
        let text = "[options]\n#Color\nCheckSpace\n";
        let d = kv::Dialect::INI;
        assert!(!get_flag(text, &d, "options.Color"));
        assert!(get_flag(text, &d, "options.CheckSpace"));
        let t = set_flag(text, &d, "options.Color", true);
        assert_eq!(t, "[options]\nColor\nCheckSpace\n");
        let t = set_flag(&t, &d, "options.CheckSpace", false);
        assert_eq!(t, "[options]\nColor\n#CheckSpace\n");
    }
}
