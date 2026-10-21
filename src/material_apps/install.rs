//! Installing, updating and removing. The release install is a port of the
//! end4-pC shell's `scripts/material-apps/install.sh` and leaves exactly the
//! same files behind. Building from source and anything that needs a
//! password run in a terminal instead (see the `*_script` functions).

use super::release::{agent, Release};
use super::{Dirs, MaterialApp};
use crate::sys::{cmd, term::shell_quote};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Progress reports from an install running on a background thread.
#[derive(Clone, Debug)]
pub enum Event {
    Stage(String),
    /// Download progress, 0–1 (only when the size is known).
    Progress(f32),
    Done(Result<String, String>),
}

/// Rewrite `Exec=<bin>` lines to the absolute path of the installed binary,
/// in case `~/.local/bin` isn't on the `PATH` launchers see. Same as the
/// script's `sed -E "s|^Exec=$bin( |$)|Exec=$BIN_DIR/$bin\1|"`.
pub fn rewrite_exec(desktop: &str, bin: &str, bin_dir: &Path) -> String {
    let prefix = format!("Exec={bin}");
    let mut out: String = desktop
        .lines()
        .map(|line| match line.strip_prefix(&prefix) {
            Some(rest) if rest.is_empty() || rest.starts_with(' ') => {
                format!("Exec={}{rest}", bin_dir.join(bin).display())
            }
            _ => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    if desktop.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn download(url: &str, to: &Path, mut progress: impl FnMut(f32)) -> Result<(), String> {
    let resp = agent(url, Duration::from_secs(60))
        .get(url)
        .call()
        .map_err(|e| format!("Download failed: {e}"))?;
    let total: Option<u64> = resp.header("content-length").and_then(|s| s.parse().ok());
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(to).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("Download failed: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        if let Some(t) = total.filter(|t| *t > 0) {
            progress(done as f32 / t as f32);
        }
    }
    file.flush().map_err(|e| e.to_string())
}

/// Check a file against a `sha256sum` line (`<hex>  <name>`).
pub fn verify_sha256(file: &Path, sums: &str) -> Result<(), String> {
    let expected = sums
        .split_whitespace()
        .next()
        .filter(|h| h.len() == 64)
        .ok_or("The checksum file is malformed")?
        .to_lowercase();
    let mut hasher = Sha256::new();
    let mut f = std::fs::File::open(file).map_err(|e| e.to_string())?;
    std::io::copy(&mut f, &mut hasher).map_err(|e| e.to_string())?;
    let actual: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if actual == expected {
        Ok(())
    } else {
        Err("The download doesn't match its checksum, so it wasn't installed.".into())
    }
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Put the contents of an unpacked release (`<bin>-x86_64-linux/`) in place
/// and record the version.
pub fn install_unpacked(app: &MaterialApp, dirs: &Dirs, unpacked: &Path, tag: &str) -> Result<(), String> {
    let err = |what: &str, e: std::io::Error| format!("Couldn't {what}: {e}");
    let binary = unpacked.join(app.bin);
    if !binary.is_file() {
        return Err(format!("The release doesn't contain {}", app.bin));
    }
    std::fs::create_dir_all(&dirs.bin).map_err(|e| err("create ~/.local/bin", e))?;
    // Copy next to the target and rename over it: replacing a running binary
    // in place fails ("text file busy"), and Settings updates itself.
    let target = dirs.binary(app);
    let tmp = dirs.bin.join(format!(".{}.new", app.bin));
    std::fs::copy(&binary, &tmp).map_err(|e| err("copy the binary", e))?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| err("make the binary executable", e))?;
    }
    std::fs::rename(&tmp, &target).map_err(|e| err("install the binary", e))?;

    let desktop = unpacked.join(format!("{}.desktop", app.desktop_id));
    if let Ok(text) = std::fs::read_to_string(&desktop) {
        std::fs::create_dir_all(dirs.applications()).map_err(|e| err("create the applications folder", e))?;
        std::fs::write(dirs.desktop_file(app), rewrite_exec(&text, app.bin, &dirs.bin))
            .map_err(|e| err("write the desktop entry", e))?;
        if cmd::exists("update-desktop-database") {
            let _ = cmd::run("update-desktop-database", &["-q", &dirs.applications().to_string_lossy()]);
        }
    }
    let share = unpacked.join("share");
    if share.is_dir() {
        copy_tree(&share, &dirs.data).map_err(|e| err("copy the app's data files", e))?;
    }
    write_state(app, dirs, tag).map_err(|e| err("record the installed version", e))
}

fn write_state(app: &MaterialApp, dirs: &Dirs, version: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(&dirs.state)?;
    std::fs::write(dirs.state_file(app, "version"), format!("{version}\n"))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    std::fs::write(dirs.state_file(app, "checked"), format!("{now}\n"))?;
    let _ = std::fs::remove_file(dirs.state_file(app, "failed"));
    Ok(())
}

/// Download, verify and install a release. Runs on a background thread and
/// reports through `events`.
pub fn install_release(app: &MaterialApp, dirs: &Dirs, release: &Release, events: &flume::Sender<Event>) -> Result<String, String> {
    let url = release
        .tarball
        .as_deref()
        .ok_or("This release has no x86_64 Linux build")?;
    let work = dirs.cache.join(format!("{}-download", app.bin));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let result = (|| {
        let tarball = work.join(format!("{}-x86_64-linux.tar.gz", app.bin));
        let _ = events.send(Event::Stage(format!("Downloading {} {}…", app.name, release.tag)));
        download(url, &tarball, |f| {
            let _ = events.send(Event::Progress(f));
        })?;
        if let Some(sum_url) = &release.checksum {
            let _ = events.send(Event::Stage("Checking the download…".into()));
            let sums = agent(sum_url, Duration::from_secs(20))
                .get(sum_url)
                .call()
                .map_err(|e| format!("Couldn't download the checksum: {e}"))?
                .into_string()
                .map_err(|e| e.to_string())?;
            verify_sha256(&tarball, &sums)?;
        }
        let _ = events.send(Event::Stage("Installing…".into()));
        cmd::run("tar", &["-xzf", &tarball.to_string_lossy(), "-C", &work.to_string_lossy()])
            .map_err(|e| format!("Couldn't unpack the download: {e}"))?;
        install_unpacked(app, dirs, &work.join(format!("{}-x86_64-linux", app.bin)), &release.tag)?;
        Ok(format!("{} {} installed", app.name, release.tag))
    })();
    let _ = std::fs::remove_dir_all(&work);
    if result.is_err() {
        let _ = std::fs::create_dir_all(&dirs.state);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = std::fs::write(dirs.state_file(app, "failed"), format!("{now}\n"));
    }
    result
}

/// A shell script (for a terminal) that clones the project, builds it with
/// cargo and installs it like a release, recording `source-<commit>`.
pub fn source_script(app: &MaterialApp, dirs: &Dirs) -> String {
    let q = |p: &Path| shell_quote(&p.to_string_lossy());
    let src = dirs.cache.join(format!("{}-src", app.bin));
    let desktop = dirs.desktop_file(app);
    let mut s = format!(
        "(set -e; \
         command -v cargo >/dev/null || {{ echo 'Building needs cargo: sudo pacman -S rust'; exit 1; }}; \
         command -v git >/dev/null || {{ echo 'Building needs git: sudo pacman -S git'; exit 1; }}; \
         rm -rf {src}; git clone --depth 1 https://github.com/{repo}.git {src}; cd {src}; \
         echo 'Building {name}. This takes a few minutes…'; cargo build --release --locked; \
         mkdir -p {bin_dir} {apps} {state}; \
         install -Dm755 target/release/{bin} {bin_dir}/.{bin}.new; mv -f {bin_dir}/.{bin}.new {bin_dir}/{bin}; \
         if [ -f packaging/{id}.desktop ]; then sed -E 's|^Exec={bin}( \\|$)|Exec={bin_path}\\1|' packaging/{id}.desktop > {desktop}; fi; ",
        src = q(&src),
        repo = app.repo,
        name = app.name,
        bin = app.bin,
        id = app.desktop_id,
        bin_dir = q(&dirs.bin),
        bin_path = dirs.binary(app).display(),
        apps = q(&dirs.applications()),
        state = q(&dirs.state),
        desktop = q(&desktop),
    );
    if let Some(service) = app.service {
        s += &format!(
            "if [ -f packaging/{service} ]; then mkdir -p {d}; sed 's|/usr/bin/{bin}|{bin_path}|' packaging/{service} > {d}/{service}; systemctl --user daemon-reload || true; fi; ",
            d = q(&dirs.data.join("systemd/user")),
            bin = app.bin,
            bin_path = dirs.binary(app).display(),
        );
    }
    if app.npm_extension {
        if let Some(share) = app.share {
            s += &format!(
                "if [ -d extension ]; then command -v npm >/dev/null || {{ echo 'The browser extension needs npm: sudo pacman -S npm'; exit 1; }}; \
                 (cd extension && npm ci --silent && node stage.mjs); mkdir -p {d}; \
                 cp stage/material-3-browser.xpi {d}/; rm -rf {d}/extension; cp -r stage/extension {d}/; fi; ",
                d = q(&dirs.data.join(share)),
            );
        }
    }
    s += &format!(
        "echo \"source-$(git rev-parse --short HEAD)\" > {version}; date +%s > {checked}; rm -f {failed}; \
         cd /; rm -rf {src}; command -v update-desktop-database >/dev/null && update-desktop-database -q {apps} || true)",
        version = q(&dirs.state_file(app, "version")),
        checked = q(&dirs.state_file(app, "checked")),
        failed = q(&dirs.state_file(app, "failed")),
        src = q(&src),
        apps = q(&dirs.applications()),
    );
    s
}

/// Files and folders an install here (or by the shell's script) created.
pub fn removal_paths(app: &MaterialApp, dirs: &Dirs) -> Vec<PathBuf> {
    let mut paths = vec![dirs.binary(app), dirs.desktop_file(app)];
    if let Some(share) = app.share {
        paths.push(dirs.data.join(share));
    }
    if let Some(service) = app.service {
        paths.push(dirs.data.join("systemd/user").join(service));
    }
    for ext in ["version", "checked", "failed"] {
        paths.push(dirs.state_file(app, ext));
    }
    paths
}

/// Remove an app installed here. Essential apps are refused.
pub fn remove_local(app: &MaterialApp, dirs: &Dirs) -> Result<String, String> {
    if app.essential {
        return Err(format!("{} comes with the shell and can't be removed", app.name));
    }
    if let Some(service) = app.service {
        let _ = cmd::run("systemctl", &["--user", "disable", "--now", service]);
    }
    let mut failed = Vec::new();
    for path in removal_paths(app, dirs) {
        let result = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if let Err(e) = result {
            if e.kind() != std::io::ErrorKind::NotFound {
                failed.push(format!("{}: {e}", path.display()));
            }
        }
    }
    if app.service.is_some() {
        let _ = cmd::run("systemctl", &["--user", "daemon-reload"]);
    }
    if failed.is_empty() {
        Ok(format!("{} removed", app.name))
    } else {
        Err(format!("Couldn't remove {}", failed.join(", ")))
    }
}

/// Terminal command that removes a pacman-installed app.
pub fn package_remove_script(app: &MaterialApp, package: &str) -> String {
    let mut s = String::new();
    if let Some(service) = app.service {
        s += &format!("systemctl --user disable --now {service} 2>/dev/null; ");
    }
    s + &format!("sudo pacman -Rns {}", shell_quote(package))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material_apps::{detect, find, tests::temp_dirs, Installed};
    use std::collections::HashSet;

    #[test]
    fn exec_lines_point_at_the_binary() {
        let entry = "[Desktop Entry]\nName=Music\nExec=m3-music %U\nTryExec=m3-music\n\n\
                     [Desktop Action new]\nExec=m3-music\nExec=m3-music-helper --x\n";
        let out = rewrite_exec(entry, "m3-music", Path::new("/home/u/.local/bin"));
        assert_eq!(
            out,
            "[Desktop Entry]\nName=Music\nExec=/home/u/.local/bin/m3-music %U\nTryExec=m3-music\n\n\
             [Desktop Action new]\nExec=/home/u/.local/bin/m3-music\nExec=m3-music-helper --x\n"
        );
        assert_eq!(rewrite_exec("Exec=m3-music", "m3-music", Path::new("/b")), "Exec=/b/m3-music");
    }

    #[test]
    fn checksums() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.tar.gz");
        std::fs::write(&file, "hello\n").unwrap();
        let good = "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03  a.tar.gz\n";
        assert!(verify_sha256(&file, good).is_ok());
        let bad = good.replace("5891", "0000");
        assert!(verify_sha256(&file, &bad).is_err());
        assert!(verify_sha256(&file, "nonsense").is_err());
    }

    /// Lay out an unpacked release like the release workflow builds it.
    fn fake_release(root: &Path, app: &MaterialApp) -> PathBuf {
        let dir = root.join(format!("{}-x86_64-linux", app.bin));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(app.bin), "#!/bin/sh\necho hi\n").unwrap();
        std::fs::write(
            dir.join(format!("{}.desktop", app.desktop_id)),
            format!("[Desktop Entry]\nExec={}\n", app.bin),
        )
        .unwrap();
        if let Some(share) = app.share {
            std::fs::create_dir_all(dir.join("share").join(share)).unwrap();
            std::fs::write(dir.join("share").join(share).join("data.txt"), "x").unwrap();
        }
        dir
    }

    #[test]
    fn install_then_remove_leaves_nothing() {
        let root = tempfile::tempdir().unwrap();
        let dirs = temp_dirs(&root.path().join("home"));
        let browser = find("browser").unwrap();
        let unpacked = fake_release(root.path(), browser);
        install_unpacked(browser, &dirs, &unpacked, "v1.0.0").unwrap();

        assert_eq!(
            detect(browser, &dirs, &HashSet::new()),
            Installed::Managed { version: "v1.0.0".into() }
        );
        let desktop = std::fs::read_to_string(dirs.desktop_file(browser)).unwrap();
        assert!(desktop.contains(&format!("Exec={}", dirs.binary(browser).display())));
        assert!(dirs.data.join("m3-browser/data.txt").is_file());
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dirs.binary(browser)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }

        let paths = removal_paths(browser, &dirs);
        assert!(paths.contains(&dirs.binary(browser)));
        assert!(paths.contains(&dirs.desktop_file(browser)));
        assert!(paths.contains(&dirs.data.join("m3-browser")));
        assert!(paths.contains(&dirs.state_file(browser, "version")));
        // Nothing outside the app's own files.
        assert!(!paths.contains(&dirs.data));
        assert!(!paths.contains(&dirs.bin));

        remove_local(browser, &dirs).unwrap();
        for p in &paths {
            assert!(!p.exists(), "{} left behind", p.display());
        }
        assert!(dirs.bin.is_dir() && dirs.applications().is_dir());
        assert_eq!(detect(browser, &dirs, &HashSet::new()), Installed::NotInstalled);
    }

    #[test]
    fn essential_apps_are_never_removed() {
        let root = tempfile::tempdir().unwrap();
        let dirs = temp_dirs(root.path());
        let files = find("files").unwrap();
        let unpacked = fake_release(root.path(), files);
        install_unpacked(files, &dirs, &unpacked, "v0.2.0").unwrap();
        assert!(remove_local(files, &dirs).is_err());
        assert!(dirs.binary(files).is_file());
    }

    #[test]
    fn source_script_is_quoted_and_records_the_commit() {
        let dirs = Dirs {
            bin: "/home/a b/.local/bin".into(),
            data: "/home/a b/.local/share".into(),
            state: "/home/a b/.local/state/quickshell/user/material-apps".into(),
            cache: "/home/a b/.cache/m3-settings/material-apps".into(),
            pacman_db: "/var/lib/pacman/local".into(),
        };
        let s = source_script(find("browser").unwrap(), &dirs);
        assert!(s.starts_with("(set -e;") && s.ends_with(')'));
        assert!(s.contains("git clone --depth 1 https://github.com/bhimio1/material-3-browser.git"));
        assert!(s.contains("cargo build --release --locked"));
        assert!(s.contains("npm ci"));
        assert!(s.contains("'/home/a b/.local/state/quickshell/user/material-apps/m3-browser.version'"));
        assert!(s.contains("source-$(git rev-parse --short HEAD)"));
        let s = source_script(find("text").unwrap(), &dirs);
        assert!(!s.contains("npm"));
    }
}
