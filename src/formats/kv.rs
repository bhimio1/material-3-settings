//! Line-based `key = value` / `key value` files: INI, kitty, ghostty, mpv,
//! foot, btop, pacman.conf, gitconfig, zathura (`set key value`), tmux
//! (`set -g key value`) and friends.
//!
//! Edits touch only the line for the key. Comments, ordering and blank
//! lines are preserved; a commented-out default (`#key = x`) is uncommented
//! in place rather than appending a duplicate.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sep {
    /// `key = value` (or `key=value`).
    Equals,
    /// `key value`.
    Space,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dialect {
    pub sep: Sep,
    /// Text before the key on every line, e.g. `set ` for zathura.
    pub prefix: &'static str,
    /// Whether `[section]` headers are meaningful.
    pub sections: bool,
    /// Write `key=value` without spaces around `=`.
    pub compact: bool,
}

impl Dialect {
    pub const INI: Dialect = Dialect {
        sep: Sep::Equals,
        prefix: "",
        sections: true,
        compact: false,
    };
    pub const INI_COMPACT: Dialect = Dialect {
        sep: Sep::Equals,
        prefix: "",
        sections: true,
        compact: true,
    };
    pub const EQUALS: Dialect = Dialect {
        sep: Sep::Equals,
        prefix: "",
        sections: false,
        compact: false,
    };
    pub const SPACE: Dialect = Dialect {
        sep: Sep::Space,
        prefix: "",
        sections: false,
        compact: false,
    };
}

/// Split `section.key` for sectioned dialects. Keys without a dot live
/// before the first section header.
fn split_key(dialect: &Dialect, key: &str) -> (Option<String>, String) {
    if dialect.sections {
        if let Some((s, k)) = key.split_once('.') {
            return (Some(s.to_string()), k.to_string());
        }
    }
    (None, key.to_string())
}

fn section_name(line: &str) -> Option<String> {
    let t = line.trim();
    if t.starts_with('[') && t.ends_with(']') && !t.starts_with("[[") {
        Some(t[1..t.len() - 1].trim().to_string())
    } else {
        None
    }
}

fn is_comment_char(c: char) -> bool {
    c == '#' || c == ';'
}

/// If `line` sets `key`, return (indent, value, commented).
fn match_line<'a>(dialect: &Dialect, line: &'a str, key: &str) -> Option<(&'a str, &'a str, bool)> {
    let indent_len = line.len() - line.trim_start().len();
    let indent = &line[..indent_len];
    let mut rest = &line[indent_len..];
    let mut commented = false;
    if rest.starts_with(is_comment_char) {
        commented = true;
        rest = rest.trim_start_matches(is_comment_char).trim_start();
    }
    rest = rest.strip_prefix(dialect.prefix)?;
    let rest = rest.strip_prefix(key)?;
    let value = match dialect.sep {
        Sep::Equals => rest.trim_start().strip_prefix('=')?,
        Sep::Space => {
            if !rest.starts_with([' ', '\t']) && !rest.is_empty() {
                return None;
            }
            rest
        }
    };
    Some((indent, value.trim(), commented))
}

/// Remove matching quotes around a value.
pub fn unquote(v: &str) -> &str {
    let v = v.trim();
    for q in ['"', '\''] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
            return &v[1..v.len() - 1];
        }
    }
    v
}

/// Index ranges `[start, end)` of lines belonging to `section` (None = the
/// preamble before any header). The header line itself is excluded.
fn section_ranges(dialect: &Dialect, lines: &[&str], section: Option<&str>) -> Vec<(usize, usize)> {
    if !dialect.sections {
        return vec![(0, lines.len())];
    }
    let mut ranges = Vec::new();
    let mut current: Option<String> = None;
    let mut start = 0;
    for (i, line) in lines.iter().enumerate() {
        if let Some(name) = section_name(line) {
            if current.as_deref() == section {
                ranges.push((start, i));
            }
            current = Some(name);
            start = i + 1;
        }
    }
    if current.as_deref() == section {
        ranges.push((start, lines.len()));
    }
    ranges
}

