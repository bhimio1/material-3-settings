//! Controls for quick settings: reading the current value from a config
//! file and turning the user's input back into the file's notation.

use crate::catalog::{Kind, Quick};
use crate::files::ConfigFile;
use crate::formats::{self, BoolStyle, Format};
use material_3_ui::theme::palette::{parse_hex, to_hex};
use material_3_ui::widgets;
use egui::{Color32, RichText, Ui};
use material_3_ui::icons::icons::*;
use serde_json::Value;
use std::collections::HashMap;

/// What the user did to a setting this frame.
pub enum Change {
    Set(Value),
    Unset,
    Flag(bool),
}

pub struct Outcome {
    pub change: Option<Change>,
    /// The control is mid-drag: apply now, save on release.
    pub dragging: bool,
}

fn structured(format: Format) -> bool {
    matches!(format, Format::Toml | Format::Json)
}

/// The current value as text (quotes stripped), if set.
fn current_text(file: &ConfigFile, quick: &Quick) -> Option<String> {
    file.get(quick.key).map(|v| match v {
        Value::String(s) => formats::kv::unquote(&s).to_string(),
        other => other.to_string(),
    })
}

fn parse_bool(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "yes" | "true" | "1" | "on")
}

fn bool_value(format: Format, style: BoolStyle, on: bool) -> Value {
    if structured(format) {
        return Value::Bool(on);
    }
    let s = match (style, on) {
        (BoolStyle::YesNo, true) => "yes",
        (BoolStyle::YesNo, false) => "no",
        (BoolStyle::Capitalized, true) => "True",
        (BoolStyle::Capitalized, false) => "False",
        (BoolStyle::OneZero, true) => "1",
        (BoolStyle::OneZero, false) => "0",
        (_, true) => "true",
        (_, false) => "false",
    };
    Value::String(s.into())
}

/// A text value in the file's notation (quoted when the program wants it).
pub fn text_value(format: Format, quick: &Quick, text: &str) -> Value {
    if quick.quoted && !structured(format) {
        Value::String(format!("\"{}\"", text.replace('"', "\\\"")))
    } else {
        Value::String(text.to_string())
    }
}

fn number_value(format: Format, n: f64, step: f64, integer: bool) -> Value {
    if integer {
        let i = n.round() as i64;
        return if structured(format) {
            serde_json::json!(i)
        } else {
            Value::String(i.to_string())
        };
    }
    let decimals = if step >= 1.0 {
        1
    } else {
        (-step.log10()).ceil().max(1.0) as usize
    };
    let scale = 10f64.powi(decimals as i32);
    let rounded = (n * scale).round() / scale;
    let text = format!("{rounded:.decimals$}");
    if structured(format) {
        serde_json::from_str(&text).unwrap_or(serde_json::json!(rounded))
    } else {
        Value::String(text)
    }
}

fn reset_button(ui: &mut Ui, set: bool) -> bool {
    if set {
        widgets::icon_button_sized(ui, ICON_RESTART_ALT, "Reset to the program's default", true, 30.0, false)
            .clicked()
    } else {
        ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
        false
    }
}

fn default_hint(ui: &mut Ui) {
    let p = widgets::palette(ui);
    ui.label(RichText::new("default").size(12.0).italics().color(p.on_surface_variant));
}

