//! The config text editor: a monospace `TextEdit` with line numbers and
//! lightweight syntax highlighting per [`Syntax`].
//!
//! Highlighting is line based and forgiving: it only has to make keys,
//! values and comments easy to tell apart, not parse anything. The layout
//! job is cached by a hash of the text so large files stay cheap to draw.

use crate::formats::Syntax;
use material_3_ui::theme::M3Palette;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontFamily, FontId, Stroke, Ui};
use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tok {
    Plain,
    Comment,
    Header,
    Key,
    Str,
    Number,
    Keyword,
}

/// Highlighted spans (byte ranges into `text`), in order, covering only
/// the non-plain parts.
pub fn highlight(text: &str, syntax: Syntax) -> Vec<(Range<usize>, Tok)> {
    let mut out = Vec::new();
    let mut in_block = false;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        for (r, t) in line_spans(body, syntax, &mut in_block) {
            out.push((r.start + offset..r.end + offset, t));
        }
        offset += line.len();
    }
    out
}

fn block_comments(syntax: Syntax) -> bool {
    matches!(syntax, Syntax::Json | Syntax::Css | Syntax::Plain)
}

fn line_comment_at(syntax: Syntax, line: &str, i: usize) -> bool {
    let rest = &line[i..];
    let prev_ws = i == 0 || line[..i].ends_with([' ', '\t']);
    match syntax {
        Syntax::Toml => rest.starts_with('#'),
        Syntax::Shell | Syntax::Yaml => rest.starts_with('#') && prev_ws,
        Syntax::Lua => rest.starts_with("--"),
        Syntax::Json | Syntax::Css | Syntax::Plain => rest.starts_with("//") && prev_ws,
        // `#` inside values is usually a color (`#1e1e2e`); only a `#`
        // between spaces counts as a trailing comment.
        Syntax::Hash => rest.starts_with("# ") && prev_ws && i > 0,
        Syntax::Ini => false,
    }
}

fn is_keyword(syntax: Syntax, word: &str) -> bool {
    const COMMON: &[&str] = &[
        "true", "false", "yes", "no", "on", "off", "True", "False", "null", "none", "None",
    ];
    const LUA: &[&str] = &[
        "local", "function", "end", "if", "then", "else", "elseif", "return", "require", "nil",
        "for", "in", "do", "while", "and", "or", "not",
    ];
    const SHELL: &[&str] = &[
        "if", "then", "else", "elif", "fi", "for", "do", "done", "while", "case", "esac",
        "function", "export", "alias", "set", "end", "source", "local", "return",
    ];
    COMMON.contains(&word)
        || (syntax == Syntax::Lua && LUA.contains(&word))
        || (syntax == Syntax::Shell && SHELL.contains(&word))
}

/// Byte range of the key at the start of `line`, if the syntax has keys.
fn key_range(line: &str, syntax: Syntax) -> Option<Range<usize>> {
    let start = line.len() - line.trim_start().len();
    let trimmed = &line[start..];
    if trimmed.is_empty() {
        return None;
    }
    let simple = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_alphanumeric() || "_-.:/@$ ".contains(c))
    };
    match syntax {
        Syntax::Ini | Syntax::Toml | Syntax::Hash => {
            if let Some(eq) = trimmed.find('=') {
                let k = trimmed[..eq].trim_end();
                if simple(k) && (syntax != Syntax::Hash || !k.contains(' ') || k.starts_with("set ")) {
                    return Some(start..start + k.len());
                }
            }
            if syntax == Syntax::Hash {
                // `key value` (kitty, sway, ssh, tmux).
                let end = trimmed
                    .find([' ', '\t'])
                    .unwrap_or(trimmed.len());
                let word = &trimmed[..end];
                if simple(word) && !word.is_empty() && trimmed.len() > end {
                    return Some(start..start + end);
                }
            }
            None
        }
        Syntax::Yaml => {
            let (s, t) = match trimmed.strip_prefix("- ") {
                Some(t) => (start + 2, t),
                None => (start, trimmed),
            };
            let colon = t.find(": ").or_else(|| t.ends_with(':').then(|| t.len() - 1))?;
            simple(&t[..colon]).then(|| s..s + colon)
        }
        Syntax::Css => {
            if trimmed.ends_with('{') {
                return None;
            }
            let colon = trimmed.find(':')?;
            let k = &trimmed[..colon];
            (simple(k) && !k.contains(' ')).then(|| start..start + colon)
        }
        Syntax::Lua | Syntax::Shell => {
            let eq = trimmed.find('=')?;
            if trimmed[eq + 1..].starts_with('=') {
                return None;
            }
            let k = trimmed[..eq].trim_end();
            let k = k.strip_prefix("local ").or(k.strip_prefix("export ")).unwrap_or(k);
            let k_start = trimmed[..eq].find(k).unwrap_or(0);
            (simple(k) && !k.contains(' ') && !k.contains(':'))
                .then(|| start + k_start..start + k_start + k.len())
        }
        Syntax::Json | Syntax::Plain => None,
    }
}

