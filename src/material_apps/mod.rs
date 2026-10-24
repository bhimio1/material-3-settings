//! The Material 3 apps: what they are, where they live and how they are
//! installed. Settings is where the optional ones are installed, updated and
//! removed; the essential ones come with the end4-pC shell (its
//! `scripts/material-apps/install.sh`) and can be updated or repaired here.
//!
//! Installs made here and by the shell's script share the same layout and
//! state files, so either one picks up what the other installed:
//!
//! - the binary in `~/.local/bin/<bin>`;
//! - the desktop entry in `$XDG_DATA_HOME/applications/<desktop-id>.desktop`;
//! - data files (if any) under `$XDG_DATA_HOME`;
//! - `$XDG_STATE_HOME/quickshell/user/material-apps/<bin>.version`, holding
//!   the release tag or `source-<commit>`.

pub mod install;
pub mod release;

use material_3_ui::sys::{arch, cmd, paths};
use material_3_ui::icons::{icons::*, MaterialIcon};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Something an app needs (or can use) that comes from a package.
pub struct Need {
    /// Any of these commands satisfies the need.
    pub programs: &'static [&'static str],
    /// Package to install when none of them is there.
    pub package: &'static str,
    pub purpose: &'static str,
    pub required: bool,
}

pub struct MaterialApp {
    /// Short name, used on the command line (`m3-settings material-apps music`).
    pub id: &'static str,
    pub bin: &'static str,
    pub name: &'static str,
    /// GitHub `owner/repo`.
    pub repo: &'static str,
    /// Desktop entry name without `.desktop`; also the Wayland app id.
    pub desktop_id: &'static str,
    /// AUR package base: published as `<aur>-bin` and `<aur>-git`.
    pub aur: &'static str,
    pub icon: MaterialIcon,
    pub summary: &'static str,
    pub description: &'static str,
    /// Installed with the shell. Can be updated or repaired, never removed.
    pub essential: bool,
    pub needs: &'static [Need],
    /// Data directory under `$XDG_DATA_HOME` the release ships in `share/`.
    pub share: Option<&'static str>,
    /// A systemd user service it ships (and should run in the background).
    pub service: Option<&'static str>,
    /// Building from source also builds the browser extension with npm.
    pub npm_extension: bool,
}

const fn need(programs: &'static [&'static str], package: &'static str, purpose: &'static str, required: bool) -> Need {
    Need { programs, package, purpose, required }
}