/// Current value of `key` (last active occurrence wins, like most of these
/// programs). Quotes are not stripped; see [`unquote`].
pub fn get(text: &str, dialect: &Dialect, key: &str) -> Option<String> {
    let (section, key) = split_key(dialect, key);
    let lines: Vec<&str> = text.lines().collect();
    let mut found = None;
    for (start, end) in section_ranges(dialect, &lines, section.as_deref()) {
        for line in &lines[start..end] {
            if let Some((_, value, false)) = match_line(dialect, line, &key) {
                found = Some(value.to_string());
            }
        }
    }
    found
}

/// Is the key present but commented out (and not active anywhere)?
pub fn is_commented(text: &str, dialect: &Dialect, key: &str) -> bool {
    if get(text, dialect, key).is_some() {
        return false;
    }
    let (section, key) = split_key(dialect, key);
    let lines: Vec<&str> = text.lines().collect();
    section_ranges(dialect, &lines, section.as_deref())
        .into_iter()
        .any(|(s, e)| lines[s..e].iter().any(|l| match_line(dialect, l, &key).is_some()))
}

fn format_line(dialect: &Dialect, indent: &str, key: &str, value: &str) -> String {
    match dialect.sep {
        Sep::Equals if dialect.compact => format!("{indent}{}{key}={value}", dialect.prefix),
        Sep::Equals => format!("{indent}{}{key} = {value}", dialect.prefix),
        Sep::Space => format!("{indent}{}{key} {value}", dialect.prefix),
    }
}

/// Keep `key=value` vs `key = value` as the existing line had it.
fn restyle(dialect: &Dialect, original: &str, indent: &str, key: &str, value: &str) -> String {
    if dialect.sep == Sep::Equals {
        let compact = !original.contains(" =") && !original.contains("= ");
        let d = Dialect { compact, ..*dialect };
        format_line(&d, indent, key, value)
    } else {
        // Preserve the column alignment used by kitty-style files.
        let trimmed = original.trim_start().trim_start_matches(is_comment_char).trim_start();
        let after_key = &trimmed[(dialect.prefix.len() + key.len()).min(trimmed.len())..];
        let gap = after_key.len() - after_key.trim_start().len();
        let gap = if gap == 0 { 1 } else { gap };
        format!("{indent}{}{key}{}{value}", dialect.prefix, " ".repeat(gap))
    }
}

/// Set `key` to `value` (already formatted/quoted by the caller).
pub fn set(text: &str, dialect: &Dialect, key: &str, value: &str) -> String {
    let (section, bare) = split_key(dialect, key);
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let trailing_newline = text.ends_with('\n') || text.is_empty();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let ranges = section_ranges(dialect, &refs, section.as_deref());

    let mut active = None;
    let mut commented = None;
    for &(start, end) in &ranges {
        for (i, line) in refs.iter().enumerate().take(end).skip(start) {
            if let Some((_, _, is_commented)) = match_line(dialect, line, &bare) {
                if is_commented {
                    // Prefer the first commented default.
                    commented.get_or_insert(i);
                } else {
                    active = Some(i);
                }
            }
        }
    }

    if let Some(i) = active.or(commented) {
        let (indent, _, _) = match_line(dialect, &lines[i], &bare).expect("matched above");
        let indent = indent.to_string();
        lines[i] = restyle(dialect, &lines[i], &indent, &bare, value);
    } else if let Some(&(start, end)) = ranges.last() {
        // Insert after the last non-blank line of the section.
        let mut at = end;
        while at > start && lines[at - 1].trim().is_empty() {
            at -= 1;
        }
        let indent = lines[start..at]
            .iter()
            .rev()
            .find(|l| !l.trim().is_empty() && section_name(l).is_none())
            .map(|l| l[..l.len() - l.trim_start().len()].to_string())
            .unwrap_or_default();
        lines.insert(at, format_line(dialect, &indent, &bare, value));
    } else {
        // Section doesn't exist yet: add it at the end.
        if lines.last().is_some_and(|l| !l.trim().is_empty()) {
            lines.push(String::new());
        }
        if let Some(s) = &section {
            lines.push(format!("[{s}]"));
        }
        lines.push(format_line(dialect, "", &bare, value));
    }
    let mut out = lines.join("\n");
    if trailing_newline {
        out.push('\n');
    }
    out
}

