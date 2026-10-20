//! NetworkManager through `nmcli` in terse (`-t`) mode, plus the hostname.

use super::cmd;

/// Split one terse nmcli line. Fields are `:`-separated; a literal `:` is
/// written `\:` and a backslash `\\`.
pub fn split_terse(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    fields.last_mut().expect("non-empty").push(n);
                }
            }
            ':' => fields.push(String::new()),
            c => fields.last_mut().expect("non-empty").push(c),
        }
    }
    fields
}

#[derive(Clone, Debug, PartialEq)]
pub struct Wifi {
    pub ssid: String,
    pub signal: u8,
    /// Empty or `--` for open networks.
    pub security: String,
    pub in_use: bool,
}

impl Wifi {
    pub fn secured(&self) -> bool {
        let s = self.security.trim();
        !s.is_empty() && s != "--"
    }
}

/// `nmcli -t -f IN-USE,SSID,SIGNAL,SECURITY,BSSID dev wifi list`, deduped by
/// SSID (strongest access point wins), hidden networks dropped, the
/// connected one first and the rest by signal.
pub fn parse_wifi_list(text: &str) -> Vec<Wifi> {
    let mut out: Vec<Wifi> = Vec::new();
    for line in text.lines() {
        let f = split_terse(line);
        if f.len() < 4 || f[1].is_empty() {
            continue;
        }
        let wifi = Wifi {
            in_use: f[0].trim() == "*",
            ssid: f[1].clone(),
            signal: f[2].trim().parse().unwrap_or(0),
            security: f[3].clone(),
        };
        match out.iter_mut().find(|w| w.ssid == wifi.ssid) {
            Some(existing) => {
                existing.in_use |= wifi.in_use;
                if wifi.signal > existing.signal {
                    existing.signal = wifi.signal;
                    existing.security = wifi.security;
                }
            }
            None => out.push(wifi),
        }
    }
    out.sort_by(|a, b| b.in_use.cmp(&a.in_use).then(b.signal.cmp(&a.signal)));
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct Connection {
    pub name: String,
    /// `802-11-wireless`, `802-3-ethernet`, `vpn`, `wireguard`, `loopback`…
    pub kind: String,
    pub device: String,
    pub active: bool,
}

impl Connection {
    pub fn is_wifi(&self) -> bool {
        self.kind == "802-11-wireless" || self.kind == "wifi"
    }
}

/// `nmcli -t -f NAME,TYPE,DEVICE,ACTIVE connection show`
pub fn parse_connections(text: &str) -> Vec<Connection> {
    text.lines()
        .filter_map(|line| {
            let f = split_terse(line);
            (f.len() >= 4).then(|| Connection {
                name: f[0].clone(),
                kind: f[1].clone(),
                device: f[2].clone(),
                active: f[3] == "yes",
            })
        })
        .filter(|c| c.kind != "loopback")
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub wifi_enabled: bool,
    pub airplane: bool,
    pub has_wifi_device: bool,
    pub connections: Vec<Connection>,
    pub hostname: String,
}

impl State {
    pub fn active(&self) -> impl Iterator<Item = &Connection> {
        self.connections.iter().filter(|c| c.active)
    }

    pub fn saved_wifi(&self, ssid: &str) -> Option<&Connection> {
        self.connections.iter().find(|c| c.is_wifi() && c.name == ssid)
    }
}

/// `nmcli -t radio all` → `WIFI-HW:WIFI:WWAN-HW:WWAN`.
pub fn parse_radio_all(text: &str) -> (bool, bool) {
    let f = split_terse(text.trim());
    let wifi = f.get(1).is_some_and(|s| s == "enabled");
    let wwan = f.get(3).is_some_and(|s| s == "enabled");
    (wifi, !wifi && !wwan)
}

fn hostname() -> String {
    cmd::run("hostnamectl", &["hostname"])
        .or_else(|_| std::fs::read_to_string("/etc/hostname").map_err(|e| e.to_string()))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Everything except the (slower) Wi-Fi scan. Blocking.
pub fn read() -> Result<State, String> {
    let radio = cmd::run("nmcli", &["-t", "radio", "all"])?;
    let (wifi_enabled, airplane) = parse_radio_all(&radio);
    let devices = cmd::run("nmcli", &["-t", "-f", "TYPE", "device"]).unwrap_or_default();
    let connections =
        cmd::run("nmcli", &["-t", "-f", "NAME,TYPE,DEVICE,ACTIVE", "connection", "show"])
            .map(|o| parse_connections(&o))
            .unwrap_or_default();
    Ok(State {
        wifi_enabled,
        airplane,
        has_wifi_device: devices.lines().any(|l| l.trim() == "wifi"),
        connections,
        hostname: hostname(),
    })
}

pub fn scan(rescan: bool) -> Result<Vec<Wifi>, String> {
    let out = cmd::run(
        "nmcli",
        &[
            "-t",
            "-f",
            "IN-USE,SSID,SIGNAL,SECURITY,BSSID",
            "device",
            "wifi",
            "list",
            "--rescan",
            if rescan { "yes" } else { "auto" },
        ],
    )?;
    Ok(parse_wifi_list(&out))
}

pub fn set_wifi(on: bool) -> Result<(), String> {
    cmd::run("nmcli", &["radio", "wifi", if on { "on" } else { "off" }]).map(|_| ())
}

pub fn set_airplane(on: bool) -> Result<(), String> {
    cmd::run("nmcli", &["radio", "all", if on { "off" } else { "on" }]).map(|_| ())
}

/// Connect to a network: a saved profile is brought up; otherwise a new one
/// is created (with `password` for secured networks).
pub fn connect(ssid: &str, password: Option<&str>, saved: bool) -> Result<(), String> {
    if saved && password.is_none() {
        return cmd::run("nmcli", &["connection", "up", "id", ssid]).map(|_| ());
    }
    let mut args = vec!["device", "wifi", "connect", ssid];
    if let Some(pw) = password {
        args.extend(["password", pw]);
    }
    cmd::run("nmcli", &args).map(|_| ())
}

pub fn disconnect(name: &str) -> Result<(), String> {
    cmd::run("nmcli", &["connection", "down", "id", name]).map(|_| ())
}

pub fn forget(name: &str) -> Result<(), String> {
    cmd::run("nmcli", &["connection", "delete", "id", name]).map(|_| ())
}

pub fn set_hostname(name: &str) -> Result<(), String> {
    cmd::run("hostnamectl", &["set-hostname", name]).map(|_| ())
}

/// Hostnames: letters, digits and hyphens, dot-separated labels.
pub fn valid_hostname(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.split('.').all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terse_escapes() {
        assert_eq!(split_terse("a:b\\:c:d\\\\e"), vec!["a", "b:c", "d\\e"]);
        assert_eq!(split_terse(":x:"), vec!["", "x", ""]);
    }

    #[test]
    fn wifi_list() {
        let out = "\
*:Home Net:72:WPA2:AA\\:BB\\:CC\\:DD\\:EE\\:01
 :Home Net:40:WPA2:AA\\:BB\\:CC\\:DD\\:EE\\:02
:Cafe\\: Free:55::11\\:22\\:33\\:44\\:55\\:66
:Office:88:WPA2 WPA3:11\\:22\\:33\\:44\\:55\\:77
::30:WPA2:11\\:22\\:33\\:44\\:55\\:88
";
        let list = parse_wifi_list(out);
        assert_eq!(list.len(), 3, "deduped and hidden dropped");
        assert_eq!(list[0].ssid, "Home Net");
        assert!(list[0].in_use);
        assert_eq!(list[0].signal, 72);
        assert_eq!(list[1].ssid, "Office");
        assert_eq!(list[2].ssid, "Cafe: Free");
        assert!(!list[2].secured());
        assert!(list[1].secured());
    }

    #[test]
    fn connections_and_radio() {
        let out = "Home Net:802-11-wireless:wlan0:yes\nWired connection 1:802-3-ethernet::no\nlo:loopback:lo:yes\nwg0:wireguard:wg0:yes\n";
        let c = parse_connections(out);
        assert_eq!(c.len(), 3);
        assert!(c[0].is_wifi() && c[0].active);
        assert!(!c[1].active);
        let state = State {
            connections: c,
            ..Default::default()
        };
        assert_eq!(state.active().count(), 2);
        assert!(state.saved_wifi("Home Net").is_some());

        assert_eq!(parse_radio_all("enabled:enabled:enabled:enabled\n"), (true, false));
        assert_eq!(parse_radio_all("enabled:disabled:enabled:disabled\n"), (false, true));
    }

    #[test]
    fn hostnames() {
        assert!(valid_hostname("archbox"));
        assert!(valid_hostname("my-pc.lan"));
        assert!(!valid_hostname("bad name"));
        assert!(!valid_hostname("-x"));
        assert!(!valid_hostname(""));
    }
}