pub const APPS: &[MaterialApp] = &[
    MaterialApp {
        id: "settings",
        bin: "m3-settings",
        name: "Settings",
        repo: "bhimio1/material-3-settings",
        desktop_id: "material_3_settings",
        aur: "material-3-settings",
        icon: ICON_SETTINGS,
        summary: "This app: system, desktop and config settings",
        description: "Settings for Arch Linux, Hyprland and end4-pC: displays, sound, network, packages, \
                      the shell's options, config files for dozens of programs, and these apps.",
        essential: true,
        needs: &[],
        share: None,
        service: None,
        npm_extension: false,
    },
    MaterialApp {
        id: "files",
        bin: "m3fm",
        name: "Files",
        repo: "bhimio1/material-3-file-manager-rust",
        desktop_id: "material_3_file_manager",
        aur: "material-3-file-manager",
        icon: ICON_FOLDER,
        summary: "The file manager the shell opens folders with",
        description: "A fast file manager with tabs, previews, search and archive support. \
                      The shell opens folders, downloads and screenshots in it.",
        essential: true,
        needs: &[],
        share: None,
        service: None,
        npm_extension: false,
    },
    MaterialApp {
        id: "screenshots",
        bin: "m3-screenshots",
        name: "Screenshots",
        repo: "bhimio1/material-3-screenshots",
        desktop_id: "material_3_screenshots",
        aur: "material-3-screenshots",
        icon: ICON_SCREENSHOT_MONITOR,
        summary: "Capture, annotate and record the screen",
        description: "Screenshots of a region, a window or the whole screen, with annotation, \
                      and screen recording. The shell's screenshot keys use it.",
        essential: true,
        needs: &[
            need(&["grim"], "grim", "Takes the screenshots", true),
            need(&["slurp"], "slurp", "Selects regions", true),
            need(&["wl-copy"], "wl-clipboard", "Copies to the clipboard", false),
            need(&["wf-recorder"], "wf-recorder", "Screen recording", false),
        ],
        share: None,
        service: None,
        npm_extension: false,
    },
    MaterialApp {
        id: "music",
        bin: "m3-music",
        name: "Music",
        repo: "bhimio1/material-3-music",
        desktop_id: "material_3_music",
        aur: "material-3-music",
        icon: ICON_MUSIC_NOTE,
        summary: "YouTube Music and your local library",
        description: "A YouTube Music client with sign-in, library sync and downloads, \
                      that also plays your own music files.",
        essential: false,
        needs: &[
            need(&["mpv"], "mpv", "Plays audio", true),
            need(&["yt-dlp"], "yt-dlp", "Streams and downloads from YouTube Music", true),
        ],
        share: None,
        service: None,
        npm_extension: false,
    },
    MaterialApp {
        id: "games",
        bin: "m3-games",
        name: "Games",
        repo: "bhimio1/material-3-games",
        desktop_id: "material_3_games",
        aur: "material-3-games",
        icon: ICON_SPORTS_ESPORTS,
        summary: "Steam, Lutris and Heroic games, and Game Mode",
        description: "One library for Steam, Lutris and Heroic games, with a full-screen Game Mode \
                      that runs games in gamescope instead of Hyprland for the best frame rates.",
        essential: false,
        needs: &[
            need(&["gamescope"], "gamescope", "Game Mode", false),
            need(&["steam"], "steam", "Steam games", false),
            need(&["lutris"], "lutris", "Lutris games", false),
            need(&["gamemoderun"], "gamemode", "CPU and GPU tuning while playing", false),
            need(&["mangohud"], "mangohud", "Performance overlay", false),
        ],
        share: None,
        service: None,
        npm_extension: false,
    },
    MaterialApp {
        id: "browser",
        bin: "m3-browser",
        name: "Browser",
        repo: "bhimio1/material-3-browser",
        desktop_id: "material_3_browser",
        aur: "material-3-browser",
        icon: ICON_PUBLIC,
        summary: "Zen with websites themed to your wallpaper",
        description: "A Zen-based browser setup that themes the browser and websites with your \
                      wallpaper's colors, with a blocklist and per-site options.",
        essential: false,
        needs: &[need(
            &["zen-browser", "zen", "librewolf", "firefox"],
            "zen-browser-bin",
            "The browser it runs (Zen, or LibreWolf or Firefox)",
            true,
        )],
        share: Some("m3-browser"),
        service: None,
        npm_extension: true,
    },
    MaterialApp {
        id: "ai",
        bin: "m3-ai",
        name: "AI",
        repo: "bhimio1/material-3-ai",
        desktop_id: "material_3_ai",
        aur: "material-3-ai",
        icon: ICON_SMART_TOY,
        summary: "A desktop app for Claude Code",
        description: "Chats, projects and tool approvals for Claude Code in a desktop window. \
                      Sign-in stays with the claude command itself.",
        essential: false,
        needs: &[need(&["claude"], "claude-code", "The claude command it drives", true)],
        share: None,
        service: None,
        npm_extension: false,
    },
    MaterialApp {
        id: "viewer",
        bin: "m3-viewer",
        name: "Viewer",
        repo: "bhimio1/material-3-veiwer",
        desktop_id: "material_3_viewer",
        aur: "material-3-viewer",
        icon: ICON_PHOTO_LIBRARY,
        summary: "Images and videos in one viewer",
        description: "Opens photos, GIFs and videos, flips through the folder they're in, \
                      and does quick edits like rotate and crop.",
        essential: false,
        needs: &[need(&["mpv"], "mpv", "Plays videos", true)],
        share: None,
        service: None,
        npm_extension: false,
    },
    MaterialApp {
        id: "text",
        bin: "m3-text",
        name: "Text",
        repo: "bhimio1/material-3-text",
        desktop_id: "material_3_text",
        aur: "material-3-text",
        icon: ICON_EDIT_NOTE,
        summary: "A text editor with tabs and highlighting",
        description: "A simple, quick text editor for notes and config files, with tabs, \
                      syntax highlighting, search and replace.",
        essential: false,
        needs: &[],
        share: None,
        service: None,
        npm_extension: false,
    },
    MaterialApp {
        id: "monitor",
        bin: "m3-monitor",
        name: "Monitor",
        repo: "bhimio1/material-3-monitor",
        desktop_id: "material_3_monitor",
        aur: "material-3-monitor",
        icon: ICON_MONITOR_HEART,
        summary: "Processes, CPU, memory, disks and GPU",
        description: "See what's using your CPU, memory, disks, network and GPU, \
                      and stop processes that misbehave.",
        essential: false,
        needs: &[],
        share: None,
        service: None,
        npm_extension: false,
    },
    MaterialApp {
        id: "calendar",
        bin: "m3-calendar",
        name: "Calendar",
        repo: "bhimio1/material-3-calendar",
        desktop_id: "material_3_calendar",
        aur: "material-3-calendar",
        icon: ICON_CALENDAR_MONTH,
        summary: "Google, Microsoft, iCloud and local calendars",
        description: "Add your Google, Microsoft or iCloud account by signing in, or keep a local \
                      calendar. A small background service keeps events offline and sends reminders, \
                      and the shell shows your next events.",
        essential: false,
        needs: &[need(&["notify-send"], "libnotify", "Reminder notifications", false)],
        share: None,
        service: Some("m3-calendar.service"),
        npm_extension: false,
    },
    MaterialApp {
        id: "comics",
        bin: "m3-comics",
        name: "Comics",
        repo: "bhimio1/material-3-comics",
        desktop_id: "material_3_comics",
        aur: "material-3-comics",
        icon: ICON_AUTO_STORIES,
        summary: "Manga and comics, like Mihon",
        description: "A library, updates, downloads and AniList tracking for manga and comics. \
                      Reads your CBZ and CBR files, MangaDex, and Mihon's extensions through Suwayomi.",
        essential: false,
        needs: &[
            need(&["bsdtar"], "libarchive", "Opens CBR, CB7 and CBT files", false),
            need(&["suwayomi-server"], "suwayomi-server-bin", "Runs Mihon's extensions as sources", false),
            need(&["secret-tool"], "libsecret", "Keeps the AniList sign-in in your keyring", false),
        ],
        share: None,
        service: None,
        npm_extension: false,
    },
];

