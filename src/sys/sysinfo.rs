//! Hardware and software facts for the About page.

use material_3_ui::sys::cmd;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct Disk {
    pub mount: String,
    pub used: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Default)]
pub struct SystemInfo {
    pub hostname: String,
    pub user: String,
    pub kernel: String,
    pub uptime_secs: u64,
    pub cpu: String,
    pub cores: usize,
    pub mem_total: u64,
    pub mem_available: u64,
    pub gpus: Vec<String>,
    pub disks: Vec<Disk>,
    pub desktop: String,
    pub session: String,
    pub hyprland: Option<String>,
    pub shell: String,
    pub packages: Option<usize>,
    pub uefi: bool,
}

pub fn parse_uptime(text: &str) -> Option<u64> {
    text.split_whitespace().next()?.parse::<f64>().ok().map(|s| s as u64)
}

/// (model name, logical CPUs) from /proc/cpuinfo.
pub fn parse_cpuinfo(text: &str) -> (String, usize) {
    let mut model = String::new();
    let mut count = 0;
    for line in text.lines() {
        let Some((k, v)) = line.split_once(':') else { continue };
        match k.trim() {
            "model name" | "Model" | "Hardware" if model.is_empty() => model = v.trim().to_string(),
            "processor" => count += 1,
            _ => {}
        }
    }
    (squash(&model), count)
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// (MemTotal, MemAvailable) in bytes from /proc/meminfo.
pub fn parse_meminfo(text: &str) -> (u64, u64) {
    let get = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key))
            .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
            .map(|kb| kb * 1024)
            .unwrap_or(0)
    };
    (get("MemTotal:"), get("MemAvailable:"))
}

/// GPU names from `lspci -mm`, whose fields are quoted:
/// `00:02.0 "VGA compatible controller" "Intel Corporation" "Alder Lake-P GT2" ...`
pub fn parse_lspci(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('"').collect();
            // [slot, class, _, vendor, _, device, ...]
            if fields.len() < 6 {
                return None;
            }
            let class = fields[1];
            if !(class.starts_with("VGA") || class.starts_with("3D") || class.starts_with("Display")) {
                return None;
            }
            let vendor = fields[3]
                .replace(" Corporation", "")
                .replace(", Inc.", "")
                .replace(" [AMD/ATI]", "");
            Some(squash(&format!("{vendor} {}", fields[5])))
        })
        .collect()
}

/// `df -B1 --output=target,used,size` rows.
pub fn parse_df(text: &str) -> Vec<Disk> {
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            let mount = parts.next()?.to_string();
            let used = parts.next()?.parse().ok()?;
            let total = parts.next()?.parse().ok()?;
            Some(Disk { mount, used, total })
        })
        .collect()
}

pub fn hyprland_version() -> Option<String> {
    let out = cmd::run("hyprctl", &["version", "-j"]).ok()?;
    let v: serde_json::Value = serde_json::from_str(&out).ok()?;
    v.get("tag")
        .or_else(|| v.get("version"))
        .and_then(|t| t.as_str())
        .map(str::to_string)
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

pub fn load() -> SystemInfo {
    let (cpu, cores) = parse_cpuinfo(&read("/proc/cpuinfo"));
    let (mem_total, mem_available) = parse_meminfo(&read("/proc/meminfo"));
    let mut gpus = cmd::run("lspci", &["-mm"]).map(|o| parse_lspci(&o)).unwrap_or_default();
    if gpus.is_empty() {
        // No pciutils: fall back to DRM driver names.
        if let Ok(rd) = std::fs::read_dir("/sys/class/drm") {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with("card") && !name.contains('-') {
                    if let Ok(link) = std::fs::read_link(e.path().join("device/driver")) {
                        if let Some(d) = link.file_name() {
                            gpus.push(format!("{} ({name})", d.to_string_lossy()));
                        }
                    }
                }
            }
        }
    }
    let home = material_3_ui::sys::paths::home();
    let mut df_args = vec!["-B1", "--output=target,used,size", "/"];
    let home_str = home.to_string_lossy().into_owned();
    df_args.push(&home_str);
    let mut disks = cmd::run("df", &df_args).map(|o| parse_df(&o)).unwrap_or_default();
    disks.dedup_by(|a, b| a.mount == b.mount);

    let hostname = read("/proc/sys/kernel/hostname").trim().to_string();
    SystemInfo {
        hostname: if hostname.is_empty() {
            read("/etc/hostname").trim().to_string()
        } else {
            hostname
        },
        user: std::env::var("USER").unwrap_or_default(),
        kernel: read("/proc/sys/kernel/osrelease").trim().to_string(),
        uptime_secs: parse_uptime(&read("/proc/uptime")).unwrap_or(0),
        cpu,
        cores,
        mem_total,
        mem_available,
        gpus,
        disks,
        desktop: std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
        session: std::env::var("XDG_SESSION_TYPE").unwrap_or_default(),
        hyprland: hyprland_version(),
        shell: std::env::var("SHELL")
            .ok()
            .and_then(|s| Path::new(&s).file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_default(),
        packages: Path::new("/var/lib/pacman/local")
            .is_dir()
            .then(|| material_3_ui::sys::arch::installed_packages().len()),
        uefi: Path::new("/sys/firmware/efi").exists(),
    }
}

pub fn format_uptime(secs: u64) -> String {
    let days = secs / 86_400;
    let hours = secs % 86_400 / 3600;
    let mins = secs % 3600 / 60;
    match (days, hours) {
        (0, 0) => format!("{mins} min"),
        (0, h) => format!("{h} h {mins} min"),
        (d, h) => format!("{d} day{} {h} h", if d == 1 { "" } else { "s" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_files() {
        assert_eq!(parse_uptime("12345.67 99999.00\n"), Some(12345));
        let (cpu, n) = parse_cpuinfo("processor\t: 0\nmodel name\t: AMD Ryzen 7  7840U w/ Radeon  780M Graphics\nprocessor\t: 1\nmodel name\t: AMD Ryzen 7 7840U\n");
        assert_eq!(cpu, "AMD Ryzen 7 7840U w/ Radeon 780M Graphics");
        assert_eq!(n, 2);
        let (t, a) = parse_meminfo("MemTotal:       32000000 kB\nMemFree:  1 kB\nMemAvailable:   16000000 kB\n");
        assert_eq!(t, 32_000_000 * 1024);
        assert_eq!(a, 16_000_000 * 1024);
        assert_eq!(format_uptime(3 * 3600 + 120), "3 h 2 min");
        assert_eq!(format_uptime(90_000), "1 day 1 h");
    }

    #[test]
    fn lspci_and_df() {
        let l = "00:02.0 \"VGA compatible controller\" \"Intel Corporation\" \"Alder Lake-P GT2 [Iris Xe Graphics]\" -r0c \"Lenovo\" \"Device 3f1a\"\n00:14.0 \"USB controller\" \"Intel Corporation\" \"Alder Lake PCH USB\" \"\" \"\"\n01:00.0 \"3D controller\" \"NVIDIA Corporation\" \"GA107M [GeForce RTX 3050 Mobile]\" -ra1 \"\" \"\"\n";
        let g = parse_lspci(l);
        assert_eq!(g, ["Intel Alder Lake-P GT2 [Iris Xe Graphics]", "NVIDIA GA107M [GeForce RTX 3050 Mobile]"]);
        let d = parse_df("Mounted on        Used        Size\n/         1000 4000\n/home     2000 8000\n");
        assert_eq!(d.len(), 2);
        assert_eq!(d[1].mount, "/home");
        assert_eq!(d[1].total, 8000);
    }
}
