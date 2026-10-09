//! Update checks against the project's GitHub releases.
//!
//! The app asks GitHub for its recent releases, picks the newest one (betas
//! included while the running build is a beta), and compares it with its own
//! version. To update it downloads the installer, checks its size and SHA-256
//! digest as GitHub reports them, and starts it; the installer replaces the
//! old version in place and keeps projects and settings.
//!
//! The running version comes from the release tag the build was made from
//! (`FWM_RELEASE_TAG`, set by the release workflow), else the crate version
//! with a `-dev` suffix.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Repository whose releases are checked.
pub const REPO: &str = "Carboxylate652/FantasyWorldBuilder";

/// Version of the running build, without a leading `v`.
pub fn current_version() -> String {
    match option_env!("FWM_RELEASE_TAG") {
        Some(t) if !t.is_empty() => t.trim_start_matches('v').to_string(),
        _ => format!("{}-dev", env!("CARGO_PKG_VERSION")),
    }
}

// ---------------------------------------------------------------- versions

/// A semantic version: 0.1.0-beta.2. Pre-releases sort before the release,
/// and their parts compare numerically when both are numbers (beta.2 < beta.10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub core: [u64; 3],
    pub pre: Vec<String>,
}

impl Version {
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim().trim_start_matches('v');
        let s = s.split('+').next()?; // build metadata does not count
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, p.split('.').map(str::to_string).collect()),
            None => (s, vec![]),
        };
        let mut it = core.split('.').map(|x| x.parse::<u64>());
        let v = [it.next()?.ok()?, it.next().unwrap_or(Ok(0)).ok()?, it.next().unwrap_or(Ok(0)).ok()?];
        if it.next().is_some() {
            return None;
        }
        Some(Version { core: v, pre })
    }

    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }
}

impl Ord for Version {
    fn cmp(&self, o: &Self) -> Ordering {
        self.core.cmp(&o.core).then_with(|| match (self.pre.is_empty(), o.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => {
                for (a, b) in self.pre.iter().zip(&o.pre) {
                    let c = match (a.parse::<u64>(), b.parse::<u64>()) {
                        (Ok(x), Ok(y)) => x.cmp(&y),
                        (Ok(_), Err(_)) => Ordering::Less,
                        (Err(_), Ok(_)) => Ordering::Greater,
                        _ => a.cmp(b),
                    };
                    if c != Ordering::Equal {
                        return c;
                    }
                }
                self.pre.len().cmp(&o.pre.len())
            }
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

// ---------------------------------------------------------------- releases

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// Hex SHA-256 of the file, when GitHub reports one.
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Release {
    pub tag: String,
    pub version: String,
    pub name: String,
    /// Release page on GitHub.
    pub url: String,
    pub prerelease: bool,
    pub published_at: String,
    /// Release notes (Markdown).
    pub notes: String,
    /// The Windows installer (`…_x64-setup.exe`).
    pub installer: Option<Asset>,
    /// The portable zip.
    pub portable: Option<Asset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

/// Releases from the body of GitHub's "list releases" response (drafts and
/// tags that are not versions are left out).
pub fn parse_releases(json: &str) -> Result<Vec<Release>, String> {
    let raw: Vec<GhRelease> = serde_json::from_str(json).map_err(|e| format!("unexpected answer from GitHub: {e}"))?;
    let asset = |a: &GhAsset| Asset {
        name: a.name.clone(),
        url: a.browser_download_url.clone(),
        size: a.size,
        sha256: a.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")).map(|h| h.to_ascii_lowercase()),
    };
    Ok(raw
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            let v = Version::parse(&r.tag_name)?;
            Some(Release {
                version: r.tag_name.trim_start_matches('v').to_string(),
                name: r.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| r.tag_name.clone()),
                url: r.html_url.clone(),
                prerelease: r.prerelease || v.is_prerelease(),
                published_at: r.published_at.clone().unwrap_or_default(),
                notes: r.body.clone().unwrap_or_default(),
                installer: r.assets.iter().find(|a| a.name.ends_with("-setup.exe")).map(asset),
                portable: r.assets.iter().find(|a| a.name.ends_with("-portable.zip")).map(asset),
                tag: r.tag_name,
            })
        })
        .collect())
}

/// The newest release, betas included only when asked.
pub fn newest(releases: &[Release], include_prereleases: bool) -> Option<&Release> {
    releases
        .iter()
        .filter(|r| include_prereleases || !r.prerelease)
        .filter_map(|r| Version::parse(&r.version).map(|v| (v, r)))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|x| x.1)
}

fn agent() -> ureq::Agent {
    // The system certificate store (works behind TLS-inspecting proxies) and
    // HTTPS_PROXY / ALL_PROXY from the environment.
    ureq::AgentBuilder::new().try_proxy_from_env(true).timeout_connect(Duration::from_secs(10)).timeout_read(Duration::from_secs(60)).build()
}

fn user_agent() -> String {
    format!("FantasyWorldMaker/{}", current_version())
}

/// Recent releases of the repository, from the GitHub API.
pub fn fetch_releases() -> Result<Vec<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=20");
    let resp = agent()
        .get(&url)
        .set("User-Agent", &user_agent())
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(403 | 429, _) => "GitHub's rate limit was reached; try again in an hour".to_string(),
            ureq::Error::Status(c, _) => format!("GitHub answered with status {c}"),
            ureq::Error::Transport(t) => format!("could not reach GitHub: {t}"),
        })?;
    let body = resp.into_string().map_err(|e| format!("could not read GitHub's answer: {e}"))?;
    parse_releases(&body)
}