/// Find an app by short name or binary (`music` or `m3-music`).
pub fn find(key: &str) -> Option<&'static MaterialApp> {
    APPS.iter().find(|a| a.id == key || a.bin == key)
}

impl MaterialApp {
    pub fn project_url(&self) -> String {
        format!("https://github.com/{}", self.repo)
    }

    pub fn aur_packages(&self) -> [String; 2] {
        [format!("{}-bin", self.aur), format!("{}-git", self.aur)]
    }

    /// Names a pacman install of this app can have.
    fn package_names(&self) -> [String; 3] {
        let [bin, git] = self.aur_packages();
        [bin, git, self.aur.to_string()]
    }
}

/// Where installs go. Tests point these at temporary directories.
#[derive(Clone, Debug)]
pub struct Dirs {
    pub bin: PathBuf,
    /// `$XDG_DATA_HOME`.
    pub data: PathBuf,
    /// The shared state directory (`…/quickshell/user/material-apps`).
    pub state: PathBuf,
    pub cache: PathBuf,
    pub pacman_db: PathBuf,
}

impl Dirs {
    pub fn from_env() -> Self {
        let home = paths::home();
        Self {
            bin: home.join(".local/bin"),
            data: dirs::data_dir().unwrap_or_else(|| home.join(".local/share")),
            state: dirs::state_dir()
                .unwrap_or_else(|| home.join(".local/state"))
                .join("quickshell/user/material-apps"),
            cache: dirs::cache_dir()
                .unwrap_or_else(|| home.join(".cache"))
                .join("m3-settings/material-apps"),
            pacman_db: PathBuf::from("/var/lib/pacman/local"),
        }
    }

    pub fn applications(&self) -> PathBuf {
        self.data.join("applications")
    }

    pub fn binary(&self, app: &MaterialApp) -> PathBuf {
        self.bin.join(app.bin)
    }

    pub fn desktop_file(&self, app: &MaterialApp) -> PathBuf {
        self.applications().join(format!("{}.desktop", app.desktop_id))
    }

    pub fn state_file(&self, app: &MaterialApp, ext: &str) -> PathBuf {
        self.state.join(format!("{}.{ext}", app.bin))
    }
}

/// How an app is installed right now.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Installed {
    NotInstalled,
    /// A pacman package (from the AUR or a repo).
    Package { name: String, version: Option<String> },
    /// Installed here or by the shell's script: a release tag or `source-<sha>`.
    Managed { version: String },
    /// On `PATH`, but put there some other way (`cargo install`, by hand).
    Other { path: PathBuf },
}

impl Installed {
    pub fn is_installed(&self) -> bool {
        !matches!(self, Installed::NotInstalled)
    }

    pub fn version_label(&self) -> Option<String> {
        match self {
            Installed::Package { version, .. } => version.clone(),
            Installed::Managed { version } => Some(match version.strip_prefix("source-") {
                Some(sha) => format!("built from source ({sha})"),
                None => version.clone(),
            }),
            _ => None,
        }
    }
}

