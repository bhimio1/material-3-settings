//! JSON with comments (VS Code, waybar, fastfetch, end-4's config.json).
//!
//! Values are located with a small scanner that understands comments and
//! trailing commas, so a single setting can be replaced in place without
//! re-serializing (and losing the comments of) the whole file. Paths are
//! `/`-separated because VS Code keys contain dots (`editor.fontSize`);
//! array elements are addressed by index.

use serde_json::Value;
use std::collections::HashMap;

/// Remove comments and trailing commas so serde_json can parse the text.
pub fn strip(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' {
            let end = string_end(bytes, i);
            out.push_str(&text[i..end]);
            i = end;
        } else if c == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i < bytes.len() && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/')) {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else if c == b',' {
            // Drop the comma if only whitespace/comments precede a closer.
            let mut j = i + 1;
            loop {
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if bytes.get(j) == Some(&b'/') && bytes.get(j + 1) == Some(&b'/') {
                    while j < bytes.len() && bytes[j] != b'\n' {
                        j += 1;
                    }
                } else if bytes.get(j) == Some(&b'/') && bytes.get(j + 1) == Some(&b'*') {
                    j += 2;
                    while j < bytes.len() && !(bytes[j] == b'*' && bytes.get(j + 1) == Some(&b'/')) {
                        j += 1;
                    }
                    j = (j + 2).min(bytes.len());
                } else {
                    break;
                }
            }
            if !matches!(bytes.get(j), Some(b'}') | Some(b']')) {
                out.push(',');
            }
            i += 1;
        } else {
            // Copy one UTF-8 character.
            let ch_len = utf8_len(c);
            out.push_str(&text[i..(i + ch_len).min(text.len())]);
            i += ch_len;
        }
    }
    out
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Index just past the string starting at `start` (a `"`).
fn string_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

pub fn has_comments(text: &str) -> bool {
    strip(text).len() != text.len() && {
        // Trailing commas alone also change the length; check for comments.
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'"' => i = string_end(bytes, i),
                b'/' if matches!(bytes.get(i + 1), Some(b'/') | Some(b'*')) => return true,
                _ => i += 1,
            }
        }
        false
    }
}

pub fn parse(text: &str) -> Result<Value, String> {
    if text.trim().is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    serde_json::from_str(&strip(text)).map_err(|e| e.to_string())
}

fn lookup<'a>(mut value: &'a Value, path: &str) -> Option<&'a Value> {
    for part in path.split('/').filter(|p| !p.is_empty()) {
        value = match value {
            Value::Object(m) => m.get(part)?,
            Value::Array(a) => a.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(value)
}

pub fn get(text: &str, path: &str) -> Option<Value> {
    lookup(&parse(text).ok()?, path).cloned()
}

/// Where values and objects sit in the source text.
#[derive(Default, Debug)]
struct Layout {
    /// Full byte span of each value, by path.
    values: HashMap<String, (usize, usize)>,
    /// Byte offset of `{` and `}` for each object, by path.
    objects: HashMap<String, (usize, usize)>,
    /// Number of members per object path.
    members: HashMap<String, usize>,
}

enum Frame {
    Object { path: String, key: Option<String>, open: usize },
    Array { path: String, index: usize, open: usize },
}

fn join(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}/{child}")
    }
}

fn scan(text: &str) -> Layout {
    let bytes = text.as_bytes();
    let mut layout = Layout::default();
    let mut stack: Vec<Frame> = Vec::new();
    let mut i = 0;

    // Path for a value starting now, consuming the pending key.
    fn value_path(stack: &mut [Frame]) -> String {
        match stack.last_mut() {
            None => String::new(),
            Some(Frame::Object { path, key, .. }) => join(path, &key.take().unwrap_or_default()),
            Some(Frame::Array { path, index, .. }) => {
                let p = join(path, &index.to_string());
                *index += 1;
                p
            }
        }
    }

    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i < bytes.len() && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/')) {
                    i += 1;
                }
                i += 2;
            }
            b'"' => {
                let end = string_end(bytes, i);
                let expecting_key = matches!(stack.last(), Some(Frame::Object { key: None, .. }))
                    && {
                        // A key is followed by ':'.
                        let mut j = end;
                        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                            j += 1;
                        }
                        bytes.get(j) == Some(&b':')
                    };
                if expecting_key {
                    let raw = &text[i..end];
                    let key: String = serde_json::from_str(raw).unwrap_or_default();
                    if let Some(Frame::Object { key: k, path, .. }) = stack.last_mut() {
                        *layout.members.entry(path.clone()).or_default() += 1;
                        *k = Some(key);
                    }
                } else {
                    let p = value_path(&mut stack);
                    layout.values.insert(p, (i, end));
                }
                i = end;
            }
            b'{' => {
                let p = value_path(&mut stack);
                stack.push(Frame::Object {
                    path: p,
                    key: None,
                    open: i,
                });
                i += 1;
            }
            b'[' => {
                let p = value_path(&mut stack);
                stack.push(Frame::Array {
                    path: p,
                    index: 0,
                    open: i,
                });
                i += 1;
            }
            b'}' | b']' => {
                match stack.pop() {
                    Some(Frame::Object { path, open, .. }) => {
                        layout.objects.insert(path.clone(), (open, i));
                        layout.values.insert(path, (open, i + 1));
                    }
                    Some(Frame::Array { path, open, .. }) => {
                        layout.values.insert(path, (open, i + 1));
                    }
                    None => {}
                }
                i += 1;
            }
            b',' | b':' => i += 1,
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                // Number or literal.
                let start = i;
                while i < bytes.len()
                    && !matches!(bytes[i], b',' | b'}' | b']' | b'/')
                    && !bytes[i].is_ascii_whitespace()
                {
                    i += 1;
                }
                let p = value_path(&mut stack);
                layout.values.insert(p, (start, i));
            }
        }
    }
    layout
}

