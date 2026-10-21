//! What's published: the latest GitHub release of each app (checked at most
//! every few hours and cached, since unauthenticated API calls are limited to
//! 60 an hour) and which AUR packages exist.

use super::{MaterialApp, APPS};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How old a cached answer may be before asking GitHub again.
pub const MAX_AGE: Duration = Duration::from_secs(6 * 60 * 60);

const UA: &str = concat!("m3-settings/", env!("CARGO_PKG_VERSION"));

/// The HTTP client: honours `HTTPS_PROXY`/`NO_PROXY` and trusts the system's
/// certificate store.
pub fn agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(timeout)
        .user_agent(UA)
        .try_proxy_from_env(true)
        .build()
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Release {
    pub tag: String,
    pub page: String,
    /// The x86_64 tarball and its checksum, when the release has them.
    pub tarball: Option<String>,
    pub checksum: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase", tag = "status")]
pub enum Latest {
    #[default]
    Unknown,
    Found(Release),
    /// The project has no release yet.
    None,
}

/// The cached answer for one app.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct Cached {
    checked_at: u64,
    latest: Latest,
}

#[derive(Clone, Debug, Default)]
pub struct Remote {
    /// Per app, in catalog order.
    pub latest: Vec<Latest>,
    /// AUR packages that exist (`material-3-music-bin`, …); `None` when the
    /// AUR couldn't be reached.
    pub aur: Option<Vec<String>>,
    /// GitHub refused (rate limit) or couldn't be reached; cached answers
    /// were used where there were any.
    pub problem: Option<String>,
}

impl Remote {
    pub fn latest(&self, app: &MaterialApp) -> &Latest {
        static UNKNOWN: Latest = Latest::Unknown;
        APPS.iter()
            .position(|a| a.id == app.id)
            .and_then(|i| self.latest.get(i))
            .unwrap_or(&UNKNOWN)
    }

    pub fn aur_published(&self, app: &MaterialApp) -> Option<bool> {
        let names = self.aur.as_ref()?;
        Some(app.aur_packages().iter().any(|p| names.contains(p)))
    }

