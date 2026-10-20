//! Bluetooth through `bluetoothctl` (bluez-utils).

use super::cmd;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Controller {
    pub address: String,
    pub name: String,
    pub alias: String,
    pub powered: bool,
    pub discoverable: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Device {
    pub address: String,
    pub name: String,
    /// freedesktop icon hint from bluez (`audio-headset`, `input-mouse`…).
    pub icon: String,
    pub paired: bool,
    pub trusted: bool,
    pub connected: bool,
    pub battery: Option<u8>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// The bluetooth systemd service is running.
    pub service_active: bool,
    /// `None` when there's no adapter.
    pub controller: Option<Controller>,
    pub devices: Vec<Device>,
}

impl State {
    pub fn paired(&self) -> impl Iterator<Item = &Device> {
        self.devices.iter().filter(|d| d.paired)
    }

    pub fn available(&self) -> impl Iterator<Item = &Device> {
        self.devices.iter().filter(|d| !d.paired)
    }
}

/// `Key: value` lines under an indented block.
fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|l| {
        let (k, v) = l.trim().split_once(':')?;
        (k.trim() == key).then(|| v.trim())
    })
}

fn yes(text: &str, key: &str) -> bool {
    field(text, key) == Some("yes")
}

/// `bluetoothctl show`
pub fn parse_show(text: &str) -> Option<Controller> {
    let first = text.lines().find(|l| l.starts_with("Controller "))?;
    Some(Controller {
        address: first.split_whitespace().nth(1)?.to_string(),
        name: field(text, "Name").unwrap_or_default().to_string(),
        alias: field(text, "Alias").unwrap_or_default().to_string(),
        powered: yes(text, "Powered"),
        discoverable: yes(text, "Discoverable"),
    })
}

/// `bluetoothctl devices` → (address, name). Names that are just the
/// address with dashes (unnamed LE devices) are skipped.
pub fn parse_devices(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("Device ")?;
            let (addr, name) = rest.split_once(' ').unwrap_or((rest, ""));
            if addr.len() != 17 {
                return None;
            }
            Some((addr.to_string(), name.trim().to_string()))
        })
        .collect()
}

fn is_unnamed(addr: &str, name: &str) -> bool {
    name.is_empty() || name.replace('-', ":").eq_ignore_ascii_case(addr)
}

/// `bluetoothctl info <addr>`
pub fn parse_info(address: &str, fallback_name: &str, text: &str) -> Device {
    let battery = field(text, "Battery Percentage").and_then(|v| {
        // "0x46 (70)"
        let inner = v.split_once('(')?.1.trim_end_matches(')');
        inner.trim().parse().ok()
    });
    Device {
        address: address.to_string(),
        name: field(text, "Alias")
            .or_else(|| field(text, "Name"))
            .unwrap_or(fallback_name)
            .to_string(),
        icon: field(text, "Icon").unwrap_or_default().to_string(),
        paired: yes(text, "Paired"),
        trusted: yes(text, "Trusted"),
        connected: yes(text, "Connected"),
        battery,
    }
}

