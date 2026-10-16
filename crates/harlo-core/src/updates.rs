//! Agent updates: the newest version of each agent (from the npm registry,
//! where all four publish theirs) against the installed one, and the
//! agent's own command to update itself.

use std::time::Duration;

/// The npm package each agent is published as.
fn package(agent: &str) -> Option<&'static str> {
    match agent {
        "claude" => Some("@anthropic-ai/claude-code"),
        // (OpenCode 2; `opencode-ai` is the old 1.x line.)
        "opencode" => Some("@opencode/cli"),
        "pi" => Some("@earendil-works/pi-coding-agent"),
        "omp" => Some("@oh-my-pi/pi-coding-agent"),
        _ => None,
    }
}

/// The agent's command to update itself (arguments after the program).
pub fn update_args(agent: &str) -> &'static [&'static str] {
    match agent {
        "claude" => &["update"],
        "opencode" => &["upgrade"],
        "pi" => &["update", "--self"],
        "omp" => &["update"],
        _ => &[],
    }
}

/// The newest published version of an agent, or None (offline, say).
/// `HARLO_TEST_LATEST=claude=2.1.300,opencode=…` answers for the tests.
pub fn latest(agent: &str) -> Option<String> {
    if let Ok(test) = std::env::var("HARLO_TEST_LATEST") {
        return test.split(',').find_map(|kv| kv.split_once('=').filter(|(k, _)| *k == agent).map(|(_, v)| v.to_owned()));
    }
    let url = format!("https://registry.npmjs.org/{}/latest", package(agent)?);
    let agent_http: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build().into();
    let body = agent_http.get(&url).header("Accept", "application/json").call().ok()?.body_mut().read_to_string().ok()?;
    let v: serde_json::Value = serde_json::from_str(&body).ok()?;
    v["version"].as_str().map(|s| s.trim().trim_start_matches('v').to_owned()).filter(|s| !s.is_empty())
}

// ---- harlo itself -------------------------------------------------------------

/// Where harlo's releases are (GitHub's API for the repository's latest).
const RELEASES: &str = "https://api.github.com/repos/ArcticLatent/harlo/releases/latest";

/// A harlo release: its version and its files (name, download address).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub assets: Vec<(String, String)>,
}

impl Release {
    /// From GitHub's release JSON.
    pub fn parse(v: &serde_json::Value) -> Option<Self> {
        let version = v["tag_name"].as_str()?.trim().trim_start_matches('v').to_owned();
        let assets = v["assets"].as_array()?.iter()
            .filter_map(|a| Some((a["name"].as_str()?.to_owned(), a["browser_download_url"].as_str()?.to_owned())))
            .collect();
        (!version.is_empty()).then_some(Self { version, assets })
    }

    /// The file for this way of installing it.
    pub fn asset(&self, kind: &InstallKind) -> Option<&(String, String)> {
        let suffix = match kind {
            InstallKind::Deb => "_amd64.deb",
            InstallKind::Rpm => ".x86_64.rpm",
            InstallKind::Arch => "-x86_64.pkg.tar.zst",
            InstallKind::AppImage(_) => "-x86_64.AppImage",
        };
        self.assets.iter().find(|(n, _)| n.ends_with(suffix))
    }

    pub fn sums(&self) -> Option<&(String, String)> {
        self.assets.iter().find(|(n, _)| n == "SHA256SUMS")
    }
}

/// harlo's newest release, or None (offline, say). `HARLO_TEST_RELEASE`
/// (an address) answers for the tests.
pub fn harlo_release() -> Option<Release> {
    let url = std::env::var("HARLO_TEST_RELEASE").unwrap_or_else(|_| RELEASES.to_owned());
    let http: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(15))).build().into();
    let body = http.get(&url).header("Accept", "application/vnd.github+json").header("User-Agent", "harlo").call().ok()?.body_mut().read_to_string().ok()?;
    Release::parse(&serde_json::from_str(&body).ok()?)
}

/// How this harlo was installed: an AppImage (its file), or a package
/// (its kind, in `/opt/harlo/install-kind`). None: built from source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallKind {
    AppImage(std::path::PathBuf),
    Deb,
    Rpm,
    Arch,
}

impl InstallKind {
    /// From the running program and `$APPIMAGE` (set by an AppImage).
    pub fn detect(exe: &std::path::Path, appimage: Option<&str>) -> Option<Self> {
        if let Some(path) = appimage.filter(|p| !p.is_empty()) {
            return Some(Self::AppImage(path.into()));
        }
        let marker = exe.parent()?.parent()?.join("install-kind");
        match std::fs::read_to_string(marker).ok()?.trim() {
            "deb" => Some(Self::Deb),
            "rpm" => Some(Self::Rpm),
            "arch" => Some(Self::Arch),
            _ => None,
        }
    }

    /// The command that installs a downloaded package (with the system's
    /// password dialog), or None for an AppImage (its file is replaced).
    /// `has` says whether a program is on PATH.
    pub fn install_command(&self, file: &std::path::Path, has: impl Fn(&str) -> bool) -> Option<Vec<String>> {
        let f = file.to_string_lossy().into_owned();
        let argv: Vec<&str> = match self {
            Self::AppImage(_) => return None,
            Self::Deb if has("apt-get") => vec!["apt-get", "install", "-y", "--allow-downgrades"],
            Self::Deb => vec!["dpkg", "-i"],
            Self::Rpm if has("dnf") => vec!["dnf", "install", "-y"],
            Self::Rpm if has("zypper") => vec!["zypper", "--non-interactive", "install", "--allow-unsigned-rpm"],
            Self::Rpm => vec!["rpm", "-U"],
            Self::Arch => vec!["pacman", "-U", "--noconfirm"],
        };
        Some(std::iter::once("pkexec").chain(argv).map(str::to_owned).chain(std::iter::once(f)).collect())
    }
}

