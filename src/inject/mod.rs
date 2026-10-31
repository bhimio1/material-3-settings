//! Shell injection: adding the Material 3 integration to stock shells
//! instead of shipping a fork of one.
//!
//! Shells that run from a folder in `~/.config/quickshell` (end-4's ii,
//! pctrade's end4-pC, iNiR) get an `m3/` folder with our own QML, plus a few
//! [hooks] — marked edits to upstream files. Their updaters replace those
//! files, so a systemd path unit ([watcher]) runs `m3-settings inject
//! reapply` whenever a shell's `shell.qml` changes.
//!
//! Dux Shell (formerly DankMaterialShell) is installed system-wide but has
//! a plugin API, so it gets a plugin in `~/.config/dms/plugins/` instead.
//!
//! Packs live in `inject/<pack>/` in this repository and are embedded at
//! build time (see `build.rs`).

pub mod hooks;
pub mod watcher;

use hooks::Hook;
use material_3_ui::sys::cmd;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/packs.rs"));

/// Bumped when a pack changes, so installs know to re-apply.
pub const PACK_VERSION: u32 = 1;

/// The folder inside a shell that holds our files.
pub const M3_DIR: &str = "m3";
/// Dux Shell's plugin id and folder name.
pub const DUX_PLUGIN_ID: &str = "material3";
pub const DUX_PLUGIN_DIR: &str = "Material3";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Target {
    Ii,
    End4Pc,
    Inir,
    Dux,
}

impl Target {
    pub const ALL: [Target; 4] = [Target::Ii, Target::End4Pc, Target::Inir, Target::Dux];

    /// Stable id, used on the command line and for state files.
    pub fn id(self) -> &'static str {
        match self {
            Target::Ii => "ii",
            Target::End4Pc => "end4-pC",
            Target::Inir => "inir",
            Target::Dux => "dux",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Target::Ii => "illogical-impulse (end-4)",
            Target::End4Pc => "end4-pC (pctrade)",
            Target::Inir => "iNiR",
            Target::Dux => "Dux Shell",
        }
    }

    pub fn parse(s: &str) -> Option<Target> {
        match s.to_ascii_lowercase().as_str() {
            "ii" | "illogical-impulse" | "end-4" | "end4" => Some(Target::Ii),
            "end4-pc" | "end4pc" | "pctrade" => Some(Target::End4Pc),
            "inir" => Some(Target::Inir),
            "dux" | "dux-shell" | "dms" | "dankmaterialshell" => Some(Target::Dux),
            _ => None,
        }
    }

    /// The pack in `inject/` this shell uses.
    pub fn pack(self) -> &'static str {
        match self {
            Target::Ii | Target::End4Pc => "ii",
            Target::Inir => "inir",
            Target::Dux => "dux",
        }
    }

    /// Where we write: the shell's folder, or the Dux plugin folder.
    pub fn dir(self, home: &Path) -> PathBuf {
        match self {
            Target::Dux => home.join(".config/dms/plugins").join(DUX_PLUGIN_DIR),
            t => home.join(".config/quickshell").join(t.id()),
        }
    }

    pub fn is_installed(self, home: &Path) -> bool {
        match self {
            Target::Dux => {
                ["/usr/share/quickshell/dms", "/usr/local/share/quickshell/dms"]
                    .iter()
                    .map(PathBuf::from)
                    .chain([home.join(".config/quickshell/dms")])
                    .any(|d| d.join("shell.qml").is_file())
            }
            t => t.dir(home).join("shell.qml").is_file(),
        }
    }

    /// Files whose change means the shell was updated and needs us again.
    pub fn watch_paths(self, home: &Path) -> Vec<PathBuf> {
        match self {
            Target::Dux => Vec::new(),
            Target::Inir => vec![self.dir(home).join("shell.qml"), self.dir(home).join(".inir-manifest")],
            t => vec![t.dir(home).join("shell.qml")],
        }
    }
}

// ------------------------------------------------------------------ packs

/// Files of one pack, relative to the pack folder.
pub fn pack_files(pack: &str) -> Vec<(&'static str, &'static str)> {
    let prefix = format!("{pack}/");
    PACK_FILES
        .iter()
        .filter_map(|(p, c)| p.strip_prefix(prefix.as_str()).map(|rel| (rel, *c)))
        .collect()
}