/// Indentation used by the line containing `pos`.
fn line_indent(text: &str, pos: usize) -> &str {
    let line_start = text[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line = &text[line_start..];
    &line[..line.len() - line.trim_start().len()]
}

fn detect_indent_unit(text: &str) -> String {
    text.lines()
        .map(|l| &l[..l.len() - l.trim_start().len()])
        .filter(|i| !i.is_empty())
        .min_by_key(|i| i.len())
        .map(str::to_string)
        .unwrap_or_else(|| "    ".into())
}

fn pretty(value: &Value, indent: &str) -> String {
    let mut buf = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    serde::Serialize::serialize(value, &mut ser).expect("serializing a Value cannot fail");
    String::from_utf8(buf).expect("serde_json writes UTF-8")
}

/// Re-indent a pretty-printed value so nested lines sit under `base`.
fn pretty_at(value: &Value, unit: &str, base: &str) -> String {
    pretty(value, unit).replace('\n', &format!("\n{base}"))
}

/// Set `path` to `value`, editing the text in place when possible.
pub fn set(text: &str, path: &str, value: &Value) -> Result<String, String> {
    let mut root = parse(text)?;
    let unit = detect_indent_unit(text);
    let layout = scan(text);

    if let Some(&(start, end)) = layout.values.get(path) {
        let base = line_indent(text, start).to_string();
        return Ok(format!(
            "{}{}{}",
            &text[..start],
            pretty_at(value, &unit, &base),
            &text[end..]
        ));
    }

    // Insert the missing member into the deepest existing parent object.
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    for depth in (0..parts.len()).rev() {
        let parent = parts[..depth].join("/");
        let Some(&(open, close)) = layout.objects.get(&parent) else {
            if lookup(&root, &parent).is_some() {
                break; // parent exists but isn't an object
            }
            continue;
        };
        // Build the nested value for the remaining path.
        let mut nested = value.clone();
        for part in parts[depth + 1..].iter().rev() {
            let mut m = serde_json::Map::new();
            m.insert(part.to_string(), nested);
            nested = Value::Object(m);
        }
        let base = format!("{}{unit}", line_indent(text, open));
        let key = serde_json::to_string(parts[depth]).unwrap_or_default();
        let member = format!("{key}: {}", pretty_at(&nested, &unit, &base));
        let empty = layout.members.get(&parent).copied().unwrap_or(0) == 0;
        let out = if empty {
            let closing_indent = line_indent(text, close).to_string();
            format!(
                "{}\n{base}{member}\n{closing_indent}{}",
                &text[..open + 1],
                &text[close..]
            )
        } else {
            format!("{}\n{base}{member},{}", &text[..open + 1], &text[open + 1..])
        };
        return Ok(out);
    }

    // Fallback: rewrite the whole document (comments are lost).
    set_value(&mut root, &parts, value.clone())?;
    Ok(pretty(&root, &unit) + "\n")
}

fn set_value(root: &mut Value, parts: &[&str], value: Value) -> Result<(), String> {
    let Some((last, parents)) = parts.split_last() else {
        *root = value;
        return Ok(());
    };
    let mut cur = root;
    for part in parents {
        if !cur.is_object() {
            return Err(format!("`{part}` is not an object"));
        }
        cur = cur
            .as_object_mut()
            .expect("checked")
            .entry(part.to_string())
            .or_insert_with(|| Value::Object(Default::default()));
    }
    match cur {
        Value::Object(m) => {
            m.insert(last.to_string(), value);
            Ok(())
        }
        Value::Array(a) => {
            let i: usize = last.parse().map_err(|_| "bad array index")?;
            if i < a.len() {
                a[i] = value;
                Ok(())
            } else {
                Err("array index out of range".into())
            }
        }
        _ => Err("parent is not an object".into()),
    }
}

/// Remove a member, keeping the rest of the text as it was.
pub fn unset(text: &str, path: &str) -> Result<String, String> {
    let layout = scan(text);
    let Some(&(start, end)) = layout.values.get(path) else {
        return Ok(text.to_string());
    };
    // Walk back over `"key":` and whitespace to the start of the member.
    let before = &text[..start];
    let colon = before.rfind(':').ok_or("not an object member")?;
    let key_end = before[..colon].trim_end().len();
    let key_start = before[..key_end - 1].rfind('"').ok_or("malformed key")?;
    let line_start = text[..key_start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let member_start = if text[line_start..key_start].trim().is_empty() {
        line_start
    } else {
        key_start
    };
    // Eat a following comma, or else a preceding one.
    let after = &text[end..];
    let trimmed = after.trim_start_matches([' ', '\t']);
    let (cut_end, cut_start) = if trimmed.starts_with(',') {
        let mut e = end + (after.len() - trimmed.len()) + 1;
        if text[e..].starts_with('\n') {
            e += 1;
        } else if text[e..].starts_with("\r\n") {
            e += 2;
        }
        (e, member_start)
    } else {
        let prev = text[..member_start].trim_end();
        let s = if prev.ends_with(',') { prev.len() - 1 } else { member_start };
        (end, s)
    };
    let out = format!("{}{}", &text[..cut_start], &text[cut_end..]);
    parse(&out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const VSCODE: &str = r#"{
    // Editor
    "editor.fontSize": 14,
    "editor.fontFamily": "'JetBrains Mono', monospace", /* inline */
    "workbench.colorTheme": "Default Dark Modern",
}
"#;

    #[test]
    fn strip_and_parse() {
        let v = parse(VSCODE).unwrap();
        assert_eq!(v["editor.fontSize"], json!(14));
        assert!(has_comments(VSCODE));
        assert!(!has_comments(r#"{"a": "http://x"}"#));
        assert_eq!(get(VSCODE, "editor.fontSize"), Some(json!(14)));
    }

    #[test]
    fn in_place_replace_keeps_comments() {
        let t = set(VSCODE, "editor.fontSize", &json!(16)).unwrap();
        assert!(t.contains("// Editor"));
        assert!(t.contains("/* inline */"));
        assert!(t.contains("\"editor.fontSize\": 16,"));
        assert_eq!(get(&t, "editor.fontSize"), Some(json!(16)));
    }

    #[test]
    fn insert_new_member() {
        let t = set(VSCODE, "editor.minimap.enabled", &json!(false)).unwrap();
        assert!(t.contains("// Editor"));
        assert_eq!(get(&t, "editor.minimap.enabled"), Some(json!(false)));
        let nested = set("{\n  \"a\": {}\n}\n", "a/b/c", &json!(1)).unwrap();
        assert_eq!(get(&nested, "a/b/c"), Some(json!(1)));
        assert_eq!(set("", "x", &json!(true)).unwrap().trim(), "{\n    \"x\": true\n}");
    }

    #[test]
    fn nested_and_arrays() {
        let text = "[\n  {\n    \"height\": 30, // px\n    \"modules-left\": [\"a\", \"b\"]\n  }\n]\n";
        assert_eq!(get(text, "0/height"), Some(json!(30)));
        let t = set(text, "0/height", &json!(34)).unwrap();
        assert!(t.contains("\"height\": 34, // px"));
        let t = set(&t, "0/modules-left/1", &json!("c")).unwrap();
        assert_eq!(get(&t, "0/modules-left"), Some(json!(["a", "c"])));
    }

    #[test]
    fn remove_member() {
        let t = unset(VSCODE, "editor.fontSize").unwrap();
        assert_eq!(get(&t, "editor.fontSize"), None);
        assert!(t.contains("// Editor"));
        let t = unset("{\"a\": 1, \"b\": 2}", "b").unwrap();
        assert_eq!(parse(&t).unwrap(), json!({"a": 1}));
    }

    #[test]
    fn strings_with_slashes_and_braces() {
        let text = r#"{"url": "https://x/{y}", "n": 1}"#;
        let t = set(text, "n", &json!(2)).unwrap();
        assert_eq!(parse(&t).unwrap(), json!({"url": "https://x/{y}", "n": 2}));
    }
}
