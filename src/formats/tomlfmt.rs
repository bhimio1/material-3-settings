//! TOML via `toml_edit`, which keeps comments and formatting intact.
//! Keys are dotted paths (`font.normal.family`).

use toml_edit::{DocumentMut, Item, Table, Value};

pub fn parse(text: &str) -> Result<DocumentMut, String> {
    text.parse::<DocumentMut>().map_err(|e| e.to_string())
}

fn lookup<'a>(doc: &'a DocumentMut, key: &str) -> Option<&'a Item> {
    let mut item = doc.as_item();
    for part in key.split('.') {
        item = item.get(part)?;
    }
    Some(item)
}

/// The value at `key` as a JSON value (strings unquoted).
pub fn get(text: &str, key: &str) -> Option<serde_json::Value> {
    let doc = parse(text).ok()?;
    let value = lookup(&doc, key)?.as_value()?;
    Some(match value {
        Value::String(s) => serde_json::Value::String(s.value().clone()),
        Value::Integer(i) => serde_json::json!(*i.value()),
        Value::Float(f) => serde_json::json!(*f.value()),
        Value::Boolean(b) => serde_json::Value::Bool(*b.value()),
        other => serde_json::Value::String(other.to_string().trim().to_string()),
    })
}

fn to_toml(value: &serde_json::Value) -> Value {
    match value {
        serde_json::Value::Bool(b) => Value::from(*b),
        serde_json::Value::Number(n) if n.is_i64() => Value::from(n.as_i64().unwrap_or(0)),
        serde_json::Value::Number(n) => Value::from(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Value::from(s.as_str()),
        other => Value::from(other.to_string()),
    }
}

pub fn set(text: &str, key: &str, value: &serde_json::Value) -> Result<String, String> {
    let mut doc = parse(text)?;
    let parts: Vec<&str> = key.split('.').collect();
    let (last, parents) = parts.split_last().ok_or("empty key")?;
    let mut table: &mut Table = doc.as_table_mut();
    for part in parents {
        let entry = table.entry(part).or_insert_with(|| {
            let mut t = Table::new();
            t.set_implicit(true);
            Item::Table(t)
        });
        // Inline tables are edited in place.
        if let Some(inline) = entry.as_inline_table_mut() {
            let mut converted = inline.clone().into_table();
            converted.set_implicit(false);
            *entry = Item::Table(converted);
        }
        table = entry
            .as_table_mut()
            .ok_or_else(|| format!("`{part}` is not a table"))?;
    }
    let mut new_value = to_toml(value);
    // Keep any trailing comment on the existing value.
    if let Some(old) = table.get(last).and_then(Item::as_value) {
        *new_value.decor_mut() = old.decor().clone();
    }
    table.insert(last, Item::Value(new_value));
    Ok(doc.to_string())
}

pub fn unset(text: &str, key: &str) -> Result<String, String> {
    let mut doc = parse(text)?;
    let parts: Vec<&str> = key.split('.').collect();
    let (last, parents) = parts.split_last().ok_or("empty key")?;
    let mut item = doc.as_item_mut();
    for part in parents {
        match item.get_mut(part) {
            Some(i) => item = i,
            None => return Ok(doc.to_string()),
        }
    }
    if let Some(t) = item.as_table_like_mut() {
        t.remove(last);
    }
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trip_keeps_comments() {
        let text = "# Alacritty\n[font]\nsize = 11.0 # points\n\n[window]\nopacity = 1.0\n";
        assert_eq!(get(text, "font.size"), Some(json!(11.0)));
        let t = set(text, "font.size", &json!(13.5)).unwrap();
        assert!(t.starts_with("# Alacritty\n"));
        assert!(t.contains("size = 13.5 # points"));
        let t = set(&t, "font.normal.family", &json!("JetBrains Mono")).unwrap();
        assert_eq!(get(&t, "font.normal.family"), Some(json!("JetBrains Mono")));
        let t = set(&t, "add_newline", &json!(false)).unwrap();
        assert_eq!(get(&t, "add_newline"), Some(json!(false)));
        let t = unset(&t, "window.opacity").unwrap();
        assert_eq!(get(&t, "window.opacity"), None);
        assert!(parse(&t).is_ok());
    }

    #[test]
    fn inline_tables() {
        let text = "font = { size = 10 }\n";
        let t = set(text, "font.size", &json!(12)).unwrap();
        assert_eq!(get(&t, "font.size"), Some(json!(12)));
    }
}