pub fn pack_hooks(pack: &str) -> Result<Vec<Hook>, String> {
    match pack_files(pack).into_iter().find(|(p, _)| *p == "hooks.toml") {
        Some((_, text)) => hooks::parse(text),
        None => Ok(Vec::new()),
    }
}

// ------------------------------------------------------------------ state

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Skipped {
    pub hook: String,
    pub feature: String,
    pub reason: String,
}

/// What was last applied to a shell, in
/// `~/.local/state/m3-settings/inject/<shell>.json`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Record {
    pub pack_version: u32,
    /// Features that are in, and those left out because a hook missed.
    pub features: Vec<String>,
    pub skipped: Vec<Skipped>,
    /// Upstream files we edited, relative to the shell folder.
    pub files: Vec<String>,
    /// Unix seconds.
    pub applied_at: i64,
}

pub fn state_dir(home: &Path) -> PathBuf {
    // XDG_STATE_HOME only applies to the real home (tests pass a temp one).
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && home == material_3_ui::sys::paths::home())
        .unwrap_or_else(|| home.join(".local/state"))
        .join("m3-settings/inject")
}

fn record_path(home: &Path, t: Target) -> PathBuf {
    state_dir(home).join(format!("{}.json", t.id()))
}

fn backup_dir(home: &Path, t: Target) -> PathBuf {
    state_dir(home).join("backup").join(t.id())
}

pub fn read_record(home: &Path, t: Target) -> Option<Record> {
    serde_json::from_str(&std::fs::read_to_string(record_path(home, t)).ok()?).ok()
}

fn write_record(home: &Path, t: Target, r: &Record) -> Result<(), String> {
    let p = record_path(home, t);
    std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    write_if_changed(&p, &serde_json::to_string_pretty(r).unwrap())
}

/// Write only when the contents differ, so the path unit watching these
/// files doesn't fire on our own no-op writes.
fn write_if_changed(path: &Path, contents: &str) -> Result<(), String> {
    if std::fs::read_to_string(path).ok().as_deref() == Some(contents) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("m3-tmp");
    std::fs::write(&tmp, contents).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

// ------------------------------------------------------------------ status

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    NotInstalled,
    /// Installed, nothing of ours in it.
    Clean,
    /// Ours is in, from this pack version.
    Applied { pack_version: u32 },
    /// We applied before but the shell has since replaced our files
    /// (an update), or the pack in this build is newer.
    NeedsReapply,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub target: Target,
    pub state: State,
    pub record: Option<Record>,
}

pub fn status(home: &Path, t: Target) -> Status {
    let record = read_record(home, t);
    let state = if !t.is_installed(home) {
        State::NotInstalled
    } else {
        match &record {
            None => State::Clean,
            Some(r) if r.pack_version != PACK_VERSION || !is_in_place(home, t, r) => State::NeedsReapply,
            Some(r) => State::Applied { pack_version: r.pack_version },
        }
    };
    Status { target: t, state, record }
}

fn is_in_place(home: &Path, t: Target, r: &Record) -> bool {
    let dir = t.dir(home);
    if t == Target::Dux {
        return dir.join("plugin.json").is_file();
    }
    dir.join(M3_DIR).is_dir()
        && r.files.iter().all(|f| {
            std::fs::read_to_string(dir.join(f))
                .map(|text| hooks::has_markers(&text, hooks::comment_for(f)))
                .unwrap_or(false)
        })
}

// ------------------------------------------------------------------ apply

/// The result of planning hooks against the current upstream files.
#[derive(Debug, Default)]
pub struct Plan {
    /// New contents per edited file (relative path).
    pub edits: BTreeMap<String, String>,
    pub features: Vec<String>,
    pub skipped: Vec<Skipped>,
}