    /// The AUR package to install: the prebuilt one when it exists.
    pub fn aur_package(&self, app: &MaterialApp) -> String {
        let [bin, git] = app.aur_packages();
        match &self.aur {
            Some(names) if !names.contains(&bin) && names.contains(&git) => git,
            _ => bin,
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Read a GitHub "latest release" response.
pub fn parse_release(app: &MaterialApp, json: &str) -> Result<Release, String> {
    #[derive(Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }
    #[derive(Deserialize)]
    struct Raw {
        tag_name: String,
        #[serde(default)]
        html_url: String,
        #[serde(default)]
        assets: Vec<Asset>,
    }
    let raw: Raw = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let tarball_name = format!("{}-x86_64-linux.tar.gz", app.bin);
    let url_of = |name: &str| {
        raw.assets
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.browser_download_url.clone())
    };
    Ok(Release {
        tarball: url_of(&tarball_name),
        checksum: url_of(&format!("{tarball_name}.sha256")),
        tag: raw.tag_name,
        page: raw.html_url,
    })
}

enum Fetch {
    Found(Release),
    None,
    /// Rate limited or failed: try again later, keep the cache.
    Problem(String),
}

fn fetch_latest(app: &MaterialApp) -> Fetch {
    let url = format!("https://api.github.com/repos/{}/releases/latest", app.repo);
    let resp = agent(Duration::from_secs(20))
        .get(&url)
        .set("Accept", "application/vnd.github+json")
        .call();
    match resp {
        Ok(r) => match r.into_string().map_err(|e| e.to_string()).and_then(|s| parse_release(app, &s)) {
            Ok(rel) => Fetch::Found(rel),
            Err(e) => Fetch::Problem(e),
        },
        Err(ureq::Error::Status(404, _)) => Fetch::None,
        Err(ureq::Error::Status(403 | 429, r)) => {
            let reset = r
                .header("x-ratelimit-reset")
                .and_then(|s| s.parse::<u64>().ok())
                .map(|t| t.saturating_sub(now()) / 60 + 1);
            Fetch::Problem(match reset {
                Some(min) => format!("GitHub's limit on update checks was reached; it resets in {min} min."),
                None => "GitHub's limit on update checks was reached. Try again later.".into(),
            })
        }
        Err(e) => Fetch::Problem(format!("Couldn't reach GitHub: {e}")),
    }
}

/// The latest release of `app`, from the cache when it's fresh enough.
fn latest_cached(app: &MaterialApp, cache: &Path, force: bool, problem: &mut Option<String>) -> Latest {
    let file = cache.join(format!("{}.json", app.bin));
    let cached: Option<Cached> = std::fs::read_to_string(&file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    if let Some(c) = &cached {
        if !force && now().saturating_sub(c.checked_at) < MAX_AGE.as_secs() {
            return c.latest.clone();
        }
    }
    // After one refusal, don't spend more requests this round.
    let fetched = if problem.is_some() {
        Fetch::Problem(String::new())
    } else {
        fetch_latest(app)
    };
    let latest = match fetched {
        Fetch::Found(r) => Latest::Found(r),
        Fetch::None => Latest::None,
        Fetch::Problem(e) => {
            if problem.is_none() {
                *problem = Some(e);
            }
            return cached.map(|c| c.latest).unwrap_or_default();
        }
    };
    let entry = Cached {
        checked_at: now(),
        latest: latest.clone(),
    };
    if std::fs::create_dir_all(cache).is_ok() {
        if let Ok(json) = serde_json::to_string(&entry) {
            let _ = std::fs::write(&file, json);
        }
    }
    latest
}

/// Which of `names` exist on the AUR.
fn aur_packages(names: &[String]) -> Option<Vec<String>> {
    let query: String = names.iter().map(|n| format!("&arg[]={n}")).collect();
    let url = format!("https://aur.archlinux.org/rpc/v5/info?{}", &query[1..]);
    #[derive(Deserialize)]
    struct Pkg {
        #[serde(rename = "Name")]
        name: String,
    }
    #[derive(Deserialize)]
    struct Resp {
        results: Vec<Pkg>,
    }
    let resp: Resp = agent(Duration::from_secs(15))
        .get(&url)
        .call()
        .ok()?
        .into_json()
        .ok()?;
    Some(resp.results.into_iter().map(|p| p.name).collect())
}

/// Check everything. `force` ignores the cache (the refresh button).
pub fn check(cache: &Path, force: bool) -> Remote {
    let mut problem = None;
    let latest = APPS
        .iter()
        .map(|a| latest_cached(a, cache, force, &mut problem))
        .collect();
    let names: Vec<String> = APPS.iter().flat_map(|a| a.aur_packages()).collect();
    Remote {
        latest,
        aur: aur_packages(&names),
        problem: problem.filter(|p| !p.is_empty()),
    }
}

/// Numeric parts of a version or tag: `v1.2.3` → `[1, 2, 3]`. A pre-release
/// or build suffix (`-rc1`, `+git`) is ignored. `None` when it doesn't start
/// with a number.
pub fn parse_version(tag: &str) -> Option<Vec<u64>> {
    let tag = tag.trim();
    let tag = tag.strip_prefix(['v', 'V']).unwrap_or(tag);
    let core = tag.split(['-', '+', '_', ' ']).next()?;
    let parts: Vec<u64> = core
        .split('.')
        .map(|p| p.parse::<u64>())
        .collect::<Result<_, _>>()
        .ok()?;
    (!parts.is_empty()).then_some(parts)
}

/// Is `latest` newer than what's installed? A source build counts as older
/// than any release, so it moves to the release.
pub fn is_newer(latest: &str, installed: &str) -> bool {
    if installed.starts_with("source-") {
        return true;
    }
    match (parse_version(latest), parse_version(installed)) {
        (Some(mut a), Some(mut b)) => {
            let len = a.len().max(b.len());
            a.resize(len, 0);
            b.resize(len, 0);
            a.cmp(&b) == Ordering::Greater
        }
        _ => latest.trim() != installed.trim(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material_apps::find;

    #[test]
    fn release_tags() {
        assert_eq!(parse_version("v1.2.3"), Some(vec![1, 2, 3]));
        assert_eq!(parse_version("0.4"), Some(vec![0, 4]));
        assert_eq!(parse_version("v0.1.0-rc1"), Some(vec![0, 1, 0]));
        assert_eq!(parse_version("0.4.0-1"), Some(vec![0, 4, 0]));
        assert_eq!(parse_version("nightly"), None);
        assert_eq!(parse_version("v"), None);

        assert!(is_newer("v0.2.0", "v0.1.9"));
        assert!(is_newer("v0.10.0", "v0.9.0"));
        assert!(!is_newer("v0.2.0", "v0.2.0"));
        assert!(!is_newer("v0.2", "v0.2.0"));
        assert!(!is_newer("v0.1.0", "v0.2.0"));
        assert!(is_newer("v0.1.0", "source-abc1234"));
        assert!(is_newer("nightly-2", "nightly-1"));
        assert!(!is_newer("nightly", "nightly"));
    }

    #[test]
    fn release_json() {
        let music = find("music").unwrap();
        let json = r#"{
            "tag_name": "v0.3.0",
            "html_url": "https://github.com/bhimio1/material-3-music/releases/tag/v0.3.0",
            "assets": [
                {"name": "m3-music-x86_64-linux.tar.gz", "browser_download_url": "https://x/m3-music-x86_64-linux.tar.gz"},
                {"name": "m3-music-x86_64-linux.tar.gz.sha256", "browser_download_url": "https://x/m3-music-x86_64-linux.tar.gz.sha256"},
                {"name": "other.zip", "browser_download_url": "https://x/other.zip"}
            ]
        }"#;
        let r = parse_release(music, json).unwrap();
        assert_eq!(r.tag, "v0.3.0");
        assert_eq!(r.tarball.as_deref(), Some("https://x/m3-music-x86_64-linux.tar.gz"));
        assert_eq!(r.checksum.as_deref(), Some("https://x/m3-music-x86_64-linux.tar.gz.sha256"));

        let bare = parse_release(music, r#"{"tag_name": "v1"}"#).unwrap();
        assert_eq!(bare.tarball, None);
        assert!(parse_release(music, "{}").is_err());
    }

    #[test]
    fn cache_is_used_while_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let music = find("music").unwrap();
        let entry = Cached {
            checked_at: now(),
            latest: Latest::Found(Release {
                tag: "v9.9.9".into(),
                ..Default::default()
            }),
        };
        std::fs::write(dir.path().join("m3-music.json"), serde_json::to_string(&entry).unwrap()).unwrap();
        let mut problem = None;
        // Fresh: no network needed.
        assert_eq!(latest_cached(music, dir.path(), false, &mut problem), entry.latest);
        // A refusal earlier this round: the stale cache is still returned.
        let stale = Cached { checked_at: 0, ..entry.clone() };
        std::fs::write(dir.path().join("m3-music.json"), serde_json::to_string(&stale).unwrap()).unwrap();
        let mut problem = Some("rate limited".to_string());
        assert_eq!(latest_cached(music, dir.path(), false, &mut problem), entry.latest);
    }

    #[test]
    fn aur_package_choice() {
        let music = find("music").unwrap();
        let mut remote = Remote::default();
        assert_eq!(remote.aur_published(music), None);
        assert_eq!(remote.aur_package(music), "material-3-music-bin");
        remote.aur = Some(vec!["material-3-music-git".into()]);
        assert_eq!(remote.aur_published(music), Some(true));
        assert_eq!(remote.aur_package(music), "material-3-music-git");
        remote.aur = Some(vec![]);
        assert_eq!(remote.aur_published(music), Some(false));
    }
}