fn string_end(line: &str, start: usize, quote: char) -> usize {
    let mut escaped = false;
    for (i, c) in line[start + 1..].char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == quote {
            return start + 1 + i + 1;
        }
    }
    line.len()
}

fn line_spans(line: &str, syntax: Syntax, in_block: &mut bool) -> Vec<(Range<usize>, Tok)> {
    let mut spans = Vec::new();
    let mut i = 0;

    if *in_block {
        match line.find("*/") {
            Some(end) => {
                spans.push((0..end + 2, Tok::Comment));
                *in_block = false;
                i = end + 2;
            }
            None => return vec![(0..line.len(), Tok::Comment)],
        }
    }

    let trimmed = line.trim_start();
    let lead = line.len() - trimmed.len();
    if i == 0 {
        // Whole-line comments.
        let hash_comment = trimmed.starts_with('#')
            && matches!(syntax, Syntax::Hash | Syntax::Ini | Syntax::Toml | Syntax::Shell | Syntax::Yaml);
        let semi_comment = trimmed.starts_with(';') && syntax == Syntax::Ini;
        if hash_comment || semi_comment || (syntax == Syntax::Plain && trimmed.starts_with("<!--")) {
            return vec![(lead..line.len(), Tok::Comment)];
        }
        // Section headers.
        if matches!(syntax, Syntax::Ini | Syntax::Toml) && trimmed.starts_with('[') {
            let end = trimmed.find(']').map(|e| lead + e + 1).unwrap_or(line.len());
            spans.push((lead..end, Tok::Header));
            i = end;
        } else if matches!(syntax, Syntax::Css | Syntax::Hash) && trimmed.ends_with('{') {
            // CSS selectors, hyprlang categories (`general {`).
            let brace = lead + trimmed.len() - 1;
            let sel = line[..brace].trim_end().len();
            if sel > lead {
                spans.push((lead..sel, Tok::Header));
            }
            i = brace;
        } else if let Some(k) = key_range(line, syntax) {
            spans.push((k.clone(), Tok::Key));
            i = k.end;
        }
    }

    let bytes = line.as_bytes();
    while i < line.len() {
        if !line.is_char_boundary(i) {
            i += 1;
            continue;
        }
        if line_comment_at(syntax, line, i) {
            spans.push((i..line.len(), Tok::Comment));
            break;
        }
        if block_comments(syntax) && line[i..].starts_with("/*") {
            match line[i + 2..].find("*/") {
                Some(e) => {
                    let end = i + 2 + e + 2;
                    spans.push((i..end, Tok::Comment));
                    i = end;
                    continue;
                }
                None => {
                    spans.push((i..line.len(), Tok::Comment));
                    *in_block = true;
                    break;
                }
            }
        }
        let c = bytes[i];
        let quotes_ok = match syntax {
            Syntax::Hash | Syntax::Ini => c == b'"',
            _ => c == b'"' || c == b'\'',
        };
        if quotes_ok {
            let end = string_end(line, i, c as char);
            let is_key = syntax == Syntax::Json && line[end..].trim_start().starts_with(':');
            spans.push((i..end, if is_key { Tok::Key } else { Tok::Str }));
            i = end;
            continue;
        }
        let prev_word = i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        if !prev_word && (c.is_ascii_digit() || (c == b'-' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))) {
            let mut end = i + 1;
            while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
                end += 1;
            }
            // Only standalone numbers (not "x11" or "1080p").
            let next_word = end < bytes.len() && (bytes[end].is_ascii_alphabetic() || bytes[end] == b'_');
            if !next_word {
                spans.push((i..end, Tok::Number));
            }
            i = end;
            continue;
        }
        if !prev_word && (c.is_ascii_alphabetic() || c == b'_') {
            let mut end = i + 1;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            if is_keyword(syntax, &line[i..end]) {
                spans.push((i..end, Tok::Keyword));
            }
            i = end;
            continue;
        }
        i += 1;
    }
    spans
}

