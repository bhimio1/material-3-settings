//! An open config file: the on-disk text, the edit buffer, saving with
//! backups, and noticing changes made by other programs.

use crate::formats::{self, Format};
use crate::sys::paths;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct ConfigFile {
    pub path: PathBuf,
    pub format: Format,
    /// The edit buffer.
    pub text: String,
    /// What is on disk, as of the last load or save. `None` = no file.
    pub disk: Option<String>,
    mtime: Option<SystemTime>,
    pub load_error: Option<String>,
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

impl ConfigFile {
    pub fn open(path: PathBuf, format: Format) -> Self {
        let mut f = Self {
            path,
            format,
            text: String::new(),
            disk: None,
            mtime: None,
            load_error: None,
        };
        f.reload();
        f
    }

    pub fn reload(&mut self) {
        self.load_error = None;
        match std::fs::read_to_string(&self.path) {
            Ok(text) => {
                self.text = text.clone();
                self.disk = Some(text);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.text.clear();
                self.disk = None;
            }
            Err(e) => {
                self.load_error = Some(e.to_string());
                self.disk = None;
            }
        }
        self.mtime = mtime(&self.path);
    }

    pub fn exists(&self) -> bool {
        self.disk.is_some()
    }

    pub fn dirty(&self) -> bool {
        match &self.disk {
            Some(d) => *d != self.text,
            None => !self.text.is_empty(),
        }
    }

    /// The file was changed by something else since we loaded it.
    pub fn changed_on_disk(&self) -> bool {
        mtime(&self.path) != self.mtime
    }

    /// Reload if changed on disk and there are no unsaved edits. Returns
    /// true when the file was reloaded.
    pub fn sync_from_disk(&mut self) -> bool {
        if self.changed_on_disk() && !self.dirty() {
            self.reload();
            true
        } else {
            false
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        self.format.validate(&self.text)
    }

    pub fn save(&mut self, backups: bool) -> Result<(), String> {
        // Write through symlinks (stow/chezmoi-managed dotfiles) instead of
        // replacing the link with a regular file.
        let target = std::fs::canonicalize(&self.path).unwrap_or_else(|_| self.path.clone());
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if backups && self.disk.is_some() {
            backup(&target).map_err(|e| format!("couldn't back up: {e}"))?;
        }
        let tmp = target.with_extension(format!(
            "{}m3s-tmp",
            target
                .extension()
                .map(|e| format!("{}.", e.to_string_lossy()))
                .unwrap_or_default()
        ));
        std::fs::write(&tmp, &self.text).map_err(|e| e.to_string())?;
        if let Ok(meta) = std::fs::metadata(&target) {
            let _ = std::fs::set_permissions(&tmp, meta.permissions());
        }
        std::fs::rename(&tmp, &target).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            e.to_string()
        })?;
        self.disk = Some(self.text.clone());
        self.mtime = mtime(&self.path);
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        formats::get(self.format, &self.text, key)
    }

    /// Change one setting in the buffer.
    pub fn set(&mut self, key: &str, value: &Value) -> Result<(), String> {
        self.text = formats::set(self.format, &self.text, key, value)?;
        Ok(())
    }

    pub fn unset(&mut self, key: &str) -> Result<(), String> {
        self.text = formats::unset(self.format, &self.text, key)?;
        Ok(())
    }
}

fn backup_dir() -> PathBuf {
    paths::state_dir().join("backups")
}

/// Backups of `path` are named after its full path so files with the same
/// name in different folders don't collide.
fn backup_stem(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches('/')
        .replace('/', "%")
}

/// Copy `path` into the backup folder with a timestamp. Keeps the newest 20.
pub fn backup(path: &Path) -> std::io::Result<PathBuf> {
    let dir = backup_dir();
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S%.3f");
    let dest = dir.join(format!("{}@{stamp}", backup_stem(path)));
    std::fs::copy(path, &dest)?;
    let old = list_backups(path);
    for stale in old.iter().skip(20) {
        let _ = std::fs::remove_file(stale);
    }
    Ok(dest)
}

/// Backups of `path`, newest first.
pub fn list_backups(path: &Path) -> Vec<PathBuf> {
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let prefix = format!("{}@", backup_stem(&target));
    let mut found: Vec<PathBuf> = std::fs::read_dir(backup_dir())
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with(&prefix))
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found.reverse();
    found
}

/// "2026-10-20 14:03" from a backup file name.
pub fn backup_label(backup: &Path) -> String {
    let name = backup.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let stamp = name.rsplit('@').next().unwrap_or_default();
    chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%d-%H%M%S%.3f")
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|_| stamp.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::kv::Dialect;

    #[test]
    fn edit_save_and_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.conf");
        std::fs::write(&real, "font_size 11\n").unwrap();
        let link = dir.path().join("kitty.conf");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let mut f = ConfigFile::open(link.clone(), Format::Kv(Dialect::SPACE));
        assert!(f.exists() && !f.dirty());
        f.set("font_size", &Value::String("13".into())).unwrap();
        assert!(f.dirty());
        f.save(false).unwrap();
        assert!(!f.dirty());
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "font_size 13\n");
    }

    #[test]
    fn missing_file_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/dir/config.toml");
        let mut f = ConfigFile::open(path.clone(), Format::Toml);
        assert!(!f.exists());
        f.set("font.size", &serde_json::json!(12)).unwrap();
        f.save(true).unwrap();
        assert!(path.is_file());
        assert_eq!(f.get("font.size"), Some(serde_json::json!(12)));
    }
}