/// Work out the edits for a shell folder without writing anything.
///
/// A feature goes in only when every one of its hooks finds its line, so a
/// changed upstream never gets half a feature. Without `core` nothing goes in.
pub fn plan(shell_dir: &Path, shell: &str, hooks: &[Hook]) -> Result<Plan, String> {
    let hooks: Vec<&Hook> = hooks.iter().filter(|h| h.applies_to(shell)).collect();
    // Upstream text of every file a hook touches, with old blocks removed.
    let mut base: BTreeMap<String, String> = BTreeMap::new();
    for h in &hooks {
        if base.contains_key(&h.file) {
            continue;
        }
        let path = shell_dir.join(&h.file);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => hooks::strip(&t, hooks::comment_for(&h.file)).map_err(|e| format!("{}: {e}", h.file))?,
            Err(_) => String::new(),
        };
        base.insert(h.file.clone(), text);
    }

    // Features that miss a hook, checked hook by hook on upstream text so a
    // miss doesn't depend on what other hooks inserted.
    let mut skipped = Vec::new();
    let mut failed: BTreeSet<String> = BTreeSet::new();
    for h in &hooks {
        let text = &base[&h.file];
        let reason = if !shell_dir.join(&h.file).is_file() {
            Some(format!("{} doesn't exist in this version", h.file))
        } else {
            hooks::apply(text, h).err().map(|m| format!("{}: {m}", h.file))
        };
        if let Some(reason) = reason {
            failed.insert(h.feature.clone());
            skipped.push(Skipped { hook: h.id.clone(), feature: h.feature.clone(), reason });
        }
    }
    if failed.contains("core") {
        failed.extend(hooks.iter().map(|h| h.feature.clone()));
    }

    let mut edits = base.clone();
    let mut features = BTreeSet::new();
    for h in &hooks {
        if failed.contains(&h.feature) {
            continue;
        }
        let text = edits.get_mut(&h.file).unwrap();
        // Checked above against the stripped base; inserting other hooks
        // can only make an anchor ambiguous if it matches our own text.
        match hooks::apply(text, h) {
            Ok(t) => *text = t,
            Err(m) => return Err(format!("hook {} stopped matching after other hooks: {m}", h.id)),
        }
        features.insert(h.feature.clone());
    }
    // Keep files whose text changed, plus stripped files that had blocks,
    // so a now-skipped hook's old block goes away.
    edits.retain(|f, text| std::fs::read_to_string(shell_dir.join(f)).ok().as_deref() != Some(text.as_str()));
    Ok(Plan { edits, features: features.into_iter().collect(), skipped })
}

#[derive(Debug, Default, PartialEq)]
pub struct Report {
    pub features: Vec<String>,
    pub skipped: Vec<Skipped>,
    /// Files written (edited upstream files and our own).
    pub written: usize,
}

impl Report {
    pub fn summary(&self, t: Target) -> String {
        if self.features.is_empty() {
            return format!("Nothing could be added to {}: {}", t.label(), self.skipped.first().map_or("no hooks", |s| s.reason.as_str()));
        }
        let mut s = format!("Material 3 integration is in {}", t.label());
        if !self.skipped.is_empty() {
            let left: BTreeSet<&str> = self.skipped.iter().map(|s| s.feature.as_str()).collect();
            s.push_str(&format!(" (left out: {})", left.into_iter().collect::<Vec<_>>().join(", ")));
        }
        s
    }
}

/// Add (or refresh) the Material 3 integration in a shell.
pub fn apply(home: &Path, t: Target) -> Result<Report, String> {
    if !t.is_installed(home) {
        return Err(format!("{} isn't installed", t.label()));
    }
    if t == Target::Dux {
        return apply_dux(home);
    }
    let dir = t.dir(home);
    let hooks = pack_hooks(t.pack())?;
    let plan = plan(&dir, t.id(), &hooks)?;
    if !plan.features.iter().any(|f| f == "core") {
        return Ok(Report { features: Vec::new(), skipped: plan.skipped, written: 0 });
    }

    // Back up the upstream files we're about to edit (as upstream, without
    // our blocks), for `remove` when markers were damaged, and for undoing
    // an apply that stops the shell from starting.
    let bdir = backup_dir(home, t);
    let _ = std::fs::remove_dir_all(&bdir);
    for f in plan.edits.keys() {
        if let Ok(text) = std::fs::read_to_string(dir.join(f)) {
            let clean = hooks::strip(&text, hooks::comment_for(f)).unwrap_or(text);
            write_if_changed(&bdir.join(f), &clean)?;
        }
    }

    let mut written = 0;
    // Our folder first, so the hooks never point at files that aren't there.
    written += sync_m3_dir(&dir.join(M3_DIR), &pack_files(t.pack()))?;
    for (f, text) in &plan.edits {
        write_if_changed(&dir.join(f), text)?;
        written += 1;
    }
    let files: BTreeSet<String> = hooks
        .iter()
        .filter(|h| h.applies_to(t.id()) && plan.features.contains(&h.feature))
        .map(|h| h.file.clone())
        .collect();
    write_record(
        home,
        t,
        &Record {
            pack_version: PACK_VERSION,
            features: plan.features.clone(),
            skipped: plan.skipped.clone(),
            files: files.into_iter().collect(),
            applied_at: chrono::Utc::now().timestamp(),
        },
    )?;
    Ok(Report { features: plan.features, skipped: plan.skipped, written })
}