fn devices(filter: Option<&str>) -> Vec<(String, String)> {
    let mut args = vec!["devices"];
    if let Some(f) = filter {
        args.push(f);
    }
    match cmd::run("bluetoothctl", &args) {
        Ok(out) => parse_devices(&out),
        // bluez < 5.65 has no filter argument.
        Err(_) if filter == Some("Paired") => cmd::run("bluetoothctl", &["paired-devices"])
            .map(|o| parse_devices(&o))
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Everything. Blocking.
pub fn read() -> Result<State, String> {
    let service_active = cmd::run("systemctl", &["is-active", "bluetooth"])
        .map(|s| s.trim() == "active")
        .unwrap_or(false);
    let show = cmd::run("bluetoothctl", &["show"]).unwrap_or_default();
    let controller = parse_show(&show);
    let mut list = Vec::new();
    if controller.is_some() {
        let mut all = devices(Some("Paired"));
        for d in devices(None) {
            if !all.iter().any(|(a, _)| *a == d.0) && !is_unnamed(&d.0, &d.1) {
                all.push(d);
            }
        }
        for (addr, name) in all.into_iter().take(40) {
            let info = cmd::run("bluetoothctl", &["info", &addr]).unwrap_or_default();
            list.push(parse_info(&addr, &name, &info));
        }
    }
    list.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then(b.paired.cmp(&a.paired))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(State {
        service_active,
        controller,
        devices: list,
    })
}

fn ctl(args: &[&str]) -> Result<(), String> {
    let out = cmd::run("bluetoothctl", args)?;
    // bluetoothctl exits 0 on many failures; look at the text.
    let lower = out.to_lowercase();
    if lower.contains("failed") || lower.contains("not available") || lower.contains("error") {
        let line = out
            .lines()
            .rev()
            .find(|l| {
                let l = l.to_lowercase();
                l.contains("failed") || l.contains("error") || l.contains("not available")
            })
            .unwrap_or("bluetoothctl failed");
        return Err(line.trim().to_string());
    }
    Ok(())
}

pub fn set_powered(on: bool) -> Result<(), String> {
    ctl(&["power", if on { "on" } else { "off" }])
}

pub fn set_discoverable(on: bool) -> Result<(), String> {
    ctl(&["discoverable", if on { "on" } else { "off" }])
}

/// Discover nearby devices for `seconds`. Blocking.
pub fn scan(seconds: u32) -> Result<(), String> {
    cmd::run("bluetoothctl", &["--timeout", &seconds.to_string(), "scan", "on"]).map(|_| ())
}

/// Pair, trust and connect a new device.
pub fn pair(address: &str) -> Result<(), String> {
    ctl(&["--timeout", "25", "pair", address])?;
    ctl(&["trust", address])?;
    ctl(&["--timeout", "15", "connect", address])
}

pub fn connect(address: &str) -> Result<(), String> {
    ctl(&["--timeout", "15", "connect", address])
}

pub fn disconnect(address: &str) -> Result<(), String> {
    ctl(&["disconnect", address])
}

pub fn remove(address: &str) -> Result<(), String> {
    ctl(&["remove", address])
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOW: &str = "Controller 00:1A:7D:DA:71:13 (public)
\tManufacturer: 0x000a (10)
\tName: archbox
\tAlias: archbox
\tClass: 0x006c010c
\tPowered: yes
\tDiscoverable: no
\tPairable: yes
\tDiscovering: no
";

    const INFO: &str = "Device AC:80:0A:11:22:33 (public)
\tName: WH-1000XM4
\tAlias: Headphones
\tClass: 0x00240404
\tIcon: audio-headset
\tPaired: yes
\tBonded: yes
\tTrusted: yes
\tBlocked: no
\tConnected: yes
\tLegacyPairing: no
\tBattery Percentage: 0x46 (70)
";

    #[test]
    fn controller() {
        let c = parse_show(SHOW).unwrap();
        assert_eq!(c.address, "00:1A:7D:DA:71:13");
        assert_eq!(c.alias, "archbox");
        assert!(c.powered && !c.discoverable);
        assert!(parse_show("No default controller available\n").is_none());
    }

    #[test]
    fn device_lists_and_info() {
        let d = parse_devices(
            "Device AC:80:0A:11:22:33 WH-1000XM4\nDevice 4C:87:5D:00:11:22 4C-87-5D-00-11-22\n[CHG] junk\n",
        );
        assert_eq!(d.len(), 2);
        assert!(is_unnamed(&d[1].0, &d[1].1));
        assert!(!is_unnamed(&d[0].0, &d[0].1));

        let dev = parse_info("AC:80:0A:11:22:33", "WH", INFO);
        assert_eq!(dev.name, "Headphones");
        assert_eq!(dev.icon, "audio-headset");
        assert!(dev.paired && dev.trusted && dev.connected);
        assert_eq!(dev.battery, Some(70));
        let bare = parse_info("X", "Fallback", "");
        assert_eq!(bare.name, "Fallback");
        assert!(!bare.paired);
    }
}
