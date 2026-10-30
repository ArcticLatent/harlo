//! Agent updates: the newest version of each agent (from the npm registry,
//! where the four JavaScript ones publish theirs; Hermes' from its GitHub
//! releases) against the installed one, and the agent's own command to
//! update itself.

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

/// Where an agent's newest version is published.
fn latest_url(agent: &str) -> Option<String> {
    match agent {
        // (GitHub's API: once an hour at most, well inside its allowance.)
        "hermes" => Some("https://api.github.com/repos/NousResearch/hermes-agent/releases/latest".to_owned()),
        _ => package(agent).map(|p| format!("https://registry.npmjs.org/{p}/latest")),
    }
}

/// The agent's command to update itself (arguments after the program).
pub fn update_args(agent: &str) -> &'static [&'static str] {
    match agent {
        "claude" => &["update"],
        "opencode" => &["upgrade"],
        "pi" => &["update", "--self"],
        "omp" => &["update"],
        "hermes" => &["update", "--yes"],
        _ => &[],
    }
}

/// The newest published version of an agent, or None (offline, say).
/// `HARLO_TEST_LATEST=claude=2.1.300,opencode=…` answers for the tests.
pub fn latest(agent: &str) -> Option<String> {
    if let Ok(test) = std::env::var("HARLO_TEST_LATEST") {
        return test.split(',').find_map(|kv| kv.split_once('=').filter(|(k, _)| *k == agent).map(|(_, v)| v.to_owned()));
    }
    let url = latest_url(agent)?;
    let agent_http: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build().into();
    let body = agent_http.get(&url).header("Accept", "application/json").header("User-Agent", concat!("harlo/", env!("CARGO_PKG_VERSION"))).call().ok()?.body_mut().read_to_string().ok()?;
    let v: serde_json::Value = serde_json::from_str(&body).ok()?;
    v["version"].as_str().or_else(|| v["tag_name"].as_str()).map(|s| s.trim().trim_start_matches('v').to_owned()).filter(|s| !s.is_empty())
}

// ---- harlo itself -------------------------------------------------------------

/// harlo's repository, and its latest release's signed manifest: the
/// packages with their SHA-256 sums (`packaging/release-manifests.sh`).
/// A fixed address, not GitHub's API, which allows few requests an hour.
const REPOSITORY: &str = "ArcticLatent/harlo";
const MANIFEST: &str = "https://github.com/ArcticLatent/harlo/releases/latest/download/linux-release.json";

pub use harlo_update_signing::Asset;

/// A harlo release, from a manifest whose signature checked out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub assets: Vec<Asset>,
}

impl Release {
    /// From `linux-release.json`, only if it's signed with harlo's key (and
    /// is harlo's: its repository, its tag the version's).
    pub fn verified(json: &str) -> anyhow::Result<Self> {
        use anyhow::{Context, bail};
        let v: serde_json::Value = serde_json::from_str(json).context("the release manifest isn't JSON")?;
        let text = |key: &str| v[key].as_str().map(str::to_owned).with_context(|| format!("the release manifest has no {key}"));
        let (version, tag, repository) = (text("version")?, text("tag")?, text("repository")?);
        let assets = v["assets"]
            .as_array()
            .context("the release manifest has no files")?
            .iter()
            .map(|a| {
                let field = |key: &str| a[key].as_str().map(str::to_owned).with_context(|| format!("a file in the release manifest has no {key}"));
                Ok(Asset { name: field("name")?, sha256: field("sha256")?, download_url: field("download_url")? })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let signature = v["signature"].as_str().context("the release manifest isn't signed")?;
        let payload = harlo_update_signing::signing_payload(&version, &tag, &repository, &assets);
        harlo_update_signing::verify_with_harlo_key(&payload, signature).context("the release manifest's signature isn't harlo's")?;
        if repository != REPOSITORY || tag != format!("v{version}") {
            bail!("the release manifest is for {repository} {tag}, not harlo {version}");
        }
        // (Checked as signed; compared in lowercase.)
        let assets = assets.into_iter().map(|a| Asset { sha256: a.sha256.to_lowercase(), ..a }).collect();
        Ok(Self { version, assets })
    }

    /// The package for this way of installing harlo.
    pub fn asset(&self, kind: &InstallKind) -> Option<&Asset> {
        let suffix = match kind {
            InstallKind::Deb => "_amd64.deb",
            InstallKind::Rpm => ".x86_64.rpm",
            InstallKind::Arch => "-x86_64.pkg.tar.zst",
            InstallKind::AppImage(_) => "-x86_64.AppImage",
        };
        self.assets.iter().find(|a| a.name.ends_with(suffix))
    }
}

/// harlo's newest release, its manifest's signature checked.
/// `HARLO_UPDATE_MANIFEST_URL` points elsewhere (the tests' stand-in).
pub fn harlo_release() -> anyhow::Result<Release> {
    use anyhow::Context;
    let url = std::env::var("HARLO_UPDATE_MANIFEST_URL").ok().filter(|u| !u.trim().is_empty()).unwrap_or_else(|| MANIFEST.to_owned());
    let http: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(15))).build().into();
    let body = http
        .get(&url)
        .header("User-Agent", concat!("harlo/", env!("CARGO_PKG_VERSION")))
        .call()
        .context("couldn't reach harlo's releases")?
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()
        .context("couldn't read harlo's release manifest")?;
    Release::verified(&body)
}