fn color(p: &M3Palette, tok: Tok) -> Color32 {
    match tok {
        Tok::Plain => p.on_surface,
        Tok::Comment => p.on_surface_variant.gamma_multiply(0.85),
        Tok::Header => p.primary,
        Tok::Key => p.primary,
        Tok::Str => p.tertiary,
        Tok::Number | Tok::Keyword => p.secondary,
    }
}

fn layout_job(text: &str, syntax: Syntax, font_size: f32, p: &M3Palette) -> LayoutJob {
    let mono = FontId::monospace(font_size);
    let fmt = |tok: Tok| {
        let mut f = TextFormat::simple(mono.clone(), color(p, tok));
        if tok == Tok::Comment {
            f.italics = true;
        }
        if tok == Tok::Header {
            f.underline = Stroke::NONE;
            f.background = p.primary.gamma_multiply(0.10);
        }
        f
    };
    let mut job = LayoutJob::default();
    let mut pos = 0;
    for (range, tok) in highlight(text, syntax) {
        if range.start < pos || range.end > text.len() {
            continue;
        }
        if range.start > pos {
            job.append(&text[pos..range.start], 0.0, fmt(Tok::Plain));
        }
        job.append(&text[range.clone()], 0.0, fmt(tok));
        pos = range.end;
    }
    if pos < text.len() {
        job.append(&text[pos..], 0.0, fmt(Tok::Plain));
    }
    job.wrap.max_width = f32::INFINITY;
    job
}

thread_local! {
    /// Most recent layout jobs, keyed by a hash of everything that affects them.
    static CACHE: RefCell<Vec<(u64, LayoutJob)>> = const { RefCell::new(Vec::new()) };
}

fn cached_job(text: &str, syntax: Syntax, font_size: f32, p: &M3Palette) -> LayoutJob {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    format!("{syntax:?}").hash(&mut h);
    font_size.to_bits().hash(&mut h);
    p.primary.hash(&mut h);
    p.on_surface.hash(&mut h);
    p.tertiary.hash(&mut h);
    let key = h.finish();
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some((_, job)) = c.iter().find(|(k, _)| *k == key) {
            return job.clone();
        }
        let job = layout_job(text, syntax, font_size, p);
        c.insert(0, (key, job.clone()));
        c.truncate(4);
        job
    })
}

