//! Power profiles (power-profiles-daemon), batteries from sysfs, screen
//! brightness (brightnessctl) and charge thresholds.

use super::cmd;
use std::path::{Path, PathBuf};

/// Profiles as reported by `powerprofilesctl list`; the active one is
/// marked with `*`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profiles {
    pub available: Vec<String>,
    pub active: String,
}

pub fn parse_profiles(list: &str) -> Profiles {
    let mut p = Profiles::default();
    for line in list.lines() {
        // Headers look like "* balanced:" or "  power-saver:"; detail lines
        // are indented further and carry a value after the colon.
        let indent = line.len() - line.trim_start().len();
        let t = line.trim();
        let starred = t.starts_with('*');
        let name = t.trim_start_matches('*').trim();
        if let Some(name) = name.strip_suffix(':') {
            if indent <= 2 && !name.is_empty() && !name.contains(char::is_whitespace) {
                p.available.push(name.to_string());
                if starred {
                    p.active = name.to_string();
                }
            }
        }
    }
    p
}

pub fn load_profiles() -> Result<Profiles, String> {
    let list = cmd::run("powerprofilesctl", &["list"])?;
    let mut p = parse_profiles(&list);
    if p.active.is_empty() {
        p.active = cmd::run("powerprofilesctl", &["get"])?.trim().to_string();
    }
    Ok(p)
}

pub fn set_profile(name: &str) -> Result<(), String> {
    cmd::run("powerprofilesctl", &["set", name]).map(|_| ())
}