/// The full changelog (as of the release `latest`), on GitHub.
pub fn harlo_changelog_url(latest: &str) -> String {
    format!("https://github.com/{REPOSITORY}/blob/v{latest}/CHANGELOG.md")
}

/// What's new since `current`, up to `latest`: CHANGELOG.md at that
/// release's tag, read with [`changes_since`]. `HARLO_UPDATE_CHANGELOG_URL`
/// points elsewhere (the tests' stand-in).
pub fn harlo_changes(current: &str, latest: &str) -> anyhow::Result<String> {
    use anyhow::Context;
    let url = std::env::var("HARLO_UPDATE_CHANGELOG_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| format!("https://raw.githubusercontent.com/{REPOSITORY}/v{latest}/CHANGELOG.md"));
    let http: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(15))).build().into();
    let text = http
        .get(&url)
        .header("User-Agent", concat!("harlo/", env!("CARGO_PKG_VERSION")))
        .call()
        .context("couldn't reach harlo's changelog")?
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()
        .context("couldn't read harlo's changelog")?;
    Ok(changes_since(&text, current, latest))
}

/// The CHANGELOG.md sections newer than `current`, up to `latest`, as
/// plain text: a line per item ("• " for a list's), subheadings left out,
/// each version named when there's more than one.
pub fn changes_since(changelog: &str, current: &str, latest: &str) -> String {
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    let mut taking = false;
    let mut item: Option<String> = None;
    let flush = |item: &mut Option<String>, sections: &mut Vec<(String, Vec<String>)>| {
        if let (Some(text), Some(last)) = (item.take(), sections.last_mut()) {
            last.1.push(text);
        }
    };
    for line in changelog.lines() {
        let trimmed = line.trim();
        if let Some(version) = line.strip_prefix("## ") {
            flush(&mut item, &mut sections);
            let version = version.trim().trim_start_matches('v').to_owned();
            taking = newer(&version, current) && !newer(&version, latest);
            if taking {
                sections.push((version, Vec::new()));
            }
            continue;
        }
        if !taking {
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            flush(&mut item, &mut sections);
        } else if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            flush(&mut item, &mut sections);
            item = Some(format!("• {}", plain(rest)));
        } else {
            // A wrapped line goes on; else a paragraph starts.
            match item.as_mut() {
                Some(text) => {
                    text.push(' ');
                    text.push_str(&plain(trimmed));
                }
                None => item = Some(plain(trimmed)),
            }
        }
    }
    flush(&mut item, &mut sections);
    let named = sections.len() > 1;
    let mut out = Vec::new();
    for (version, items) in sections.into_iter().filter(|(_, items)| !items.is_empty()) {
        if named {
            out.push(format!("harlo {version}"));
        }
        out.extend(items);
    }
    out.join("\n")
}

/// Markdown's marks out: `code`, **bold**, [links](…).
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        let (before, link) = rest.split_at(start);
        out.push_str(before);
        match (link.find("]("), link.find(')')) {
            (Some(close), Some(end)) if close < end => {
                out.push_str(&link[1..close]);
                rest = &link[end + 1..];
            }
            _ => {
                out.push('[');
                rest = &link[1..];
            }
        }
    }
    out.push_str(rest);
    out.replace("**", "").replace('`', "")
}