#[derive(Clone, Debug, Serialize)]
pub struct Check {
    pub current: String,
    pub include_prereleases: bool,
    /// The newest release (whether or not it is newer than this build).
    pub latest: Option<Release>,
    /// The newest release is newer than this build.
    pub update_available: bool,
    /// This build was installed with the installer (so the installer can
    /// update it in place); false for the portable zip.
    pub installed: bool,
}

/// Compare this build with the newest release. `include_prereleases`:
/// None = only when this build is itself a pre-release.
pub fn check(include_prereleases: Option<bool>) -> Result<Check, String> {
    let current = current_version();
    let cur = Version::parse(&current);
    let pre = include_prereleases.unwrap_or_else(|| cur.as_ref().is_some_and(|v| v.is_prerelease()));
    let list = fetch_releases()?;
    let latest = newest(&list, pre).cloned();
    let update_available = match (&latest, &cur) {
        (Some(l), Some(c)) => Version::parse(&l.version).is_some_and(|v| v > *c),
        _ => false,
    };
    Ok(Check { current, include_prereleases: pre, latest, update_available, installed: is_installed_copy() })
}

// ---------------------------------------------------------------- download

/// Download an asset into `dir`, checking its size and SHA-256 digest.
/// `progress(done, total)` is called as bytes arrive.
pub fn download(asset: &Asset, dir: &Path, progress: &dyn Fn(u64, u64)) -> Result<PathBuf, String> {
    if !asset.url.starts_with("https://github.com/") && !asset.url.starts_with("https://objects.githubusercontent.com/") {
        return Err(format!("refusing to download from {}", asset.url));
    }
    let name = Path::new(&asset.name).file_name().ok_or("bad asset name")?.to_owned();
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(&name);
    let part = dir.join(format!("{}.part", name.to_string_lossy()));
    let resp = agent().get(&asset.url).set("User-Agent", &user_agent()).call().map_err(|e| format!("download failed: {e}"))?;
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let k = reader.read(&mut buf).map_err(|e| format!("download interrupted: {e}"))?;
        if k == 0 {
            break;
        }
        hasher.update(&buf[..k]);
        file.write_all(&buf[..k]).map_err(|e| format!("{}: {e}", part.display()))?;
        done += k as u64;
        progress(done, asset.size);
    }
    drop(file);
    let fail = |msg: String| {
        let _ = std::fs::remove_file(&part);
        Err(msg)
    };
    if asset.size > 0 && done != asset.size {
        return fail(format!("download incomplete: {done} of {} bytes", asset.size));
    }
    let got: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if let Some(want) = &asset.sha256 {
        if *want != got {
            return fail(format!("download corrupted: SHA-256 {got}, expected {want}"));
        }
    }
    std::fs::rename(&part, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// Folder for downloaded installers.
pub fn download_dir() -> PathBuf {
    std::env::temp_dir().join("FantasyWorldMaker-updates")
}

/// Start a downloaded installer. The caller should exit right after, so the
/// installer can replace the running program.
pub fn launch_installer(path: &Path) -> Result<(), String> {
    if !cfg!(windows) {
        return Err("the installer runs on Windows only".into());
    }
    std::process::Command::new(path).spawn().map_err(|e| format!("could not start the installer: {e}"))?;
    Ok(())
}

/// The installer puts an uninstaller next to the program; the portable zip
/// has none.
pub fn is_installed_copy() -> bool {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("uninstall.exe").exists())).unwrap_or(false)
}