/// Draw the control for one quick setting (inside a right-to-left row).
pub fn control(
    ui: &mut Ui,
    quick: &Quick,
    file: &ConfigFile,
    fonts: Option<&[String]>,
    font_filter: &mut String,
    text_bufs: &mut HashMap<String, String>,
) -> Outcome {
    let p = widgets::palette(ui);
    let format = file.format;
    let mut out = Outcome {
        change: None,
        dragging: false,
    };
    let buf_key = format!("{}\0{}", file.path.display(), quick.key);

    match quick.kind {
        Kind::Bool(BoolStyle::Flag) => {
            let Format::Kv(d) = format else {
                return out;
            };
            let mut on = formats::get_flag(&file.text, &d, quick.key);
            ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
            if widgets::switch(ui, &mut on).changed() {
                out.change = Some(Change::Flag(on));
            }
        }
        Kind::Bool(style) => {
            let current = file.get(quick.key);
            let set = current.is_some();
            if reset_button(ui, set) {
                out.change = Some(Change::Unset);
            }
            let mut on = match &current {
                Some(Value::Bool(b)) => *b,
                Some(Value::String(s)) => parse_bool(formats::kv::unquote(s)),
                Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) != 0.0,
                _ => false,
            };
            if widgets::switch(ui, &mut on).changed() {
                out.change = Some(Change::Set(bool_value(format, style, on)));
            }
            if !set {
                default_hint(ui);
            }
        }
        Kind::Int { min, max } => {
            let current = current_text(file, quick).and_then(|s| s.trim().parse::<f64>().ok());
            if reset_button(ui, current.is_some()) {
                out.change = Some(Change::Unset);
            }
            let mut v = current.unwrap_or(min.max(0) as f64).round() as i64;
            let r = if max - min <= 200 {
                ui.add(egui::Slider::new(&mut v, min..=max))
            } else {
                ui.add(egui::DragValue::new(&mut v).range(min..=max).speed(((max - min) as f64 / 500.0).max(1.0)))
            };
            if r.changed() {
                out.change = Some(Change::Set(number_value(format, v as f64, 1.0, true)));
                out.dragging = r.dragged();
            }
            if current.is_none() {
                default_hint(ui);
            }
        }
        Kind::Float { min, max, step } => {
            let current = current_text(file, quick).and_then(|s| s.trim().parse::<f64>().ok());
            if reset_button(ui, current.is_some()) {
                out.change = Some(Change::Unset);
            }
            let mut v = current.unwrap_or(min.max(0.0));
            let decimals = if step >= 1.0 { 1 } else { (-step.log10()).ceil().max(1.0) as usize };
            let r = ui.add(egui::Slider::new(&mut v, min..=max).max_decimals(decimals).min_decimals(1));
            if r.changed() {
                out.change = Some(Change::Set(number_value(format, v, step, false)));
                out.dragging = r.dragged();
            }
            if current.is_none() {
                default_hint(ui);
            }
        }
        Kind::Text => {
            let current = current_text(file, quick);
            if reset_button(ui, current.is_some()) {
                out.change = Some(Change::Unset);
                text_bufs.remove(&buf_key);
            }
            let buf = text_bufs
                .entry(buf_key.clone())
                .or_insert_with(|| current.clone().unwrap_or_default());
            let r = ui.add(
                egui::TextEdit::singleline(buf)
                    .hint_text(RichText::new("default").italics())
                    .desired_width(240.0),
            );
            if r.lost_focus() {
                let text = buf.clone();
                text_bufs.remove(&buf_key);
                if Some(&text) != current.as_ref() {
                    out.change = Some(if text.is_empty() && current.is_some() {
                        Change::Unset
                    } else {
                        Change::Set(text_value(format, quick, &text))
                    });
                }
            } else if !r.has_focus() {
                // Follow edits made elsewhere (editor tab, other programs).
                *buf = current.unwrap_or_default();
            }
        }
        Kind::Choice(options) => {
            let current = current_text(file, quick);
            if reset_button(ui, current.is_some()) {
                out.change = Some(Change::Unset);
            }
            let shown = current.clone().unwrap_or_else(|| "default".into());
            egui::ComboBox::from_id_salt(&buf_key)
                .width(200.0)
                .selected_text(if current.is_some() {
                    RichText::new(shown)
                } else {
                    RichText::new(shown).italics().color(p.on_surface_variant)
                })
                .show_ui(ui, |ui| {
                    let mut all: Vec<&str> = options.to_vec();
                    if let Some(c) = current.as_deref() {
                        if !all.contains(&c) {
                            all.insert(0, c);
                        }
                    }
                    for opt in all {
                        if ui.selectable_label(current.as_deref() == Some(opt), opt).clicked()
                            && current.as_deref() != Some(opt)
                        {
                            out.change = Some(Change::Set(text_value(format, quick, opt)));
                        }
                    }
                });
        }
        Kind::Color | Kind::ColorBare => {
            let current = current_text(file, quick);
            if reset_button(ui, current.is_some()) {
                out.change = Some(Change::Unset);
                text_bufs.remove(&buf_key);
            }
            // Keep whichever notation the file already uses.
            let hash = match &current {
                Some(c) => c.starts_with('#'),
                None => quick.kind == Kind::Color,
            };
            let parsed = current.as_deref().and_then(parse_hex);
            let mut color = parsed.unwrap_or(Color32::GRAY);
            let to_text = |c: Color32| {
                let hex = to_hex(c).to_lowercase();
                if hash {
                    hex
                } else {
                    hex.trim_start_matches('#').to_string()
                }
            };
            let buf = text_bufs
                .entry(buf_key.clone())
                .or_insert_with(|| current.clone().unwrap_or_default());
            let r = ui.add(
                egui::TextEdit::singleline(buf)
                    .hint_text(RichText::new("default").italics())
                    .desired_width(90.0)
                    .font(egui::TextStyle::Monospace),
            );
            if r.lost_focus() {
                let text = buf.trim().to_string();
                text_bufs.remove(&buf_key);
                if text.is_empty() && current.is_some() {
                    out.change = Some(Change::Unset);
                } else if parse_hex(&text).is_some() && Some(&text) != current.as_ref() {
                    out.change = Some(Change::Set(text_value(format, quick, &text)));
                }
            } else if !r.has_focus() {
                *buf = current.clone().unwrap_or_default();
            }
            if ui.color_edit_button_srgba(&mut color).changed() {
                // Keep an alpha suffix the value already had (rrggbbaa).
                let mut text = to_text(color);
                if let Some(c) = &current {
                    let digits = c.trim_start_matches('#');
                    if digits.len() == 8 {
                        text.push_str(&digits[6..]);
                    }
                }
                text_bufs.remove(&buf_key);
                out.change = Some(Change::Set(text_value(format, quick, &text)));
            }
        }
        Kind::Font => {
            let current = current_text(file, quick);
            if reset_button(ui, current.is_some()) {
                out.change = Some(Change::Unset);
            }
            let shown = current.clone().unwrap_or_else(|| "default".into());
            let popup = egui::ComboBox::from_id_salt(&buf_key)
                .width(240.0)
                .height(320.0)
                .selected_text(if current.is_some() {
                    RichText::new(shown)
                } else {
                    RichText::new(shown).italics().color(p.on_surface_variant)
                })
                .show_ui(ui, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(font_filter)
                            .hint_text("Filter fonts or type a name")
                            .desired_width(f32::INFINITY),
                    )
                    .request_focus();
                    let filter = font_filter.trim().to_lowercase();
                    if !filter.is_empty()
                        && ui
                            .selectable_label(false, format!("Use “{}”", font_filter.trim()))
                            .clicked()
                    {
                        out.change = Some(Change::Set(text_value(format, quick, font_filter.trim())));
                    }
                    for generic in ["monospace", "sans-serif", "serif"] {
                        if (filter.is_empty() || generic.contains(&filter))
                            && ui.selectable_label(current.as_deref() == Some(generic), generic).clicked()
                        {
                            out.change = Some(Change::Set(text_value(format, quick, generic)));
                        }
                    }
                    match fonts {
                        Some(list) => {
                            for f in list.iter().filter(|f| filter.is_empty() || f.to_lowercase().contains(&filter)) {
                                if ui.selectable_label(current.as_deref() == Some(f), f).clicked() {
                                    out.change = Some(Change::Set(text_value(format, quick, f)));
                                }
                            }
                        }
                        None => {
                            ui.add(egui::Spinner::new());
                        }
                    }
                });
            if popup.inner.is_none() && !font_filter.is_empty() {
                font_filter.clear();
            }
        }
    }
    out
}