/// A full-size code editor. Returns the text edit's output.
pub fn show(
    ui: &mut Ui,
    id: egui::Id,
    text: &mut String,
    syntax: Syntax,
    font_size: f32,
    p: &M3Palette,
) -> egui::text_edit::TextEditOutput {
    let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, _wrap: f32| {
        let job = cached_job(buf.as_str(), syntax, font_size, p);
        ui.fonts_mut(|f| f.layout_job(job))
    };
    let lines = text.lines().count().max(1) + usize::from(text.ends_with('\n'));
    let gutter: String = (1..=lines).map(|n| format!("{n}\n")).collect();
    let digits = lines.to_string().len().max(2);

    egui::ScrollArea::both()
        .id_salt(id.with("scroll"))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                // Line numbers, in the same font so rows line up.
                egui::Frame::new()
                    .inner_margin(egui::Margin { left: 10, right: 12, top: 1, bottom: 4 })
                    .show(ui, |ui| {
                        ui.set_min_width(font_size * 0.62 * digits as f32);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(gutter.trim_end_matches('\n'))
                                    .font(FontId::new(font_size, FontFamily::Monospace))
                                    .color(p.on_surface_variant.gamma_multiply(0.6)),
                            )
                            .halign(egui::Align::RIGHT)
                            .selectable(false),
                        );
                    });
                egui::TextEdit::multiline(text)
                    .id(id)
                    .code_editor()
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::symmetric(4, 4))
                    .desired_width(f32::INFINITY)
                    .desired_rows(24)
                    .lock_focus(true)
                    .layouter(&mut layouter)
                    .show(ui)
            })
            .inner
        })
        .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(text: &str, syntax: Syntax) -> Vec<(&str, Tok)> {
        highlight(text, syntax)
            .into_iter()
            .map(|(r, t)| (&text[r], t))
            .collect()
    }

    #[test]
    fn kitty_lines() {
        let t = toks("# comment\nfont_size 11.0\nbackground #1e1e2e\n", Syntax::Hash);
        assert_eq!(t[0], ("# comment", Tok::Comment));
        assert_eq!(t[1], ("font_size", Tok::Key));
        assert_eq!(t[2], ("11.0", Tok::Number));
        assert_eq!(t[3], ("background", Tok::Key));
        // Colors are not comments.
        assert!(!t.iter().any(|(s, k)| *k == Tok::Comment && s.contains("1e1e2e")));
        let t = toks("timeout = 300 # 5mins\n", Syntax::Hash);
        assert_eq!(t.last().unwrap(), &("# 5mins", Tok::Comment));
    }

    #[test]
    fn ini_and_toml() {
        let t = toks("[main]\nfont=monospace:size=10\n; note\n", Syntax::Ini);
        assert_eq!(t[0], ("[main]", Tok::Header));
        assert_eq!(t[1], ("font", Tok::Key));
        assert_eq!(t.last().unwrap(), &("; note", Tok::Comment));
        let t = toks("size = 11 # pts\nname = \"x # y\"\n", Syntax::Toml);
        assert!(t.contains(&("# pts", Tok::Comment)));
        assert!(t.contains(&("\"x # y\"", Tok::Str)));
    }

    #[test]
    fn json_keys_and_block_comments() {
        let text = "{\n  /* multi\n  line */ \"a\": 1, // tail\n  \"b\": \"https://x\"\n}";
        let t = toks(text, Syntax::Json);
        assert!(t.contains(&("\"a\"", Tok::Key)));
        assert!(t.contains(&("1", Tok::Number)));
        assert!(t.contains(&("// tail", Tok::Comment)));
        assert!(t.contains(&("\"https://x\"", Tok::Str)));
        assert!(t.iter().filter(|(_, k)| *k == Tok::Comment).count() >= 3);
    }

    #[test]
    fn lua_and_css() {
        let t = toks("local x = 1 -- hi\nvim.opt.number = true\n", Syntax::Lua);
        assert!(t.contains(&("-- hi", Tok::Comment)));
        assert!(t.contains(&("vim.opt.number", Tok::Key)));
        assert!(t.contains(&("true", Tok::Keyword)));
        let t = toks("window#waybar {\n  color: #fff;\n}\n", Syntax::Css);
        assert_eq!(t[0], ("window#waybar", Tok::Header));
        assert_eq!(t[1], ("color", Tok::Key));
    }

    #[test]
    fn unicode_does_not_panic() {
        for s in [Syntax::Hash, Syntax::Ini, Syntax::Toml, Syntax::Json, Syntax::Lua, Syntax::Yaml, Syntax::Css, Syntax::Shell, Syntax::Plain] {
            let _ = highlight("prompt = \"❯ \" # ✓\n日本語 = 1\n'ünï' : \"x\n/* é", s);
        }
    }
}