/// The installed version of a pacman package, from its directory name in the
/// local database (`<name>-<pkgver>-<pkgrel>`).
pub fn package_version(db: &Path, name: &str) -> Option<String> {
    let prefix = format!("{name}-");
    std::fs::read_dir(db).ok()?.flatten().find_map(|e| {
        let dir = e.file_name().to_string_lossy().into_owned();
        let rest = dir.strip_prefix(&prefix)?;
        // pkgver and pkgrel can't contain '-', so exactly one is left; this
        // keeps `foo-bin` from matching a `foo` lookup.
        (rest.matches('-').count() == 1 && rest.starts_with(|c: char| c.is_ascii_alphanumeric()))
            .then(|| rest.to_string())
    })
}

pub fn detect(app: &MaterialApp, dirs: &Dirs, packages: &HashSet<String>) -> Installed {
    if let Some(name) = app.package_names().into_iter().find(|n| packages.contains(n)) {
        let version = package_version(&dirs.pacman_db, &name);
        return Installed::Package { name, version };
    }
    let binary = dirs.binary(app);
    if binary.is_file() {
        if let Ok(v) = std::fs::read_to_string(dirs.state_file(app, "version")) {
            let version = v.trim().to_string();
            if !version.is_empty() {
                return Installed::Managed { version };
            }
        }
    }
    if binary.is_file() {
        return Installed::Other { path: binary };
    }
    match which(app.bin) {
        Some(path) => Installed::Other { path },
        None => Installed::NotInstalled,
    }
}

fn which(program: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|d| d.join(program))
        .find(|p| p.is_file())
}

/// Everything the page shows about the local system, read in the background.
#[derive(Clone, Debug, Default)]
pub struct LocalState {
    pub installed: Vec<Installed>,
    /// Programs from every app's needs that are on `PATH`.
    pub present: HashSet<&'static str>,
    /// Services that are enabled (`systemctl --user is-enabled`).
    pub services_enabled: HashSet<&'static str>,
}

impl LocalState {
    pub fn get(&self, app: &MaterialApp) -> &Installed {
        static NONE: Installed = Installed::NotInstalled;
        APPS.iter()
            .position(|a| a.id == app.id)
            .and_then(|i| self.installed.get(i))
            .unwrap_or(&NONE)
    }

    pub fn need_met(&self, need: &Need) -> bool {
        need.programs.iter().any(|p| self.present.contains(p))
    }
}

pub fn read_local(dirs: &Dirs) -> LocalState {
    let packages = arch::installed_packages_in(&dirs.pacman_db);
    let installed = APPS.iter().map(|a| detect(a, dirs, &packages)).collect();
    let present = APPS
        .iter()
        .flat_map(|a| a.needs.iter().flat_map(|n| n.programs.iter().copied()))
        .filter(|p| cmd::exists(p))
        .collect();
    let services_enabled = APPS
        .iter()
        .filter_map(|a| a.service)
        .filter(|s| cmd::run("systemctl", &["--user", "is-enabled", "--quiet", s]).is_ok())
        .collect();
    LocalState {
        installed,
        present,
        services_enabled,
    }
}

/// How to install an app.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    /// Pick the best of the others.
    #[default]
    Auto,
    /// The AUR package, through paru or yay, in a terminal.
    Aur,
    /// The prebuilt binary from the latest GitHub release.
    Release,
    /// Clone and build with cargo, in a terminal.
    Source,
}

impl Method {
    pub fn label(self) -> &'static str {
        match self {
            Method::Auto => "Automatic",
            Method::Aur => "AUR package",
            Method::Release => "GitHub release",
            Method::Source => "Build from source",
        }
    }
}

/// What can be used to install an app on this machine.
#[derive(Clone, Copy, Debug, Default)]
pub struct Availability {
    pub arch_based: bool,
    /// paru or yay is installed.
    pub aur_helper: bool,
    /// The app is published on the AUR (`None` = not known yet).
    pub aur_published: Option<bool>,
    /// A release with an x86_64 build exists, and this is x86_64.
    pub release: Option<bool>,
}