/// At most `max` characters of `text`, cut at a word, "…" if cut.
pub fn excerpt(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let cut: String = text.chars().take(max).collect();
    let end = cut.rfind(char::is_whitespace).filter(|&i| i > max / 2).unwrap_or(cut.len());
    format!("{}…", cut[..end].trim_end_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':' | '.' | '•')))
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

    /// The package manager's command that installs a downloaded package,
    /// or None for an AppImage (its file is replaced). `has` says whether a
    /// program is on PATH.
    fn installer(&self, has: impl Fn(&str) -> bool) -> Option<Vec<&'static str>> {
        Some(match self {
            Self::AppImage(_) => return None,
            Self::Deb if has("apt-get") => vec!["apt-get", "install", "-y", "--allow-downgrades"],
            Self::Deb => vec!["dpkg", "-i"],
            Self::Rpm if has("dnf") => vec!["dnf", "install", "-y"],
            Self::Rpm if has("zypper") => vec!["zypper", "--non-interactive", "install", "--allow-unsigned-rpm"],
            Self::Rpm => vec!["rpm", "-U"],
            Self::Arch => vec!["pacman", "-U", "--noconfirm"],
        })
    }

    /// The command that installs a downloaded package with the rights it
    /// needs (`elevate`: see [`elevation`]), or None for an AppImage.
    pub fn install_command(&self, file: &std::path::Path, has: impl Fn(&str) -> bool, elevate: &[String]) -> Option<Vec<String>> {
        let argv = self.installer(has)?;
        Some(elevate.iter().cloned().chain(argv.into_iter().map(str::to_owned)).chain(std::iter::once(file.to_string_lossy().into_owned())).collect())
    }

    /// The same, to type in a terminal (when installing it here failed).
    pub fn manual_command(&self, file: &std::path::Path, has: impl Fn(&str) -> bool) -> Option<String> {
        let argv = self.installer(has)?;
        Some(format!("sudo {} \"{}\"", argv.join(" "), file.display()))
    }
}

