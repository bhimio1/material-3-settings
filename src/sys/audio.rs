//! Audio devices through `pactl` (PipeWire's pipewire-pulse or PulseAudio).

use material_3_ui::sys::cmd;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Output,
    Input,
}

impl Dir {
    fn noun(self) -> &'static str {
        match self {
            Dir::Output => "sink",
            Dir::Input => "source",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub name: String,
    pub description: String,
    /// Average over channels, in percent (100 = 0 dB).
    pub volume: u32,
    pub muted: bool,
    /// freedesktop icon name hint (`audio-headset`, `audio-card`…).
    pub icon: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub outputs: Vec<Device>,
    pub inputs: Vec<Device>,
    pub default_output: String,
    pub default_input: String,
}

impl State {
    pub fn devices(&self, dir: Dir) -> &[Device] {
        match dir {
            Dir::Output => &self.outputs,
            Dir::Input => &self.inputs,
        }
    }

    pub fn default_name(&self, dir: Dir) -> &str {
        match dir {
            Dir::Output => &self.default_output,
            Dir::Input => &self.default_input,
        }
    }
}

fn percent(v: &Value) -> Option<u32> {
    v.get("value_percent")
        .and_then(Value::as_str)
        .and_then(|p| p.trim().trim_end_matches('%').parse().ok())
        .or_else(|| {
            v.get("value")
                .and_then(Value::as_f64)
                .map(|raw| (raw / 65536.0 * 100.0).round() as u32)
        })
}

/// Parse `pactl -f json list sinks|sources`.
pub fn parse_json(text: &str, dir: Dir) -> Result<Vec<Device>, String> {
    let list: Vec<Value> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    Ok(list
        .iter()
        .filter_map(|d| {
            let name = d.get("name")?.as_str()?.to_string();
            let props = d.get("properties");
            let is_monitor = name.ends_with(".monitor")
                || props
                    .and_then(|p| p.get("device.class"))
                    .and_then(Value::as_str)
                    == Some("monitor");
            if dir == Dir::Input && is_monitor {
                return None;
            }
            let vols: Vec<u32> = d
                .get("volume")
                .and_then(Value::as_object)
                .map(|m| m.values().filter_map(percent).collect())
                .unwrap_or_default();
            let volume = if vols.is_empty() {
                100
            } else {
                vols.iter().sum::<u32>() / vols.len() as u32
            };
            Some(Device {
                description: d
                    .get("description")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&name)
                    .to_string(),
                name,
                volume,
                muted: d.get("mute").and_then(Value::as_bool).unwrap_or(false),
                icon: props
                    .and_then(|p| p.get("device.icon_name"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            })
        })
        .collect())
}

/// Device names from `pactl list short sinks|sources`.
pub fn parse_short(text: &str, dir: Dir) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.split('\t').nth(1))
        .map(str::to_string)
        .filter(|n| !(dir == Dir::Input && n.ends_with(".monitor")))
        .collect()
}

/// `Volume: front-left: 32768 /  50% / -18.06 dB,   front-right: …`
pub fn parse_volume_line(text: &str) -> Option<u32> {
    let vals: Vec<u32> = text
        .split('/')
        .filter_map(|part| part.trim().strip_suffix('%')?.trim().parse().ok())
        .collect();
    (!vals.is_empty()).then(|| vals.iter().sum::<u32>() / vals.len() as u32)
}

fn list(dir: Dir) -> Result<Vec<Device>, String> {
    let plural = format!("{}s", dir.noun());
    if let Ok(out) = cmd::run("pactl", &["-f", "json", "list", &plural]) {
        if let Ok(devices) = parse_json(&out, dir) {
            return Ok(devices);
        }
    }
    // Older pactl without JSON output.
    let out = cmd::run("pactl", &["list", "short", &plural])?;
    Ok(parse_short(&out, dir)
        .into_iter()
        .map(|name| {
            let vol = cmd::run("pactl", &[&format!("get-{}-volume", dir.noun()), &name])
                .ok()
                .and_then(|o| parse_volume_line(&o))
                .unwrap_or(100);
            let muted = cmd::run("pactl", &[&format!("get-{}-mute", dir.noun()), &name])
                .map(|o| o.contains("yes"))
                .unwrap_or(false);
            Device {
                description: name.clone(),
                name,
                volume: vol,
                muted,
                icon: String::new(),
            }
        })
        .collect())
}

/// Query everything. Blocking.
pub fn read() -> Result<State, String> {
    Ok(State {
        outputs: list(Dir::Output)?,
        inputs: list(Dir::Input).unwrap_or_default(),
        default_output: cmd::run("pactl", &["get-default-sink"])
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
        default_input: cmd::run("pactl", &["get-default-source"])
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
    })
}