/// Turn the preferred method into the one to use. `Auto` prefers the AUR
/// package (pacman then keeps it updated), then the release, then building
/// from source. An explicit choice is used as long as it's possible at all.
pub fn choose(preferred: Method, av: Availability) -> Method {
    let aur = av.arch_based && av.aur_helper && av.aur_published == Some(true);
    let release = av.release == Some(true);
    match preferred {
        Method::Aur if av.arch_based && av.aur_helper => Method::Aur,
        Method::Release if release => Method::Release,
        Method::Source => Method::Source,
        _ if aur => Method::Aur,
        _ if release => Method::Release,
        _ => Method::Source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn temp_dirs(root: &Path) -> Dirs {
        Dirs {
            bin: root.join("bin"),
            data: root.join("share"),
            state: root.join("state/quickshell/user/material-apps"),
            cache: root.join("cache"),
            pacman_db: root.join("pacman"),
        }
    }

    #[test]
    fn catalog_is_consistent() {
        let mut ids = HashSet::new();
        let mut bins = HashSet::new();
        for a in APPS {
            assert!(ids.insert(a.id), "duplicate id {}", a.id);
            assert!(bins.insert(a.bin), "duplicate binary {}", a.bin);
            assert!(a.repo.starts_with("bhimio1/material-3-"), "{}", a.repo);
            assert!(a.desktop_id.starts_with("material_3_"), "{}", a.desktop_id);
            assert!(a.aur.starts_with("material-3-"), "{}", a.aur);
            assert!(!a.summary.is_empty() && !a.description.is_empty());
            assert_eq!(find(a.id).unwrap().bin, a.bin);
            assert_eq!(find(a.bin).unwrap().id, a.id);
        }
        let essential: Vec<_> = APPS.iter().filter(|a| a.essential).map(|a| a.bin).collect();
        assert_eq!(essential, ["m3-settings", "m3fm", "m3-screenshots"]);
        assert_eq!(find("files").unwrap().desktop_id, "material_3_file_manager");
        assert_eq!(find("viewer").unwrap().repo, "bhimio1/material-3-veiwer");
        assert_eq!(find("calendar").unwrap().service, Some("m3-calendar.service"));
        assert_eq!(find("m3-comics").unwrap().desktop_id, "material_3_comics");
        assert!(find("nope").is_none());
    }

    #[test]
    fn method_selection() {
        let all = Availability {
            arch_based: true,
            aur_helper: true,
            aur_published: Some(true),
            release: Some(true),
        };
        assert_eq!(choose(Method::Auto, all), Method::Aur);
        assert_eq!(choose(Method::Release, all), Method::Release);
        assert_eq!(choose(Method::Source, all), Method::Source);

        let no_helper = Availability { aur_helper: false, ..all };
        assert_eq!(choose(Method::Auto, no_helper), Method::Release);
        // An explicit AUR choice without a helper falls back too.
        assert_eq!(choose(Method::Aur, no_helper), Method::Release);

        let unpublished = Availability { aur_published: Some(false), ..all };
        assert_eq!(choose(Method::Auto, unpublished), Method::Release);
        // Explicitly asked for: the -git package can still be tried.
        assert_eq!(choose(Method::Aur, unpublished), Method::Aur);

        let nothing = Availability { aur_published: Some(false), release: Some(false), ..all };
        assert_eq!(choose(Method::Auto, nothing), Method::Source);
        assert_eq!(choose(Method::Release, nothing), Method::Source);

        let not_arch = Availability { arch_based: false, ..all };
        assert_eq!(choose(Method::Auto, not_arch), Method::Release);
    }

    #[test]
    fn adopts_the_shell_scripts_installs() {
        let root = tempfile::tempdir().unwrap();
        let dirs = temp_dirs(root.path());
        let music = find("music").unwrap();
        let none = HashSet::new();
        assert_eq!(detect(music, &dirs, &none), Installed::NotInstalled);

        // What install.sh leaves behind.
        std::fs::create_dir_all(&dirs.bin).unwrap();
        std::fs::create_dir_all(&dirs.state).unwrap();
        std::fs::write(dirs.binary(music), "#!/bin/sh\n").unwrap();
        // A binary without a state file isn't ours.
        assert!(matches!(detect(music, &dirs, &none), Installed::Other { .. }));
        std::fs::write(dirs.state_file(music, "version"), "v0.3.1\n").unwrap();
        let found = detect(music, &dirs, &none);
        assert_eq!(found, Installed::Managed { version: "v0.3.1".into() });
        assert_eq!(found.version_label().unwrap(), "v0.3.1");

        std::fs::write(dirs.state_file(music, "version"), "source-abc1234\n").unwrap();
        assert_eq!(
            detect(music, &dirs, &none).version_label().unwrap(),
            "built from source (abc1234)"
        );

        // A pacman package wins over everything else.
        std::fs::create_dir_all(dirs.pacman_db.join("material-3-music-bin-0.4.0-1")).unwrap();
        std::fs::create_dir_all(dirs.pacman_db.join("material-3-music-bin-debug-0.4.0-1")).unwrap();
        let pkgs: HashSet<String> = ["material-3-music-bin".to_string()].into();
        assert_eq!(
            detect(music, &dirs, &pkgs),
            Installed::Package {
                name: "material-3-music-bin".into(),
                version: Some("0.4.0-1".into())
            }
        );
    }
}