pub fn profile_label(name: &str) -> &str {
    match name {
        "power-saver" => "Power saver",
        "balanced" => "Balanced",
        "performance" => "Performance",
        other => other,
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Battery {
    pub name: String,
    pub path: PathBuf,
    pub capacity: Option<u8>,
    pub status: String,
    /// Health: full / full_design, in percent.
    pub health: Option<f32>,
    /// Hours until empty (discharging) or full (charging).
    pub hours_left: Option<f32>,
    pub model: String,
    pub end_threshold: Option<u8>,
    pub start_threshold: Option<u8>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PowerSupplies {
    pub batteries: Vec<Battery>,
    pub ac_online: Option<bool>,
}

fn read(dir: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(name))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn read_num(dir: &Path, name: &str) -> Option<f64> {
    read(dir, name)?.parse().ok()
}

pub fn read_battery(dir: &Path) -> Battery {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let status = read(dir, "status").unwrap_or_else(|| "Unknown".into());
    // Energy (µWh / µW) or charge (µAh / µA): either gives the same ratios.
    let (now, full, design, rate) = if read(dir, "energy_now").is_some() {
        (
            read_num(dir, "energy_now"),
            read_num(dir, "energy_full"),
            read_num(dir, "energy_full_design"),
            read_num(dir, "power_now"),
        )
    } else {
        (
            read_num(dir, "charge_now"),
            read_num(dir, "charge_full"),
            read_num(dir, "charge_full_design"),
            read_num(dir, "current_now"),
        )
    };
    let health = match (full, design) {
        (Some(f), Some(d)) if d > 0.0 => Some((f / d * 100.0) as f32),
        _ => None,
    };
    let hours_left = match (now, full, rate) {
        (Some(n), Some(f), Some(r)) if r > 0.0 => match status.as_str() {
            "Discharging" => Some((n / r) as f32),
            "Charging" => Some(((f - n).max(0.0) / r) as f32),
            _ => None,
        },
        _ => None,
    };
    let capacity = read_num(dir, "capacity")
        .map(|c| c.clamp(0.0, 100.0) as u8)
        .or(match (now, full) {
            (Some(n), Some(f)) if f > 0.0 => Some((n / f * 100.0).round().clamp(0.0, 100.0) as u8),
            _ => None,
        });
    let model = [read(dir, "manufacturer"), read(dir, "model_name")]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    Battery {
        name,
        path: dir.to_path_buf(),
        capacity,
        status,
        health,
        hours_left,
        model,
        end_threshold: read_num(dir, "charge_control_end_threshold").map(|v| v as u8),
        start_threshold: read_num(dir, "charge_control_start_threshold").map(|v| v as u8),
    }
}

pub fn read_supplies_in(root: &Path) -> PowerSupplies {
    let mut out = PowerSupplies::default();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    dirs.sort();
    for dir in dirs {
        match read(&dir, "type").as_deref() {
            // Peripherals (mice, headsets) report scope=Device.
            Some("Battery") if read(&dir, "scope").as_deref() != Some("Device") => {
                out.batteries.push(read_battery(&dir));
            }
            Some("Mains") => {
                let online = read(&dir, "online").as_deref() == Some("1");
                out.ac_online = Some(out.ac_online.unwrap_or(false) || online);
            }
            _ => {}
        }
    }
    out
}

pub fn read_supplies() -> PowerSupplies {
    read_supplies_in(Path::new("/sys/class/power_supply"))
}

/// Write a charge threshold (root, via polkit). The value and path are
/// passed as arguments, never interpolated into the script.
pub fn set_threshold(battery: &Path, file: &str, value: u8) -> Result<(), String> {
    if !matches!(
        file,
        "charge_control_end_threshold" | "charge_control_start_threshold"
    ) {
        return Err("unknown threshold".into());
    }
    let path = battery.join(file);
    let target = path.to_string_lossy();
    let value = value.min(100).to_string();
    cmd::run(
        "pkexec",
        &["sh", "-c", r#"echo "$1" > "$2""#, "m3-settings", &value, &target],
    )
    .map(|_| ())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Backlight {
    pub device: String,
    pub percent: u8,
}

/// Parse `brightnessctl -m` (`device,class,current,percent,max`).
pub fn parse_brightness(out: &str) -> Option<Backlight> {
    let line = out
        .lines()
        .find(|l| l.contains(",backlight,"))
        .or_else(|| out.lines().next())?;
    let fields: Vec<&str> = line.split(',').collect();
    if fields.len() < 5 {
        return None;
    }
    let percent = fields[3].trim_end_matches('%').parse().ok()?;
    Some(Backlight {
        device: fields[0].to_string(),
        percent,
    })
}

pub fn load_brightness() -> Result<Option<Backlight>, String> {
    let out = cmd::run("brightnessctl", &["-m", "--class=backlight"])
        .or_else(|_| cmd::run("brightnessctl", &["-m"]))?;
    Ok(parse_brightness(&out))
}

pub fn set_brightness(device: &str, percent: u8) -> Result<(), String> {
    let pct = format!("{}%", percent.clamp(1, 100));
    cmd::run("brightnessctl", &["-d", device, "set", &pct]).map(|_| ())
}

/// Swap that could hold a hibernation image.
pub fn has_swap() -> bool {
    std::fs::read_to_string("/proc/swaps")
        .map(|s| s.lines().count() > 1)
        .unwrap_or(false)
}

pub fn format_hours(h: f32) -> String {
    let total = (h * 60.0).round() as u32;
    let (hh, mm) = (total / 60, total % 60);
    if hh == 0 {
        format!("{mm} min")
    } else {
        format!("{hh} h {mm:02} min")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_list() {
        let out = "  performance:\n    CpuDriver:\tamd_pstate\n    Degraded:   no\n\n* balanced:\n    CpuDriver:\tamd_pstate\n    PlatformDriver:\tplatform_profile\n\n  power-saver:\n    CpuDriver:\tamd_pstate\n";
        let p = parse_profiles(out);
        assert_eq!(p.available, ["performance", "balanced", "power-saver"]);
        assert_eq!(p.active, "balanced");
    }

    #[test]
    fn brightness() {
        let b = parse_brightness("intel_backlight,backlight,9600,50%,19200\n").unwrap();
        assert_eq!(b.device, "intel_backlight");
        assert_eq!(b.percent, 50);
        assert!(parse_brightness("garbage").is_none());
    }

    #[test]
    fn sysfs_battery() {
        let dir = tempfile::tempdir().unwrap();
        let bat = dir.path().join("BAT0");
        std::fs::create_dir(&bat).unwrap();
        for (k, v) in [
            ("type", "Battery"),
            ("status", "Discharging"),
            ("capacity", "50"),
            ("energy_now", "25000000"),
            ("energy_full", "50000000"),
            ("energy_full_design", "62500000"),
            ("power_now", "10000000"),
            ("charge_control_end_threshold", "80"),
            ("model_name", "5B10W51"),
        ] {
            std::fs::write(bat.join(k), format!("{v}\n")).unwrap();
        }
        let ac = dir.path().join("AC");
        std::fs::create_dir(&ac).unwrap();
        std::fs::write(ac.join("type"), "Mains\n").unwrap();
        std::fs::write(ac.join("online"), "0\n").unwrap();
        let mouse = dir.path().join("hidpp_battery_0");
        std::fs::create_dir(&mouse).unwrap();
        std::fs::write(mouse.join("type"), "Battery\n").unwrap();
        std::fs::write(mouse.join("scope"), "Device\n").unwrap();

        let s = read_supplies_in(dir.path());
        assert_eq!(s.ac_online, Some(false));
        assert_eq!(s.batteries.len(), 1);
        let b = &s.batteries[0];
        assert_eq!(b.capacity, Some(50));
        assert_eq!(b.health.map(|h| h.round()), Some(80.0));
        assert_eq!(b.hours_left, Some(2.5));
        assert_eq!(b.end_threshold, Some(80));
        assert_eq!(format_hours(2.5), "2 h 30 min");
        assert_eq!(format_hours(0.25), "15 min");
    }
}