/// How to run a package manager as root: as it is (harlo runs as root),
/// `sudo -n` (sudo needs no password here), else `pkexec` (the desktop's
/// password dialog).
pub fn elevation() -> Vec<String> {
    use std::os::unix::fs::MetadataExt;
    if std::fs::metadata("/proc/self").is_ok_and(|m| m.uid() == 0) {
        return Vec::new();
    }
    let sudo_free = std::process::Command::new("sudo")
        .args(["-n", "true"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if sudo_free { vec!["sudo".into(), "-n".into()] } else { vec!["pkexec".into()] }
}

/// The program to start harlo again: the AppImage (its file), else this
/// program. A package upgrade replaces it while it runs, after which
/// Linux names it "<path> (deleted)"; the path itself is the new harlo.
pub fn this_program() -> Option<std::path::PathBuf> {
    if let Some(appimage) = std::env::var_os("APPIMAGE").filter(|a| !a.is_empty()) {
        return Some(appimage.into());
    }
    let exe = std::env::current_exe().ok()?;
    let exe = exe.to_string_lossy();
    Some(exe.strip_suffix(" (deleted)").unwrap_or(&exe).into())
}

/// Starts `program` once this process has exited: a waiter outside harlo
/// (a harlo started while this one runs would only hand over to it).
pub fn relaunch_after_exit(program: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    std::process::Command::new("sh")
        .process_group(0)
        .args(["-c", r#"while kill -0 "$1" 2>/dev/null; do sleep 0.2; done; shift; exec "$@""#, "sh", &std::process::id().to_string()])
        .arg(program)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
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

    /// A manifest as the release workflow writes it, signed with a
    /// throwaway key (`HARLO_UPDATE_PUBLIC_KEY` makes harlo trust it).
    fn manifest(version: &str, repository: &str, key: &[u8; 32]) -> String {
        use base64::{Engine, engine::general_purpose::STANDARD};
        use ed25519_dalek::{Signer, SigningKey};
        let names = [format!("harlo_{version}-1_amd64.deb"), format!("harlo-{version}-1.x86_64.rpm"), format!("harlo-{version}-1-x86_64.pkg.tar.zst"), format!("harlo-{version}-x86_64.AppImage")];
        let assets: Vec<Asset> = names.iter().map(|n| Asset { name: n.clone(), sha256: format!("AB{}", n.len()), download_url: format!("https://x/{n}") }).collect();
        let tag = format!("v{version}");
        let signing = SigningKey::from_bytes(key);
        let signature = STANDARD.encode(signing.sign(&harlo_update_signing::signing_payload(version, &tag, repository, &assets)).to_bytes());
        serde_json::json!({ "version": version, "tag": tag, "repository": repository, "signature": signature,
            "assets": assets.iter().map(|a| serde_json::json!({ "name": a.name, "sha256": a.sha256, "download_url": a.download_url })).collect::<Vec<_>>() }).to_string()
    }

    #[test]
    fn harlo_releases() {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let test_key = [5u8; 32];
        // SAFETY: only this test reads HARLO_UPDATE_PUBLIC_KEY.
        unsafe { std::env::set_var("HARLO_UPDATE_PUBLIC_KEY", STANDARD.encode(ed25519_dalek::SigningKey::from_bytes(&test_key).verifying_key().to_bytes())) };
        let r = Release::verified(&manifest("1.2.0", "ArcticLatent/harlo", &test_key)).unwrap();
        assert_eq!(r.version, "1.2.0");
        assert_eq!(r.asset(&InstallKind::Deb).unwrap().download_url, "https://x/harlo_1.2.0-1_amd64.deb");
        assert_eq!(r.asset(&InstallKind::Rpm).unwrap().name, "harlo-1.2.0-1.x86_64.rpm");
        assert_eq!(r.asset(&InstallKind::Arch).unwrap().name, "harlo-1.2.0-1-x86_64.pkg.tar.zst");
        let appimage = r.asset(&InstallKind::AppImage("/a".into())).unwrap();
        assert_eq!((appimage.name.as_str(), appimage.sha256.as_str()), ("harlo-1.2.0-x86_64.AppImage", "ab27"), "sums are lowercased");
        // Signed by another key, or changed after signing: refused.
        assert!(Release::verified(&manifest("1.2.0", "ArcticLatent/harlo", &[6u8; 32])).is_err());
        let changed = manifest("1.2.0", "ArcticLatent/harlo", &test_key).replace("https://x/harlo-1.2.0-x86_64.AppImage", "https://evil/harlo");
        assert!(Release::verified(&changed).is_err());
        // Unsigned: refused.
        let mut unsigned: serde_json::Value = serde_json::from_str(&manifest("1.2.0", "ArcticLatent/harlo", &test_key)).unwrap();
        unsigned.as_object_mut().unwrap().remove("signature");
        assert!(Release::verified(&unsigned.to_string()).is_err());
        // Signed, but another project's.
        assert!(Release::verified(&manifest("1.2.0", "someone/else", &test_key)).is_err());
        // SAFETY: as above.
        unsafe { std::env::remove_var("HARLO_UPDATE_PUBLIC_KEY") };

        let f = std::path::Path::new("/tmp/h.rpm");
        let pk = vec!["pkexec".to_owned()];
        assert_eq!(InstallKind::Rpm.install_command(f, |p| p == "zypper", &pk).unwrap().join(" "), "pkexec zypper --non-interactive install --allow-unsigned-rpm /tmp/h.rpm");
        assert_eq!(InstallKind::Deb.install_command(f, |p| p == "apt-get", &["sudo".into(), "-n".into()]).unwrap()[..3], ["sudo", "-n", "apt-get"]);
        assert_eq!(InstallKind::Arch.install_command(f, |_| true, &[]).unwrap().join(" "), "pacman -U --noconfirm /tmp/h.rpm", "as root");
        assert!(InstallKind::AppImage("/a".into()).install_command(f, |_| true, &pk).is_none());
        assert_eq!(InstallKind::Deb.manual_command(std::path::Path::new("/d/h.deb"), |p| p == "apt-get").unwrap(), "sudo apt-get install -y --allow-downgrades \"/d/h.deb\"");
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
    fn every_agent_has_a_source_and_an_update() {
        for a in crate::remote::AGENTS {
            assert!(latest_url(a).is_some() && !update_args(a).is_empty(), "{a}");
        }
    }

    #[test]
    fn changes_since_your_version() {
        let log = "# Changelog\n\nIntro, not a release.\n\n## 1.0.2\n\n- Check now, in Settings.\n\n## 1.0.1\n\n- OpenCode: denying a permission\n  ends the run (`shutdown`).\n- **Archiving** hides it; see [the docs](https://x.y/z).\n\n## 1.0.0\n\nThe first release.\n\n### Agents\n\n- Claude Code.\n";
        assert_eq!(changes_since(log, "1.0.1", "1.0.2"), "• Check now, in Settings.");
        assert_eq!(
            changes_since(log, "1.0.0", "1.0.2"),
            "harlo 1.0.2\n• Check now, in Settings.\nharlo 1.0.1\n• OpenCode: denying a permission ends the run (shutdown).\n• Archiving hides it; see the docs."
        );
        // A paragraph is an item too; subheadings are left out.
        assert_eq!(changes_since(log, "0.9.0", "1.0.0"), "The first release.\n• Claude Code.");
        // Nothing newer (or a release missing from it): nothing.
        assert_eq!(changes_since(log, "1.0.2", "1.0.2"), "");
        assert_eq!(changes_since(log, "1.0.2", "1.0.3"), "");
    }

    #[test]
    fn excerpts_cut_at_a_word() {
        assert_eq!(excerpt("short", 20), "short");
        assert_eq!(excerpt("• Archiving hides it, unless it's working", 24), "• Archiving hides it…");
    }

    /// Each release in harlo's own CHANGELOG.md reads as notes.
    #[test]
    fn harlos_changelog_reads() {
        let log = include_str!("../../../CHANGELOG.md");
        let versions: Vec<&str> = log.lines().filter_map(|l| l.strip_prefix("## ")).map(str::trim).collect();
        assert!(!versions.is_empty());
        // Newest first: each against the one before it (the first against 0).
        for (i, v) in versions.iter().enumerate() {
            let before = versions.get(i + 1).copied().unwrap_or("0");
            let notes = changes_since(log, before, v);
            assert!(!notes.is_empty() && !notes.starts_with("harlo "), "{v}: {notes:?}");
        }
    }
}