/// Open a GitHub page of this project in the default browser.
pub fn open_page(url: &str) -> Result<(), String> {
    let allowed = format!("https://github.com/{REPO}/");
    if !url.starts_with(&allowed) || url.chars().any(|c| c.is_whitespace() || c == '"' || c == '&' || c == '^' || c == '|') {
        return Err(format!("refusing to open {url}"));
    }
    let r = if cfg!(windows) {
        std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    r.map(|_| ()).map_err(|e| format!("could not open the browser: {e}"))
}

/// UI commands shared by the desktop app and `worldgen serve`: `app_version`,
/// `update_check` { include_prereleases? } and `update_open` { url }.
/// Returns None for other commands.
pub fn handle(cmd: &str, args: &serde_json::Value) -> Option<Result<serde_json::Value, String>> {
    Some(match cmd {
        "app_version" => Ok(serde_json::json!({ "version": current_version(), "repo": REPO, "installed": is_installed_copy() })),
        "update_check" => check(args["include_prereleases"].as_bool()).map(|c| serde_json::to_value(c).unwrap()),
        "update_open" => match args["url"].as_str() {
            Some(u) => open_page(u).map(|_| serde_json::json!({ "opened": u })),
            None => Err("argument `url` missing".into()),
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn versions_order_like_semver() {
        assert!(v("0.1.0-beta.2") < v("0.1.0-beta.10"));
        assert!(v("0.1.0-beta.10") < v("0.1.0-rc.1"));
        assert!(v("0.1.0-rc.1") < v("0.1.0"));
        assert!(v("0.1.0") < v("0.1.1-beta.1"));
        assert!(v("v0.2.0") > v("0.1.9"));
        assert!(v("0.1.0-beta") < v("0.1.0-beta.1"));
        assert_eq!(v("0.1.0+build5"), v("0.1.0"));
        assert!(v("0.1.0-dev") > v("0.1.0-beta.2"), "a dev build is not offered older betas");
        assert!(Version::parse("latest").is_none());
        assert!(Version::parse("1.2.3.4").is_none());
    }

    const SAMPLE: &str = r#"[
      {"tag_name":"v0.1.0-beta.2","name":"Fantasy World Maker v0.1.0-beta.2","html_url":"https://github.com/Carboxylate652/FantasyWorldBuilder/releases/tag/v0.1.0-beta.2",
       "draft":false,"prerelease":true,"published_at":"2026-10-08T18:00:00Z","body":"notes",
       "assets":[{"name":"Fantasy.World.Maker_0.1.0_x64-setup.exe","browser_download_url":"https://github.com/Carboxylate652/FantasyWorldBuilder/releases/download/v0.1.0-beta.2/Fantasy.World.Maker_0.1.0_x64-setup.exe","size":3000000,"digest":"sha256:ABCDEF"},
                 {"name":"FantasyWorldMaker-v0.1.0-beta.2-windows-x64-portable.zip","browser_download_url":"https://github.com/x/y.zip","size":5000000}]},
      {"tag_name":"v0.2.0","html_url":"https://github.com/Carboxylate652/FantasyWorldBuilder/releases/tag/v0.2.0","draft":true,"prerelease":false,"assets":[]},
      {"tag_name":"v0.1.0-beta.1","name":"","html_url":"https://github.com/Carboxylate652/FantasyWorldBuilder/releases/tag/v0.1.0-beta.1","prerelease":true,"assets":[]},
      {"tag_name":"nightly","html_url":"https://github.com/x","assets":[]}
    ]"#;

    #[test]
    fn releases_parse_and_pick_newest() {
        let r = parse_releases(SAMPLE).unwrap();
        assert_eq!(r.len(), 2, "draft and non-version tags are dropped");
        assert_eq!(r[1].name, "v0.1.0-beta.1", "empty name falls back to the tag");
        let inst = r[0].installer.as_ref().unwrap();
        assert_eq!(inst.sha256.as_deref(), Some("abcdef"));
        assert!(r[0].portable.as_ref().unwrap().sha256.is_none());
        assert_eq!(newest(&r, true).unwrap().tag, "v0.1.0-beta.2");
        assert!(newest(&r, false).is_none(), "no stable release yet");
    }

    #[test]
    fn download_checks_the_digest_and_source() {
        let bad = Asset { name: "x.exe".into(), url: "https://example.com/x.exe".into(), size: 1, sha256: None };
        assert!(download(&bad, &std::env::temp_dir(), &|_, _| {}).unwrap_err().contains("refusing"));
        assert!(open_page("https://example.com/").is_err());
        assert!(open_page("https://github.com/Carboxylate652/FantasyWorldBuilder/releases/tag/v1 & calc").is_err());
    }
}
