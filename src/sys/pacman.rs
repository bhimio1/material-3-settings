//! pacman: package statistics, updates, the package cache, searching, and
//! editing `/etc/pacman.conf`.

use super::cmd;
use crate::formats::{self, kv::Dialect};
use std::path::Path;

pub const PACMAN_CONF: &str = "/etc/pacman.conf";
pub const MIRRORLIST: &str = "/etc/pacman.d/mirrorlist";
pub const CACHE_DIR: &str = "/var/cache/pacman/pkg";

fn count_lines(s: &str) -> usize {
    s.lines().filter(|l| !l.trim().is_empty()).count()
}

/// `pacman -Q…q` style output as a list of names. pacman exits with 1 when
/// a query matches nothing, which is not an error here.
fn names(args: &[&str]) -> Result<Vec<String>, String> {
    match cmd::run("pacman", args) {
        Ok(out) => Ok(out
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect()),
        Err(e) if e.contains("exited with") => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub total: usize,
    pub explicit: usize,
    pub foreign: Vec<String>,
    pub orphans: Vec<String>,
    pub cache_bytes: u64,
    pub cache_files: usize,
}

pub fn load_stats() -> Result<Stats, String> {
    let total = count_lines(&cmd::run("pacman", &["-Qq"])?);
    let (cache_bytes, cache_files) = dir_size(Path::new(CACHE_DIR));
    Ok(Stats {
        total,
        explicit: names(&["-Qeq"])?.len(),
        foreign: names(&["-Qmq"])?,
        orphans: names(&["-Qdtq"])?,
        cache_bytes,
        cache_files,
    })
}

/// Total size and number of regular files below `dir`.
pub fn dir_size(dir: &Path) -> (u64, usize) {
    let mut bytes = 0;
    let mut files = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push(e.path());
            } else if ft.is_file() {
                if let Ok(m) = e.metadata() {
                    bytes += m.len();
                    files += 1;
                }
            }
        }
    }
    (bytes, files)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    pub name: String,
    pub old: String,
    pub new: String,
    pub aur: bool,
}

/// Parse `checkupdates` / `paru -Qua` lines: `name old -> new`.
pub fn parse_updates(text: &str, aur: bool) -> Vec<Update> {
    text.lines()
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            let name = parts.next()?;
            let old = parts.next()?;
            if parts.next()? != "->" {
                return None;
            }
            let new = parts.next()?;
            Some(Update {
                name: name.to_string(),
                old: old.to_string(),
                new: new.to_string(),
                aur,
            })
        })
        .collect()
}

