//! Time zone, NTP and locale through systemd's `timedatectl` / `localectl`.

use material_3_ui::sys::cmd;
use std::collections::HashMap;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimeStatus {
    pub timezone: String,
    pub ntp: bool,
    pub ntp_synced: bool,
    pub local_rtc: bool,
    /// Whether systemd-timesyncd (or another NTP unit) can be enabled.
    pub can_ntp: bool,
}

/// `Key=Value` lines as printed by `timedatectl show`.
fn props(text: &str) -> HashMap<&str, &str> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim(), v.trim()))
        .collect()
}

pub fn parse_show(text: &str) -> TimeStatus {
    let p = props(text);
    let yes = |k: &str| p.get(k).is_some_and(|v| *v == "yes");
    TimeStatus {
        timezone: p.get("Timezone").unwrap_or(&"").to_string(),
        ntp: yes("NTP"),
        ntp_synced: yes("NTPSynchronized"),
        local_rtc: yes("LocalRTC"),
        can_ntp: yes("CanNTP") || !p.contains_key("CanNTP"),
    }
}

pub fn load_status() -> Result<TimeStatus, String> {
    cmd::run("timedatectl", &["show"]).map(|t| parse_show(&t))
}

pub fn list_timezones() -> Result<Vec<String>, String> {
    let out = cmd::run("timedatectl", &["list-timezones"])?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

pub fn set_timezone(tz: &str) -> Result<(), String> {
    cmd::run("timedatectl", &["set-timezone", tz]).map(|_| ())
}

pub fn set_ntp(on: bool) -> Result<(), String> {
    cmd::run("timedatectl", &["set-ntp", if on { "true" } else { "false" }]).map(|_| ())
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LocaleStatus {
    pub locale: String,
    pub keymap: String,
    pub x11_layout: String,
    pub x11_variant: String,
}

/// Parse `localectl status`:
/// ```text
///    System Locale: LANG=en_US.UTF-8
///        VC Keymap: us
///       X11 Layout: us
/// ```
pub fn parse_localectl(text: &str) -> LocaleStatus {
    let mut s = LocaleStatus::default();
    let mut last_key = "";
    for line in text.lines() {
        let (key, value) = match line.split_once(':') {
            Some((k, v)) if !k.trim().is_empty() => (k.trim(), v.trim()),
            // Continuation lines of System Locale (LC_* variables).
            _ => (last_key, line.trim()),
        };
        match key {
            "System Locale" => {
                if s.locale.is_empty() {
                    s.locale = value.to_string();
                } else if !value.is_empty() {
                    s.locale.push_str(", ");
                    s.locale.push_str(value);
                }
            }
            "VC Keymap" => s.keymap = value.to_string(),
            "X11 Layout" => s.x11_layout = value.to_string(),
            "X11 Variant" => s.x11_variant = value.to_string(),
            _ => {}
        }
        last_key = key;
    }
    s
}

pub fn load_locale() -> Result<LocaleStatus, String> {
    cmd::run("localectl", &["status"]).map(|t| parse_localectl(&t))
}

/// Locales generated on this system (`locale -a`).
pub fn available_locales() -> Vec<String> {
    cmd::run("locale", &["-a"])
        .map(|o| {
            o.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_output() {
        let t = "Timezone=Europe/Berlin\nLocalRTC=no\nCanNTP=yes\nNTP=yes\nNTPSynchronized=no\nTimeUSec=Mon 2026-10-20 08:00:00 CEST\n";
        let s = parse_show(t);
        assert_eq!(s.timezone, "Europe/Berlin");
        assert!(s.ntp && !s.ntp_synced && !s.local_rtc && s.can_ntp);
    }

    #[test]
    fn localectl_output() {
        let t = "   System Locale: LANG=en_US.UTF-8\n                  LC_TIME=en_GB.UTF-8\n       VC Keymap: us\n      X11 Layout: us,de\n     X11 Variant: ,nodeadkeys\n";
        let s = parse_localectl(t);
        assert_eq!(s.locale, "LANG=en_US.UTF-8, LC_TIME=en_GB.UTF-8");
        assert_eq!(s.keymap, "us");
        assert_eq!(s.x11_layout, "us,de");
        assert_eq!(s.x11_variant, ",nodeadkeys");
    }
}