/// Installed font families, from fontconfig.
pub fn load_fonts() -> Vec<String> {
    let out = material_3_ui::sys::cmd::run("fc-list", &[":", "family"]).unwrap_or_default();
    let mut families: Vec<String> = out
        .lines()
        .filter_map(|l| l.split(',').next())
        .map(|s| s.trim().replace("\\-", "-"))
        .filter(|s| !s.is_empty())
        .collect();
    families.sort_by_key(|s| s.to_lowercase());
    families.dedup();
    families
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::kv::Dialect;

    #[test]
    fn values_in_file_notation() {
        let kv = Format::Kv(Dialect::SPACE);
        assert_eq!(bool_value(kv, BoolStyle::YesNo, true), Value::String("yes".into()));
        assert_eq!(bool_value(kv, BoolStyle::Capitalized, false), Value::String("False".into()));
        assert_eq!(bool_value(Format::Toml, BoolStyle::YesNo, true), Value::Bool(true));
        assert_eq!(number_value(kv, 0.950000001, 0.05, false), Value::String("0.95".into()));
        assert_eq!(number_value(kv, 11.0, 0.5, false), Value::String("11.0".into()));
        assert_eq!(number_value(Format::Json, 14.2, 1.0, true), serde_json::json!(14));
        assert_eq!(number_value(Format::Toml, 0.9, 0.05, false), serde_json::json!(0.9));
        let q = Quick::new("k", "K", Kind::Text).quoted();
        assert_eq!(text_value(kv, &q, "a b"), Value::String("\"a b\"".into()));
        assert_eq!(text_value(Format::Toml, &q, "a b"), Value::String("a b".into()));
    }
}