/// The SHA-256 a `SHA256SUMS` file gives for `name`.
pub fn sum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, file) = l.split_once(char::is_whitespace)?;
        (file.trim().trim_start_matches('*') == name).then(|| hash.trim().to_lowercase())
    })
}

/// Downloads `url` to `dest`: its SHA-256 (hex).
pub fn download(url: &str, dest: &std::path::Path) -> anyhow::Result<String> {
    use anyhow::Context;
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    let http: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(600))).build().into();
    let mut resp = http.get(url).header("User-Agent", "harlo").call().with_context(|| format!("downloading {url}"))?;
    let mut reader = resp.body_mut().with_config().limit(1024 * 1024 * 1024).reader();
    let mut file = std::fs::File::create(dest).with_context(|| format!("writing {}", dest.display()))?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        file.write_all(&buf[..n])?;
    }
    file.flush()?;
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// The numbers of a version ("2.0.28-beta.1" → [2, 0, 28]).
fn numbers(v: &str) -> Vec<u64> {
    let core = v.trim().trim_start_matches('v').split(['-', '+']).next().unwrap_or_default();
    if !core.starts_with(|c: char| c.is_ascii_digit()) {
        return Vec::new();
    }
    core.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// `latest` is newer than `current`.
pub fn newer(latest: &str, current: &str) -> bool {
    let (a, b) = (numbers(latest), numbers(current));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harlo_releases() {
        let r = Release::parse(&serde_json::json!({ "tag_name": "v1.2.0", "assets": [
            { "name": "harlo_1.2.0-1_amd64.deb", "browser_download_url": "https://x/deb" },
            { "name": "harlo-1.2.0-1.x86_64.rpm", "browser_download_url": "https://x/rpm" },
            { "name": "harlo-1.2.0-1-x86_64.pkg.tar.zst", "browser_download_url": "https://x/arch" },
            { "name": "harlo-1.2.0-x86_64.AppImage", "browser_download_url": "https://x/appimage" },
            { "name": "SHA256SUMS", "browser_download_url": "https://x/sums" } ] })).unwrap();
        assert_eq!(r.version, "1.2.0");
        assert_eq!(r.asset(&InstallKind::Deb).unwrap().1, "https://x/deb");
        assert_eq!(r.asset(&InstallKind::Rpm).unwrap().1, "https://x/rpm");
        assert_eq!(r.asset(&InstallKind::Arch).unwrap().1, "https://x/arch");
        assert_eq!(r.asset(&InstallKind::AppImage("/a".into())).unwrap().1, "https://x/appimage");
        assert_eq!(r.sums().unwrap().1, "https://x/sums");
        assert_eq!(sum_for("abc  harlo_1.2.0-1_amd64.deb\nDEF *harlo-1.2.0-x86_64.AppImage\n", "harlo-1.2.0-x86_64.AppImage").as_deref(), Some("def"));
        assert_eq!(sum_for("abc  other", "harlo"), None);
        let f = std::path::Path::new("/tmp/h.rpm");
        assert_eq!(InstallKind::Rpm.install_command(f, |p| p == "zypper").unwrap().join(" "), "pkexec zypper --non-interactive install --allow-unsigned-rpm /tmp/h.rpm");
        assert_eq!(InstallKind::Deb.install_command(f, |p| p == "apt-get").unwrap()[..2], ["pkexec", "apt-get"]);
        assert_eq!(InstallKind::Arch.install_command(f, |_| true).unwrap().join(" "), "pkexec pacman -U --noconfirm /tmp/h.rpm");
        assert!(InstallKind::AppImage("/a".into()).install_command(f, |_| true).is_none());
        // How it was installed.
        assert_eq!(InstallKind::detect(std::path::Path::new("/x/bin/harlo"), Some("/home/me/harlo.AppImage")), Some(InstallKind::AppImage("/home/me/harlo.AppImage".into())));
        let root = std::env::temp_dir().join(format!("harlo-kind-{}", std::process::id()));
        std::fs::create_dir_all(root.join("bin")).unwrap();
        assert_eq!(InstallKind::detect(&root.join("bin/harlo"), None), None, "built from source");
        std::fs::write(root.join("install-kind"), "rpm\n").unwrap();
        assert_eq!(InstallKind::detect(&root.join("bin/harlo"), None), Some(InstallKind::Rpm));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn compares_versions() {
        assert!(newer("2.0.28", "2.0.25"));
        assert!(newer("18.10.4", "18.8.9"));
        assert!(newer("1.2.0", "1.0.4"));
        assert!(!newer("2.1.299", "2.1.299"));
        assert!(!newer("1.19.0", "2.0.25"));
        assert!(!newer("2.0.28", ""));
        assert!(newer("v3.0", "2.9.9"));
        assert!(!newer("2.0.28-beta.1", "2.0.28"));
    }

    #[test]
    fn every_agent_has_a_package_and_an_update() {
        for a in crate::remote::AGENTS {
            assert!(package(a).is_some() && !update_args(a).is_empty(), "{a}");
        }
    }
}
