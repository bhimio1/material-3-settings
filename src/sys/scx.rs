//! sched-ext CPU schedulers through `scxctl` (scx_loader), which CachyOS
//! ships by default. Output formats follow scx-loader's scxctl:
//! `running Lavd in Gaming mode`, `running Bpfland with its own defaults`,
//! `running Lavd with arguments "…"`, `no scx scheduler running` and
//! `supported schedulers: ["bpfland", "lavd", …]`.

use material_3_ui::sys::cmd;

/// Modes scx_loader understands (`scxctl … --mode`).
pub const MODES: [(&str, &str); 5] = [
    ("auto", "Auto"),
    ("gaming", "Gaming"),
    ("lowlatency", "Low latency"),
    ("powersave", "Power save"),
    ("server", "Server"),
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Current {
    /// Scheduler name without the `scx_` prefix, lowercase. None = stock
    /// kernel scheduler (EEVDF).
    pub sched: Option<String>,
    /// Mode in `scxctl` spelling (`gaming`), when one is reported.
    pub mode: Option<String>,
    /// Custom arguments, when it runs with them instead of a mode.
    pub args: Option<String>,
}

pub fn available() -> bool {
    cmd::exists("scxctl")
}

/// Parse `scxctl get`.
pub fn parse_get(out: &str) -> Current {
    let line = out.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let Some(rest) = line.strip_prefix("running ") else {
        return Current::default();
    };
    let (name, tail) = rest.split_once(' ').unwrap_or((rest, ""));
    let sched = Some(name.trim_matches('"').to_lowercase());
    if let Some(mode) = tail.strip_prefix("in ").and_then(|t| t.strip_suffix(" mode")) {
        return Current {
            sched,
            mode: Some(mode.trim().to_lowercase()),
            args: None,
        };
    }
    if let Some(args) = tail.strip_prefix("with arguments ") {
        return Current {
            sched,
            mode: None,
            args: Some(args.trim().trim_matches('"').to_string()),
        };
    }
    Current {
        sched,
        mode: None,
        args: None,
    }
}

/// Parse `scxctl list`.
pub fn parse_list(out: &str) -> Vec<String> {
    let Some(start) = out.find('[') else {
        return Vec::new();
    };
    let end = out[start..].find(']').map(|e| start + e).unwrap_or(out.len());
    out[start + 1..end]
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_start_matches("scx_").to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn current() -> Result<Current, String> {
    cmd::run("scxctl", &["get"]).map(|o| parse_get(&o))
}

pub fn list() -> Result<Vec<String>, String> {
    cmd::run("scxctl", &["list"]).map(|o| parse_list(&o))
}

/// Start or switch to `sched` in `mode`.
pub fn switch(sched: &str, mode: &str, running: bool) -> Result<String, String> {
    let verb = if running { "switch" } else { "start" };
    cmd::run("scxctl", &[verb, "--sched", sched, "--mode", mode]).map(|o| o.trim().to_string())
}

pub fn stop() -> Result<String, String> {
    cmd::run("scxctl", &["stop"]).map(|o| o.trim().to_string())
}

/// Go back to the default scheduler from scx_loader's configuration.
pub fn restore() -> Result<String, String> {
    cmd::run("scxctl", &["restore"]).map(|o| o.trim().to_string())
}

/// Human description of what's running.
pub fn describe(c: &Current) -> String {
    match (&c.sched, &c.mode, &c.args) {
        (None, _, _) => "The kernel's own scheduler (no sched-ext scheduler loaded).".into(),
        (Some(s), Some(m), _) => {
            let mode = MODES.iter().find(|(k, _)| k == m).map(|(_, l)| *l).unwrap_or(m);
            format!("scx_{s}, {mode} mode.")
        }
        (Some(s), None, Some(a)) => format!("scx_{s} with arguments {a}."),
        (Some(s), None, None) => format!("scx_{s} with its own defaults."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_output() {
        assert_eq!(parse_get("no scx scheduler running\n"), Current::default());
        assert_eq!(
            parse_get("running Lavd in Gaming mode\n"),
            Current { sched: Some("lavd".into()), mode: Some("gaming".into()), args: None }
        );
        assert_eq!(
            parse_get("running Bpfland in LowLatency mode"),
            Current { sched: Some("bpfland".into()), mode: Some("lowlatency".into()), args: None }
        );
        assert_eq!(
            parse_get("running Rusty with its own defaults\n").sched.as_deref(),
            Some("rusty")
        );
        assert_eq!(
            parse_get("running Lavd with arguments \"--performance --slice-us 5000\"").args.as_deref(),
            Some("--performance --slice-us 5000")
        );
    }

    #[test]
    fn list_output() {
        assert_eq!(
            parse_list("supported schedulers: [\"bpfland\", \"lavd\", \"scx_rusty\"]\n"),
            vec!["bpfland", "lavd", "rusty"]
        );
        assert!(parse_list("garbage").is_empty());
    }

    #[test]
    fn descriptions() {
        assert!(describe(&Current::default()).contains("kernel's own"));
        let c = parse_get("running Lavd in Gaming mode");
        assert_eq!(describe(&c), "scx_lavd, Gaming mode.");
    }
}