pub fn set_default(dir: Dir, name: &str) -> Result<(), String> {
    cmd::run("pactl", &[&format!("set-default-{}", dir.noun()), name]).map(|_| ())
}

pub fn set_volume(dir: Dir, name: &str, percent: u32) -> Result<(), String> {
    cmd::run(
        "pactl",
        &[&format!("set-{}-volume", dir.noun()), name, &format!("{percent}%")],
    )
    .map(|_| ())
}

pub fn set_mute(dir: Dir, name: &str, muted: bool) -> Result<(), String> {
    cmd::run(
        "pactl",
        &[&format!("set-{}-mute", dir.noun()), name, if muted { "1" } else { "0" }],
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SINKS: &str = r#"[{"index":56,"state":"RUNNING","name":"alsa_output.pci-0000_00_1f.3.analog-stereo","description":"Built-in Audio Analog Stereo","driver":"PipeWire","sample_specification":"s32le 2ch 48000Hz","channel_map":"front-left,front-right","owner_module":4294967295,"mute":false,"volume":{"front-left":{"value":42597,"value_percent":"65%","db":"-11.23 dB"},"front-right":{"value":42597,"value_percent":"65%","db":"-11.23 dB"}},"balance":0.0,"base_volume":{"value":65536,"value_percent":"100%","db":"0.00 dB"},"monitor_source":"alsa_output.pci-0000_00_1f.3.analog-stereo.monitor","latency":{"actual":0.0,"configured":0.0},"flags":["HARDWARE","HW_MUTE_CTRL"],"properties":{"device.description":"Built-in Audio","device.icon_name":"audio-card-analog-pci","media.class":"Audio/Sink"},"ports":[],"active_port":"analog-output-speaker","formats":["pcm"]},
{"index":80,"state":"SUSPENDED","name":"bluez_output.AC_80_0A_11_22_33.1","description":"WH-1000XM4","mute":true,"volume":{"front-left":{"value":32768,"value_percent":"50%","db":"-18.06 dB"},"front-right":{"value":39322,"value_percent":"60%","db":"-13.31 dB"}},"properties":{"device.icon_name":"audio-headset-bluetooth"}}]"#;

    const SOURCES: &str = r#"[{"index":57,"name":"alsa_output.pci-0000_00_1f.3.analog-stereo.monitor","description":"Monitor of Built-in Audio","mute":false,"volume":{"mono":{"value":65536,"value_percent":"100%"}},"properties":{"device.class":"monitor"}},
{"index":58,"name":"alsa_input.pci-0000_00_1f.3.analog-stereo","description":"Built-in Audio Analog Stereo","mute":false,"volume":{"front-left":{"value":65536,"value_percent":"100%"},"front-right":{"value":65536,"value_percent":"100%"}},"properties":{"device.icon_name":"audio-input-microphone"}}]"#;

    #[test]
    fn json_lists() {
        let sinks = parse_json(SINKS, Dir::Output).unwrap();
        assert_eq!(sinks.len(), 2);
        assert_eq!(sinks[0].description, "Built-in Audio Analog Stereo");
        assert_eq!(sinks[0].volume, 65);
        assert!(!sinks[0].muted);
        assert_eq!(sinks[1].volume, 55);
        assert!(sinks[1].muted);
        assert_eq!(sinks[1].icon, "audio-headset-bluetooth");

        let sources = parse_json(SOURCES, Dir::Input).unwrap();
        assert_eq!(sources.len(), 1, "monitors are hidden");
        assert_eq!(sources[0].name, "alsa_input.pci-0000_00_1f.3.analog-stereo");
        assert!(parse_json("not json", Dir::Output).is_err());
    }

    #[test]
    fn short_and_volume_fallbacks() {
        let short = "56\talsa_output.pci.analog-stereo\tPipeWire\ts32le 2ch 48000Hz\tRUNNING\n57\talsa_output.pci.analog-stereo.monitor\tPipeWire\ts32le 2ch 48000Hz\tIDLE\n";
        assert_eq!(parse_short(short, Dir::Output).len(), 2);
        assert_eq!(parse_short(short, Dir::Input).len(), 1);
        let vol = "Volume: front-left: 32768 /  50% / -18.06 dB,   front-right: 45875 /  70% / -9.29 dB\n        balance 0.00\n";
        assert_eq!(parse_volume_line(vol), Some(60));
        assert_eq!(parse_volume_line("nothing"), None);
    }
}
