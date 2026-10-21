//! An open config file: the on-disk text, the edit buffer, saving with
//! backups, and noticing changes made by other programs.

use crate::formats::{self, Format};
use material_3_ui::fs::{self as m3fs, Backups, SaveError};
use material_3_ui::sys::paths;
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

    /// Write the buffer, through symlinks. With `backups`, the file's
    /// previous contents are copied to the backup folder first.
    pub fn save(&mut self, backups: bool) -> Result<(), String> {
        let bytes = self.text.as_bytes();
        let result = if backups && self.disk.is_some() {
            backup_store().save(&self.path, bytes)
        } else {
            m3fs::save(&self.path, bytes)
        };
        result.map_err(|e| match e {
            SaveError::NeedsRoot => "permission denied".to_string(),
            SaveError::Io(e) => e,
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

/// Where copies of config files go before they're overwritten: the newest
/// 20 per file, named after the file's full path and the time.
pub fn backup_store() -> Backups {
    Backups::new(paths::state_dir("m3-settings").join("backups"), 20)
}

/// Backups of `path`, newest first.
pub fn list_backups(path: &Path) -> Vec<PathBuf> {
    backup_store().list(path)
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