/// Make `dest` hold exactly the pack's `m3/` files.
fn sync_m3_dir(dest: &Path, files: &[(&str, &str)]) -> Result<usize, String> {
    let prefix = format!("{M3_DIR}/");
    let ours: BTreeMap<&str, &str> = files.iter().filter_map(|(p, c)| p.strip_prefix(prefix.as_str()).map(|r| (r, *c))).collect();
    let mut written = 0;
    for (rel, contents) in &ours {
        let p = dest.join(rel);
        if std::fs::read_to_string(&p).ok().as_deref() != Some(*contents) {
            write_if_changed(&p, contents)?;
            written += 1;
        }
    }
    // Drop files an older pack had.
    let mut stack = vec![dest.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(rel) = p.strip_prefix(dest) {
                if !ours.contains_key(rel.to_string_lossy().as_ref()) {
                    let _ = std::fs::remove_file(&p);
                }
            }
        }
    }
    Ok(written)
}

/// Take the integration out again, leaving upstream's files as they were.
pub fn remove(home: &Path, t: Target) -> Result<String, String> {
    if t == Target::Dux {
        return remove_dux(home);
    }
    let dir = t.dir(home);
    let mut files: BTreeSet<String> = read_record(home, t).map(|r| r.files.into_iter().collect()).unwrap_or_default();
    files.extend(pack_hooks(t.pack())?.into_iter().map(|h| h.file));
    let bdir = backup_dir(home, t);
    for f in &files {
        let path = dir.join(f);
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let comment = hooks::comment_for(f);
        if !hooks::has_markers(&text, comment) {
            continue;
        }
        let clean = match hooks::strip(&text, comment) {
            Ok(c) => c,
            Err(e) => std::fs::read_to_string(bdir.join(f)).map_err(|_| format!("{f}: {e}, and there's no backup"))?,
        };
        write_if_changed(&path, &clean)?;
    }
    let m3 = dir.join(M3_DIR);
    if m3.is_dir() {
        std::fs::remove_dir_all(&m3).map_err(|e| format!("{}: {e}", m3.display()))?;
    }
    let _ = std::fs::remove_file(record_path(home, t));
    let _ = std::fs::remove_dir_all(bdir);
    Ok(format!("Material 3 integration removed from {}", t.label()))
}

/// Put back the files saved before the last apply (used when the shell
/// stopped starting after it).
pub fn restore_backup(home: &Path, t: Target) -> Result<(), String> {
    let dir = t.dir(home);
    let bdir = backup_dir(home, t);
    let Some(r) = read_record(home, t) else { return Ok(()) };
    for f in &r.files {
        if let Ok(text) = std::fs::read_to_string(bdir.join(f)) {
            write_if_changed(&dir.join(f), &text)?;
        }
    }
    let _ = std::fs::remove_dir_all(dir.join(M3_DIR));
    let _ = std::fs::remove_file(record_path(home, t));
    Ok(())
}

/// Re-apply to every shell we applied to before whose files were replaced
/// (or whose pack is older). Returns one line per shell touched.
pub fn reapply_all(home: &Path) -> Vec<(Target, Result<Report, String>)> {
    Target::ALL
        .into_iter()
        .filter(|t| status(home, *t).state == State::NeedsReapply)
        .map(|t| (t, apply(home, t)))
        .collect()
}