/// Pending updates from the official repos (`checkupdates`, which syncs a
/// private copy of the database) plus the AUR through the helper.
pub fn load_updates(helper: Option<&str>) -> Result<Vec<Update>, String> {
    let mut out = match cmd::run("checkupdates", &[]) {
        Ok(text) => parse_updates(&text, false),
        // checkupdates exits 2 when there are no updates.
        Err(e) if e.contains("exit status: 2") || e.trim().is_empty() => Vec::new(),
        Err(e) if e.contains("exited with") => Vec::new(),
        Err(e) => return Err(e),
    };
    if let Some(h) = helper {
        if let Ok(text) = cmd::run(h, &["-Qua"]) {
            out.extend(parse_updates(&text, true));
        }
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchResult {
    pub repo: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub installed: bool,
}

/// Parse `pacman -Ss` / `pacman -Qs` output: a header line per package
/// followed by an indented description.
pub fn parse_search(text: &str) -> Vec<SearchResult> {
    let mut out: Vec<SearchResult> = Vec::new();
    for line in text.lines() {
        if line.starts_with(char::is_whitespace) {
            if let Some(last) = out.last_mut() {
                if !last.description.is_empty() {
                    last.description.push(' ');
                }
                last.description.push_str(line.trim());
            }
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(full) = parts.next() else { continue };
        let Some((repo, name)) = full.split_once('/') else {
            continue;
        };
        let version = parts.next().unwrap_or_default().to_string();
        let installed = repo == "local" || line.contains("[installed");
        out.push(SearchResult {
            repo: repo.to_string(),
            name: name.to_string(),
            version,
            description: String::new(),
            installed,
        });
    }
    out
}

pub fn search(query: &str, installed_only: bool) -> Result<Vec<SearchResult>, String> {
    let terms: Vec<&str> = query.split_whitespace().collect();
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = vec![if installed_only { "-Qs" } else { "-Ss" }];
    args.push("--");
    args.extend(terms);
    match cmd::run("pacman", &args) {
        Ok(text) => Ok(parse_search(&text)),
        Err(e) if e.contains("exited with") => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

// ------------------------------------------------------------ pacman.conf

pub const FLAGS: &[(&str, &str, &str)] = &[
    ("Color", "Colored output", "Highlight package names, versions and warnings."),
    (
        "ILoveCandy",
        "Pac-Man progress bar",
        "Replace the download progress bar with Pac-Man eating dots.",
    ),
    (
        "VerbosePkgLists",
        "Detailed transaction lists",
        "Show old and new versions in a table before upgrading.",
    ),
    (
        "CheckSpace",
        "Check disk space",
        "Make sure there is enough room before installing.",
    ),
];

pub fn flag(text: &str, name: &str) -> bool {
    formats::get_flag(text, &Dialect::INI, &format!("options.{name}"))
}

pub fn set_flag(text: &str, name: &str, on: bool) -> String {
    formats::set_flag(text, &Dialect::INI, &format!("options.{name}"), on)
}

pub fn parallel_downloads(text: &str) -> Option<u32> {
    formats::kv::get(text, &Dialect::INI, "options.ParallelDownloads")?
        .trim()
        .parse()
        .ok()
}

pub fn set_parallel_downloads(text: &str, n: Option<u32>) -> String {
    match n {
        Some(n) => formats::kv::set(text, &Dialect::INI, "options.ParallelDownloads", &n.to_string()),
        None => formats::kv::unset(text, &Dialect::INI, "options.ParallelDownloads"),
    }
}

fn is_header(line: &str, name: &str) -> bool {
    line.trim() == format!("[{name}]")
}

fn is_commented_header(line: &str, name: &str) -> bool {
    let t = line.trim();
    t.starts_with('#') && t.trim_start_matches('#').trim() == format!("[{name}]")
}

/// Is the `[name]` repository enabled?
pub fn repo_enabled(text: &str, name: &str) -> bool {
    text.lines().any(|l| is_header(l, name))
}

/// Enabled repositories in order (every active section except `[options]`).
pub fn repos(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let t = l.trim();
            (t.starts_with('[') && t.ends_with(']')).then(|| t[1..t.len() - 1].trim().to_string())
        })
        .filter(|name| name != "options")
        .collect()
}

/// CachyOS's own repositories (`cachyos`, `cachyos-v3`, `cachyos-core-znver4`, …).
pub fn cachyos_repos(text: &str) -> Vec<String> {
    repos(text).into_iter().filter(|r| r.starts_with("cachyos")).collect()
}

/// Enable or disable a repository section such as `[multilib]` by
/// (un)commenting its header and the option lines directly below it, up to
/// the next blank line or section.
pub fn set_repo_enabled(text: &str, name: &str, on: bool) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let header = lines.iter().position(|l| {
        if on {
            is_commented_header(l, name) || is_header(l, name)
        } else {
            is_header(l, name)
        }
    });
    match header {
        Some(h) => {
            let mut i = h;
            loop {
                let t = lines[i].trim().to_string();
                if i > h && (t.is_empty() || t.trim_start_matches('#').trim_start().starts_with('[')) {
                    break;
                }
                // Only touch "Key = value" option lines (and the header);
                // leave prose comments alone.
                let body = t.trim_start_matches('#').trim();
                let is_option = i == h || body.contains('=');
                if is_option {
                    lines[i] = if on {
                        body.to_string()
                    } else if t.starts_with('#') {
                        t
                    } else {
                        format!("#{t}")
                    };
                }
                i += 1;
                if i >= lines.len() {
                    break;
                }
            }
        }
        None if on => {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(format!("[{name}]"));
            lines.push("Include = /etc/pacman.d/mirrorlist".into());
        }
        None => {}
    }
    let mut out = lines.join("\n");
    if text.ends_with('\n') || text.is_empty() {
        out.push('\n');
    }
    out
}

/// Active `Server = …` lines of the mirrorlist.
pub fn mirrors(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("Server"))
        .filter_map(|l| l.trim_start().strip_prefix('='))
        .map(|s| s.trim().to_string())
        .collect()
}

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONF: &str = "#\n# /etc/pacman.conf\n#\n[options]\nHoldPkg     = pacman glibc\nArchitecture = auto\n\n#UseSyslog\n#Color\n#NoProgressBar\nCheckSpace\n#VerbosePkgLists\nParallelDownloads = 5\n\n[core]\nInclude = /etc/pacman.d/mirrorlist\n\n# If you want to run 32 bit applications on your x86_64 system,\n# enable the multilib repositories as required here.\n\n#[multilib-testing]\n#Include = /etc/pacman.d/mirrorlist\n\n#[multilib]\n#Include = /etc/pacman.d/mirrorlist\n\n# An example of a custom package repository.\n#[custom]\n#SigLevel = Optional TrustAll\n#Server = file:///home/custompkgs\n";

    #[test]
    fn options() {
        assert!(!flag(CONF, "Color"));
        assert!(flag(CONF, "CheckSpace"));
        let t = set_flag(CONF, "Color", true);
        assert!(flag(&t, "Color"));
        assert!(t.contains("\nColor\n"));
        let t = set_flag(&t, "ILoveCandy", true);
        assert!(flag(&t, "ILoveCandy"));
        // Added inside [options], not at the end of the file.
        assert!(t.find("ILoveCandy").unwrap() < t.find("[core]").unwrap());
        assert_eq!(parallel_downloads(CONF), Some(5));
        let t = set_parallel_downloads(CONF, Some(10));
        assert_eq!(parallel_downloads(&t), Some(10));
    }

    #[test]
    fn multilib_toggle() {
        assert!(!repo_enabled(CONF, "multilib"));
        let on = set_repo_enabled(CONF, "multilib", true);
        assert!(repo_enabled(&on, "multilib"));
        assert!(on.contains("\n[multilib]\nInclude = /etc/pacman.d/mirrorlist\n"));
        // multilib-testing and the custom example stay commented.
        assert!(on.contains("#[multilib-testing]\n#Include"));
        assert!(on.contains("#[custom]\n#SigLevel"));
        let off = set_repo_enabled(&on, "multilib", false);
        assert_eq!(off, CONF);
        // Missing entirely: appended.
        let added = set_repo_enabled("[options]\n", "multilib", true);
        assert_eq!(added, "[options]\n\n[multilib]\nInclude = /etc/pacman.d/mirrorlist\n");
    }

    #[test]
    fn updates_and_search() {
        let u = parse_updates("linux 6.11.1.arch1-1 -> 6.11.2.arch1-1\nbogus line\n", false);
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].new, "6.11.2.arch1-1");

        let s = parse_search("core/linux 6.11.1.arch1-1 [installed]\n    The Linux kernel and modules\nextra/kitty 0.36.4-1 (terminal) [installed: 0.36.3-1]\n    A modern, hackable, featureful,\n    OpenGL-based terminal emulator\nextra/foot 1.18.1-1\n    Wayland terminal emulator\n");
        assert_eq!(s.len(), 3);
        assert!(s[0].installed && s[1].installed && !s[2].installed);
        assert_eq!(s[1].description, "A modern, hackable, featureful, OpenGL-based terminal emulator");
        assert_eq!(s[2].repo, "extra");
    }

    #[test]
    fn repo_list() {
        let conf = "[options]\nColor\n\n[cachyos-v3]\nInclude = /etc/pacman.d/cachyos-v3-mirrorlist\n\n[cachyos-core-v3]\nInclude = x\n\n[core]\nInclude = /etc/pacman.d/mirrorlist\n\n#[multilib]\n#Include = /etc/pacman.d/mirrorlist\n";
        assert_eq!(repos(conf), vec!["cachyos-v3", "cachyos-core-v3", "core"]);
        assert_eq!(cachyos_repos(conf), vec!["cachyos-v3", "cachyos-core-v3"]);
    }

    #[test]
    fn mirrorlist() {
        let m = mirrors("## Germany\n#Server = https://a/$repo\nServer = https://b/$repo/os/$arch\n");
        assert_eq!(m, ["https://b/$repo/os/$arch"]);
        assert_eq!(format_size(1536), "1.5 KiB");
        assert_eq!(format_size(12), "12 B");
    }
}