/// Comment out every active occurrence of `key`, falling back to the
/// program's default.
pub fn unset(text: &str, dialect: &Dialect, key: &str) -> String {
    let (section, bare) = split_key(dialect, key);
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let mut hits = Vec::new();
    for (start, end) in section_ranges(dialect, &refs, section.as_deref()) {
        for i in start..end {
            if let Some((_, _, false)) = match_line(dialect, refs[i], &bare) {
                hits.push(i);
            }
        }
    }
    for i in hits {
        let indent_len = lines[i].len() - lines[i].trim_start().len();
        lines[i].insert(indent_len, '#');
    }
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kitty_style() {
        let text = "# Fonts\nfont_family      JetBrains Mono\n# font_size 11.0\n";
        let d = Dialect::SPACE;
        assert_eq!(get(text, &d, "font_family").as_deref(), Some("JetBrains Mono"));
        assert_eq!(get(text, &d, "font_size"), None);
        assert!(is_commented(text, &d, "font_size"));
        // Prefix match must not confuse font_family with font.
        assert_eq!(get(text, &d, "font"), None);

        let t = set(text, &d, "font_size", "13.0");
        assert_eq!(t, "# Fonts\nfont_family      JetBrains Mono\nfont_size 13.0\n");
        let t = set(&t, &d, "font_family", "Inter");
        assert!(t.contains("font_family      Inter\n"));
        let t = set(&t, &d, "background_opacity", "0.9");
        assert!(t.ends_with("background_opacity 0.9\n"));
        let t = unset(&t, &d, "font_size");
        assert!(t.contains("#font_size 13.0"));
        assert_eq!(get(&t, &d, "font_size"), None);
    }

    #[test]
    fn ini_sections() {
        let text = "[main]\nfont=monospace:size=10\n\n[colors]\nalpha=1.0\n";
        let d = Dialect::INI;
        assert_eq!(get(text, &d, "colors.alpha").as_deref(), Some("1.0"));
        assert_eq!(get(text, &d, "main.alpha"), None);
        let t = set(text, &d, "colors.alpha", "0.9");
        // Compact style is preserved.
        assert!(t.contains("[colors]\nalpha=0.9\n"));
        let t = set(&t, &d, "main.pad", "8x8");
        assert_eq!(
            t,
            "[main]\nfont=monospace:size=10\npad = 8x8\n\n[colors]\nalpha=0.9\n"
        );
        let t = set(&t, &d, "cursor.style", "beam");
        assert!(t.ends_with("\n\n[cursor]\nstyle = beam\n"));
    }

    #[test]
    fn preamble_keys_and_prefix() {
        let d = Dialect {
            prefix: "set ",
            ..Dialect::SPACE
        };
        let text = "set recolor true\nmap q quit\n";
        assert_eq!(get(text, &d, "recolor").as_deref(), Some("true"));
        let t = set(text, &d, "recolor", "false");
        assert_eq!(t, "set recolor false\nmap q quit\n");
        let t = set("", &d, "selection-clipboard", "clipboard");
        assert_eq!(t, "set selection-clipboard clipboard\n");
    }

    #[test]
    fn pacman_conf_style() {
        let text = "[options]\n#Color\nParallelDownloads = 5\n\n#[multilib]\n#Include = /etc/pacman.d/mirrorlist\n";
        let d = Dialect::INI;
        let t = set(text, &d, "options.ParallelDownloads", "10");
        assert!(t.contains("ParallelDownloads = 10"));
        assert_eq!(get(text, &d, "options.Color"), None);
    }

    #[test]
    fn gitconfig_tabs() {
        let text = "[user]\n\tname = A\n";
        let t = set(text, &Dialect::INI, "user.email", "a@b.c");
        assert_eq!(t, "[user]\n\tname = A\n\temail = a@b.c\n");
    }

    #[test]
    fn unquoting() {
        assert_eq!(unquote("\"Default\""), "Default");
        assert_eq!(unquote("'x'"), "x");
        assert_eq!(unquote("plain"), "plain");
    }
}
