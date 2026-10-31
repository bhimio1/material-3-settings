//! Hooks: small, marked edits to an upstream shell file.
//!
//! A hook finds one line by a regex (its anchor) and puts a few lines of our
//! code before or after it, or in its place. Everything it adds sits between
//! marker comments, so it can be found and taken out again:
//!
//! ```text
//!     // m3:begin root
//!     M3Root {}
//!     // m3:end root
//! ```
//!
//! A `replace` hook keeps the lines it replaced on its begin marker
//! (`// m3:begin id orig="…"`, JSON-quoted), so stripping gives back the
//! upstream file byte for byte.

use regex::Regex;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Before,
    After,
    Replace,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Hook {
    /// Unique within a pack; used in the markers.
    pub id: String,
    /// The feature it belongs to. A feature is applied only when all its
    /// hooks find their anchor. `core` is required by every other feature.
    pub feature: String,
    /// Path inside the shell folder.
    pub file: String,
    /// Regex matched per line (multi-line mode). It must match exactly once.
    pub anchor: String,
    pub action: Action,
    /// The lines to add, as they should appear (indentation included).
    pub text: String,
    /// Skip the hook (and so its feature) when the file already matches
    /// this regex, e.g. upstream added the property the hook would add,
    /// which would be a QML error.
    #[serde(default)]
    pub unless: Option<String>,
    /// Shells (folder names) the hook is for; empty means every shell the
    /// pack is used for.
    #[serde(default)]
    pub only: Vec<String>,
}

impl Hook {
    pub fn applies_to(&self, shell: &str) -> bool {
        self.only.is_empty() || self.only.iter().any(|s| s == shell)
    }
}

#[derive(Debug, Deserialize)]
pub struct HookFile {
    #[serde(default, rename = "hook")]
    pub hooks: Vec<Hook>,
}

pub fn parse(text: &str) -> Result<Vec<Hook>, String> {
    let f: HookFile = toml::from_str(text).map_err(|e| e.to_string())?;
    let mut seen = std::collections::HashSet::new();
    for h in &f.hooks {
        if h.id.is_empty() || h.id.contains(char::is_whitespace) {
            return Err(format!("hook id {:?} is empty or has spaces", h.id));
        }
        if !seen.insert(h.id.clone()) {
            return Err(format!("hook id {:?} is used twice", h.id));
        }
        Regex::new(&format!("(?m){}", h.anchor)).map_err(|e| format!("hook {}: {e}", h.id))?;
        if let Some(u) = &h.unless {
            Regex::new(&format!("(?m){u}")).map_err(|e| format!("hook {}: {e}", h.id))?;
        }
    }
    Ok(f.hooks)
}

/// The line comment for a file, by extension.
pub fn comment_for(file: &str) -> &'static str {
    match file.rsplit('.').next() {
        Some("sh" | "py" | "fish" | "conf" | "toml") => "#",
        _ => "//",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Miss {
    NotFound,
    /// The `unless` regex matched: upstream has its own version now.
    AlreadyThere,
    /// The anchor matched this many times.
    Ambiguous(usize),
}

impl std::fmt::Display for Miss {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Miss::NotFound => write!(f, "the line it attaches to wasn't found"),
            Miss::AlreadyThere => write!(f, "the shell now has its own version of what it adds"),
            Miss::Ambiguous(n) => write!(f, "the line it attaches to appears {n} times"),
        }
    }
}

/// Byte offsets of line starts, plus `text.len()` at the end.
fn line_bounds(text: &str, start: usize, end: usize) -> (usize, usize) {
    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    // The match may end with the newline itself; stay on its line.
    let probe = if end > start && text.as_bytes()[end - 1] == b'\n' { end - 1 } else { end };
    let line_end = text[probe..].find('\n').map_or(text.len(), |i| probe + i + 1);
    (line_start, line_end)
}

fn indent_of(line: &str) -> &str {
    let n = line.len() - line.trim_start().len();
    &line[..n]
}

fn block(hook: &Hook, comment: &str, orig: Option<&str>) -> String {
    // Markers line up with the code they wrap.
    let indent = indent_of(hook.text.lines().find(|l| !l.trim().is_empty()).unwrap_or(""));
    let mut s = format!("{indent}{comment} m3:begin {}", hook.id);
    if let Some(o) = orig {
        s.push_str(" orig=");
        s.push_str(&serde_json::to_string(o).unwrap());
    }
    s.push('\n');
    for l in hook.text.trim_end_matches('\n').split('\n') {
        s.push_str(l);
        s.push('\n');
    }
    s.push_str(&format!("{indent}{comment} m3:end {}\n", hook.id));
    s
}

/// Apply one hook. `text` must not already contain this hook's markers
/// (strip first).
pub fn apply(text: &str, hook: &Hook) -> Result<String, Miss> {
    if let Some(u) = &hook.unless {
        if Regex::new(&format!("(?m){u}")).is_ok_and(|re| re.is_match(text)) {
            return Err(Miss::AlreadyThere);
        }
    }
    let re = Regex::new(&format!("(?m){}", hook.anchor)).map_err(|_| Miss::NotFound)?;
    let found: Vec<_> = re.find_iter(text).collect();
    let m = match found.len() {
        0 => return Err(Miss::NotFound),
        1 => found[0],
        n => return Err(Miss::Ambiguous(n)),
    };
    let (ls, le) = line_bounds(text, m.start(), m.end());
    let lines = &text[ls..le];
    let comment = comment_for(&hook.file);
    // The file's last line may lack a newline; give it one before appending.
    let lines_nl = if lines.ends_with('\n') { lines.to_string() } else { format!("{lines}\n") };
    let mut out = String::with_capacity(text.len() + hook.text.len() + 64);
    out.push_str(&text[..ls]);
    match hook.action {
        Action::Before => {
            out.push_str(&block(hook, comment, None));
            out.push_str(lines);
        }
        Action::After => {
            out.push_str(&lines_nl);
            out.push_str(&block(hook, comment, None));
        }
        Action::Replace => {
            out.push_str(&block(hook, comment, Some(lines)));
        }
    }
    out.push_str(&text[le..]);
    Ok(out)
}