// ------------------------------------------------------------------ verify

/// Running Quickshell configs (`qs -c <name>` / `-p <dir>`) by folder name.
pub fn running_shells() -> Vec<String> {
    let mut pgrep = String::new();
    for name in ["qs", "quickshell"] {
        if let Ok(out) = cmd::run("pgrep", &["-a", "-x", name]) {
            pgrep.push_str(&out);
        }
    }
    crate::end4::parse_running(&pgrep)
}

/// After applying to a running shell, make sure it's still running; if it
/// died, put the backups back and start it again. Quickshell keeps the old
/// config when a hot reload fails, so this mostly catches crashes.
pub fn verify_running(home: &Path, t: Target, was_running: bool) -> Result<(), String> {
    if t == Target::Dux || !was_running {
        return Ok(());
    }
    std::thread::sleep(std::time::Duration::from_secs(4));
    if running_shells().iter().any(|s| s == t.id()) {
        return Ok(());
    }
    restore_backup(home, t)?;
    let _ = cmd::spawn_detached("qs", &["-c", t.id()]);
    Err(format!("{} stopped after the change, so it was undone", t.label()))
}

// ------------------------------------------------------------------ Dux Shell

fn dux_plugin_settings(home: &Path) -> PathBuf {
    home.join(".config/dms/plugin_settings.json")
}

/// Set (or clear) `material3.enabled` in Dux Shell's plugin settings,
/// keeping everything else.
pub fn set_dux_enabled(text: &str, enabled: bool) -> Result<String, String> {
    let mut v: serde_json::Value = if text.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(text).map_err(|e| format!("plugin_settings.json: {e}"))?
    };
    let obj = v.as_object_mut().ok_or("plugin_settings.json isn't an object")?;
    if enabled {
        let entry = obj.entry(DUX_PLUGIN_ID).or_insert_with(|| serde_json::json!({}));
        entry.as_object_mut().ok_or("plugin entry isn't an object")?.insert("enabled".into(), true.into());
    } else {
        obj.remove(DUX_PLUGIN_ID);
    }
    Ok(serde_json::to_string_pretty(&v).unwrap() + "\n")
}

fn apply_dux(home: &Path) -> Result<Report, String> {
    let dir = Target::Dux.dir(home);
    let files: Vec<(&str, &str)> = pack_files("dux")
        .into_iter()
        .filter_map(|(p, c)| p.strip_prefix(&format!("{DUX_PLUGIN_DIR}/")).map(|r| (r, c)))
        .collect();
    if files.is_empty() {
        return Err("This build has no Dux Shell plugin".into());
    }
    let mut written = 0;
    for (rel, c) in &files {
        let p = dir.join(rel);
        if std::fs::read_to_string(&p).ok().as_deref() != Some(*c) {
            write_if_changed(&p, c)?;
            written += 1;
        }
    }
    let settings = dux_plugin_settings(home);
    let text = std::fs::read_to_string(&settings).unwrap_or_default();
    write_if_changed(&settings, &set_dux_enabled(&text, true)?)?;
    // Live shells pick up the plugin on a rescan; harmless when not running.
    let _ = cmd::run("dms", &["ipc", "call", "plugin-scan", "scan"]);
    let features = vec!["core".to_string()];
    write_record(
        home,
        Target::Dux,
        &Record { pack_version: PACK_VERSION, features: features.clone(), skipped: Vec::new(), files: Vec::new(), applied_at: chrono::Utc::now().timestamp() },
    )?;
    Ok(Report { features, skipped: Vec::new(), written })
}

fn remove_dux(home: &Path) -> Result<String, String> {
    let settings = dux_plugin_settings(home);
    if let Ok(text) = std::fs::read_to_string(&settings) {
        write_if_changed(&settings, &set_dux_enabled(&text, false)?)?;
    }
    let dir = Target::Dux.dir(home);
    if dir.is_dir() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let _ = std::fs::remove_file(record_path(home, Target::Dux));
    let _ = cmd::run("dms", &["ipc", "call", "plugin-scan", "scan"]);
    Ok(format!("Material 3 plugin removed from {}", Target::Dux.label()))
}

#[cfg(test)]
mod tests;