/// Whether `text` holds any of our markers.
pub fn has_markers(text: &str, comment: &str) -> bool {
    let tag = format!("{comment} m3:begin ");
    text.lines().any(|l| l.trim_start().starts_with(&tag))
}

/// Remove every hook block, putting back what `replace` hooks took out.
/// Fails (and changes nothing) on a begin marker without its end.
pub fn strip(text: &str, comment: &str) -> Result<String, String> {
    let begin = format!("{comment} m3:begin ");
    let mut out = String::with_capacity(text.len());
    let mut lines = text.split_inclusive('\n');
    while let Some(line) = lines.next() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix(&begin) else {
            out.push_str(line);
            continue;
        };
        let (id, orig) = match rest.split_once(" orig=") {
            Some((id, o)) => (id.trim(), Some(serde_json::from_str::<String>(o).map_err(|e| format!("hook {id}: bad orig: {e}"))?)),
            None => (rest.trim(), None),
        };
        let end = format!("{comment} m3:end {id}");
        loop {
            match lines.next() {
                Some(l) if l.trim() == end => break,
                Some(_) => {}
                None => return Err(format!("hook {id} has no end marker")),
            }
        }
        if let Some(o) = orig {
            out.push_str(&o);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(action: Action, anchor: &str, text: &str) -> Hook {
        Hook {
            id: "t".into(),
            feature: "core".into(),
            file: "shell.qml".into(),
            anchor: anchor.into(),
            action,
            text: text.into(),
            unless: None,
            only: vec![],
        }
    }

    const SHELL: &str = "import QtQuick\n\nShellRoot {\n    id: root\n    ReloadPopup {}\n}\n";

    #[test]
    fn after_inserts_below_the_anchor_line() {
        let h = hook(Action::After, r"^\s*ReloadPopup \{\}", "    M3Root {}");
        let out = apply(SHELL, &h).unwrap();
        assert_eq!(
            out,
            "import QtQuick\n\nShellRoot {\n    id: root\n    ReloadPopup {}\n    // m3:begin t\n    M3Root {}\n    // m3:end t\n}\n"
        );
        assert_eq!(strip(&out, "//").unwrap(), SHELL);
    }

    #[test]
    fn before_inserts_above_the_anchor_line() {
        let h = hook(Action::Before, r"^ShellRoot \{", "import \"m3\"");
        let out = apply(SHELL, &h).unwrap();
        assert!(out.contains("// m3:begin t\nimport \"m3\"\n// m3:end t\nShellRoot {"));
        assert_eq!(strip(&out, "//").unwrap(), SHELL);
    }

    #[test]
    fn replace_keeps_the_original_for_strip() {
        let h = hook(Action::Replace, r"ReloadPopup \{\}", "    // gone");
        let out = apply(SHELL, &h).unwrap();
        assert!(!out.contains("    ReloadPopup {}\n"));
        assert!(out.contains("orig=\"    ReloadPopup {}\\n\""));
        assert_eq!(strip(&out, "//").unwrap(), SHELL);
    }

    #[test]
    fn misses_are_reported() {
        assert_eq!(apply(SHELL, &hook(Action::After, "Nope", "x")), Err(Miss::NotFound));
        assert_eq!(apply("a\na\n", &hook(Action::After, "^a$", "x")), Err(Miss::Ambiguous(2)));
    }

    #[test]
    fn last_line_without_newline() {
        let out = apply("a\nb", &hook(Action::After, "^b", "c")).unwrap();
        assert_eq!(out, "a\nb\n// m3:begin t\nc\n// m3:end t\n");
        // Strip gives back the newline-terminated form; that's the only
        // difference, and the shell doesn't care.
        assert_eq!(strip(&out, "//").unwrap(), "a\nb\n");
    }

    #[test]
    fn unless_skips_when_upstream_has_it() {
        let mut h = hook(Action::After, r"^\s*ReloadPopup \{\}", "    altAction: null");
        h.unless = Some(r"^\s*id: root".into());
        assert_eq!(apply(SHELL, &h), Err(Miss::AlreadyThere));
    }

    #[test]
    fn strip_refuses_unterminated_blocks() {
        assert!(strip("// m3:begin x\nfoo\n", "//").is_err());
    }

    #[test]
    fn shell_comments() {
        assert_eq!(comment_for("scripts/colors/switchwall.sh"), "#");
        assert_eq!(comment_for("shell.qml"), "//");
    }

    #[test]
    fn parse_rejects_duplicates_and_bad_regex() {
        let one = "[[hook]]\nid='a'\nfeature='core'\nfile='shell.qml'\nanchor='x'\naction='after'\ntext='y'\n";
        assert_eq!(parse(one).unwrap().len(), 1);
        assert!(parse(&format!("{one}{one}")).is_err());
        assert!(parse(&one.replace("anchor='x'", "anchor='('")).is_err());
    }
}
