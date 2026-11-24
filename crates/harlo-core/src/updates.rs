//! Harlo's own updates: its newest release, read from a manifest signed
//! with Harlo's release key, and the package or AppImage that replaces
//! this one.
//!
//! The release workflow publishes `linux-release.json` with every release
//! (`packaging/release-manifests.sh`): the packages with their SHA-256 sums,
//! signed. Harlo trusts a manifest only when its signature checks out
//! against the key built in ([`harlo_update_signing`]), and a download only
//! when its sum matches the manifest's.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};
use std::time::Duration;

#[doc(inline)]
pub use harlo_update_signing::Asset;

/// Harlo's repository on GitHub.
pub const REPOSITORY: &str = "ArcticLatent/harlo";

/// The latest release's signed manifest: a fixed address, not GitHub's API,
/// which allows few requests an hour.
const MANIFEST: &str =
    "https://github.com/ArcticLatent/harlo/releases/latest/download/linux-release.json";

/// Where the tests point the manifest instead.
const MANIFEST_VAR: &str = "HARLO_UPDATE_MANIFEST_URL";

/// Where the tests point the changelog instead.
const CHANGELOG_VAR: &str = "HARLO_UPDATE_CHANGELOG_URL";

/// The most a manifest or a changelog may be: both are a few kilobytes.
const TEXT_LIMIT: usize = 1024 * 1024;

/// How long asking for a manifest or a changelog may take.
const TEXT_TIMEOUT: Duration = Duration::from_secs(15);

/// A Harlo release, from a manifest whose signature checked out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// Its version, such as `2.0.1`.
    pub version: String,
    /// Its packages.
    pub assets: Vec<Asset>,
}

impl Release {
    /// Reads `linux-release.json`, only if it's signed with Harlo's key and
    /// is Harlo's: its repository, and its tag the version's.
    ///
    /// # Errors
    ///
    /// When `json` isn't a manifest (a field missing), its signature isn't
    /// Harlo's, or it names another repository or a tag that isn't its
    /// version's.
    pub fn verified(json: &str) -> Result<Self, String> {
        Self::verified_by(json, &harlo_update_signing::harlo_public_key())
    }

    /// As [`Release::verified`], with the signature checked against
    /// `public_key` (base64) instead of Harlo's key.
    ///
    /// # Errors
    ///
    /// As [`Release::verified`].
    pub fn verified_by(json: &str, public_key: &str) -> Result<Self, String> {
        let v: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| format!("The release manifest isn't JSON: {e}"))?;
        let text = |key: &str| {
            v[key]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("The release manifest has no {key}."))
        };
        let (version, tag, repository) = (text("version")?, text("tag")?, text("repository")?);
        let assets = v["assets"]
            .as_array()
            .ok_or("The release manifest has no files.")?
            .iter()
            .map(|a| {
                let field = |key: &str| {
                    a[key]
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| format!("A file in the release manifest has no {key}."))
                };
                Ok(Asset {
                    name: field("name")?,
                    sha256: field("sha256")?,
                    download_url: field("download_url")?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let signature = v["signature"]
            .as_str()
            .ok_or("The release manifest isn't signed.")?;
        let payload = harlo_update_signing::signing_payload(&version, &tag, &repository, &assets);
        harlo_update_signing::verify(&payload, signature, public_key)
            .map_err(|e| format!("The release manifest's signature isn't Harlo's: {e}"))?;
        if repository != REPOSITORY || tag != format!("v{version}") {
            return Err(format!(
                "The release manifest is for {repository} {tag}, not Harlo {version}."
            ));
        }
        // Checked as signed; compared in lowercase.
        let assets = assets
            .into_iter()
            .map(|a| Asset {
                sha256: a.sha256.to_lowercase(),
                ..a
            })
            .collect();
        Ok(Self { version, assets })
    }

    /// The package for this way of installing Harlo.
    #[must_use]
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

/// The client the update checks use: short timeouts, Harlo's name.
fn http(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(concat!("harlo/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("Couldn't set up a connection: {e}"))
}

/// A short text from `url`, at most [`TEXT_LIMIT`] bytes.
async fn text(url: &str, what: &str) -> Result<String, String> {
    let response = http(TEXT_TIMEOUT)?
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("Couldn't reach Harlo's {what}: {e}"))?;
    if response
        .content_length()
        .is_some_and(|n| n > TEXT_LIMIT as u64)
    {
        return Err(format!("Harlo's {what} is too big to be one."));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Couldn't read Harlo's {what}: {e}"))?;
    if bytes.len() > TEXT_LIMIT {
        return Err(format!("Harlo's {what} is too big to be one."));
    }
    String::from_utf8(bytes.to_vec()).map_err(|e| format!("Harlo's {what} isn't text: {e}"))
}

/// The address set in `var`, when it is.
fn overridden(var: &str) -> Option<String> {
    std::env::var(var).ok().filter(|u| !u.trim().is_empty())
}

/// Harlo's newest release, its manifest's signature checked.
/// `HARLO_UPDATE_MANIFEST_URL` points elsewhere, as the tests do.
///
/// # Errors
///
/// When the manifest can't be downloaded or isn't verified (see
/// [`Release::verified`]).
pub async fn latest() -> Result<Release, String> {
    let url = overridden(MANIFEST_VAR).unwrap_or_else(|| MANIFEST.to_owned());
    latest_at(&url, &harlo_update_signing::harlo_public_key()).await
}

/// The release whose manifest is at `url`, its signature checked against
/// `public_key` (base64).
///
/// # Errors
///
/// As [`latest`].
pub async fn latest_at(url: &str, public_key: &str) -> Result<Release, String> {
    Release::verified_by(&text(url, "release manifest").await?, public_key)
}

/// What's new since `current`, up to `latest`: CHANGELOG.md at that
/// release's tag, read with [`changes_since`]. `HARLO_UPDATE_CHANGELOG_URL`
/// points elsewhere, as the tests do.
///
/// # Errors
///
/// When the changelog can't be downloaded.
pub async fn changes(current: &str, latest: &str) -> Result<String, String> {
    let url = overridden(CHANGELOG_VAR).unwrap_or_else(|| {
        format!("https://raw.githubusercontent.com/{REPOSITORY}/v{latest}/CHANGELOG.md")
    });
    changes_at(&url, current, latest).await
}

/// As [`changes`], with the changelog at `url`.
///
/// # Errors
///
/// When the changelog can't be downloaded.
pub async fn changes_at(url: &str, current: &str, latest: &str) -> Result<String, String> {
    Ok(changes_since(
        &text(url, "changelog").await?,
        current,
        latest,
    ))
}

/// The page listing every release.
#[must_use]
pub fn releases_url() -> String {
    format!("https://github.com/{REPOSITORY}/releases")
}

/// The CHANGELOG.md sections newer than `current`, up to `latest`, as
/// plain text: a line per item ("• " for a list's), subheadings left out,
/// each version named when there's more than one.
#[must_use]
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
            let version = version
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_start_matches('v')
                .to_owned();
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
        } else if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            flush(&mut item, &mut sections);
            item = Some(format!("• {}", plain(rest)));
        } else if let Some(text) = item.as_mut() {
            // A wrapped line goes on.
            text.push(' ');
            text.push_str(&plain(trimmed));
        } else {
            item = Some(plain(trimmed));
        }
    }
    flush(&mut item, &mut sections);
    let named = sections.len() > 1;
    let mut out = Vec::new();
    for (version, items) in sections.into_iter().filter(|(_, items)| !items.is_empty()) {
        if named {
            out.push(format!("Harlo {version}"));
        }
        out.extend(items);
    }
    out.join("\n")
}

/// Markdown's marks taken out: `code`, **bold**, [links](…).
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
#[must_use]
pub fn excerpt(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let cut: String = text.chars().take(max).collect();
    let end = cut
        .rfind(char::is_whitespace)
        .filter(|&i| i > max / 2)
        .unwrap_or(cut.len());
    format!(
        "{}…",
        cut[..end].trim_end_matches(
            |c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':' | '.' | '•')
        )
    )
}

/// The numbers of a version ("2.0.28-beta.1" → [2, 0, 28]).
fn numbers(v: &str) -> Vec<u64> {
    let core = v
        .trim()
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()
        .unwrap_or_default();
    if !core.starts_with(|c: char| c.is_ascii_digit()) {
        return Vec::new();
    }
    core.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// Whether `latest` is newer than `current`.
#[must_use]
pub fn newer(latest: &str, current: &str) -> bool {
    let (a, b) = (numbers(latest), numbers(current));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x > y;
        }
    }
    false
}

/// How this Harlo was installed: an AppImage (its file), or a package
/// (its kind, in `/opt/harlo/install-kind`). A Harlo built from source has
/// none, and isn't updated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallKind {
    /// An AppImage, at this path.
    AppImage(PathBuf),
    /// A Debian or Ubuntu package.
    Deb,
    /// A Fedora or openSUSE package.
    Rpm,
    /// An Arch Linux package.
    Arch,
}

impl InstallKind {
    /// From the running program and `$APPIMAGE`, which an AppImage sets.
    #[must_use]
    pub fn detect(exe: &Path, appimage: Option<&str>) -> Option<Self> {
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

    /// How this Harlo was installed, from the running program.
    #[must_use]
    pub fn current() -> Option<Self> {
        let exe = std::env::current_exe().ok()?;
        Self::detect(&exe, std::env::var("APPIMAGE").ok().as_deref())
    }

    /// The package manager's command that installs a downloaded package,
    /// or none for an AppImage, whose file is replaced. `has` says whether
    /// a program is on `PATH`.
    fn installer(&self, has: impl Fn(&str) -> bool) -> Option<Vec<&'static str>> {
        Some(match self {
            Self::AppImage(_) => return None,
            Self::Deb if has("apt-get") => vec!["apt-get", "install", "-y", "--allow-downgrades"],
            Self::Deb => vec!["dpkg", "-i"],
            Self::Rpm if has("dnf") => vec!["dnf", "install", "-y"],
            Self::Rpm if has("zypper") => vec![
                "zypper",
                "--non-interactive",
                "install",
                "--allow-unsigned-rpm",
            ],
            Self::Rpm => vec!["rpm", "-U"],
            Self::Arch => vec!["pacman", "-U", "--noconfirm"],
        })
    }

    /// The command that installs `file` with the rights it needs
    /// (`elevate`: see [`elevation`]), or none for an AppImage.
    #[must_use]
    pub fn install_command(
        &self,
        file: &Path,
        has: impl Fn(&str) -> bool,
        elevate: &[String],
    ) -> Option<Vec<String>> {
        let argv = self.installer(has)?;
        Some(
            elevate
                .iter()
                .cloned()
                .chain(argv.into_iter().map(str::to_owned))
                .chain(std::iter::once(file.to_string_lossy().into_owned()))
                .collect(),
        )
    }

    /// The same, to type in a terminal when installing it from Harlo failed.
    #[must_use]
    pub fn manual_command(&self, file: &Path, has: impl Fn(&str) -> bool) -> Option<String> {
        let argv = self.installer(has)?;
        Some(format!("sudo {} \"{}\"", argv.join(" "), file.display()))
    }
}

/// Whether a program is on `PATH`.
#[must_use]
pub fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
}

/// How to run a package manager as root: as it is (Harlo runs as root),
/// `sudo -n` (sudo needs no password here), else `pkexec` (the desktop's
/// password dialog).
pub async fn elevation() -> Vec<String> {
    if std::fs::metadata("/proc/self").is_ok_and(|m| std::os::unix::fs::MetadataExt::uid(&m) == 0) {
        return Vec::new();
    }
    let sudo_free = tokio::process::Command::new("sudo")
        .args(["-n", "true"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .is_ok_and(|s| s.success());
    if sudo_free {
        vec!["sudo".into(), "-n".into()]
    } else {
        vec!["pkexec".into()]
    }
}

/// Where downloaded updates wait to be installed.
#[must_use]
pub fn folder() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("harlo/updates")
}

/// Why an update couldn't be installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallError {
    /// What went wrong.
    pub message: String,
    /// The command that installs it from a terminal, when there is one.
    pub manual: Option<String>,
}

/// Puts the downloaded `file` in place: over the AppImage, or through the
/// system's package manager with the rights it needs.
///
/// # Errors
///
/// When the AppImage can't be replaced, or the package manager fails or
/// can't be started; the error says why and how to install it by hand.
pub async fn install(kind: &InstallKind, file: &Path) -> Result<(), InstallError> {
    let failed = |message: String| InstallError {
        message,
        manual: kind.manual_command(file, on_path),
    };
    if let InstallKind::AppImage(path) = kind {
        return replace_appimage(file, path).map_err(failed);
    }
    let command = kind
        .install_command(file, on_path, &elevation().await)
        .ok_or_else(|| failed("There's no installer for this kind of package.".to_owned()))?;
    let (program, rest) = command
        .split_first()
        .ok_or_else(|| failed("There's no installer for this kind of package.".to_owned()))?;
    let output = tokio::process::Command::new(program)
        .args(rest)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| failed(format!("Couldn't run {program}: {e}")))?;
    if output.status.success() {
        let _ = tokio::fs::remove_file(file).await;
        return Ok(());
    }
    let said = String::from_utf8_lossy(&output.stderr);
    let last = said
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    Err(failed(if last.is_empty() {
        format!("{program} didn't install it.")
    } else {
        format!("{program} didn't install it: {}", last.trim())
    }))
}

/// Copies the new AppImage beside the running one, then over it: a running
/// AppImage's file can be replaced.
fn replace_appimage(file: &Path, path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    let next = path.with_extension("new");
    std::fs::copy(file, &next).map_err(|e| format!("Couldn't write {}: {e}", next.display()))?;
    std::fs::set_permissions(&next, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("Couldn't make {} runnable: {e}", next.display()))?;
    std::fs::rename(&next, path).map_err(|e| {
        let _ = std::fs::remove_file(&next);
        format!("Couldn't replace {}: {e}", path.display())
    })?;
    let _ = std::fs::remove_file(file);
    Ok(())
}

/// The program that starts Harlo again: the AppImage's file, else this
/// program.
///
/// A package upgrade replaces the program while it runs, after which Linux
/// names it "<path> (deleted)"; the path itself is the new Harlo.
#[must_use]
pub fn this_program() -> Option<PathBuf> {
    if let Some(appimage) = std::env::var_os("APPIMAGE").filter(|a| !a.is_empty()) {
        return Some(appimage.into());
    }
    let exe = std::env::current_exe().ok()?;
    let exe = exe.to_string_lossy();
    Some(exe.strip_suffix(" (deleted)").unwrap_or(&exe).into())
}

/// Starts `program` once this process has exited, through a waiter outside
/// Harlo: a Harlo started while this one runs would only hand over to it.
///
/// # Errors
///
/// When the waiter can't be started.
pub fn relaunch_after_exit(program: &Path) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt as _;
    std::process::Command::new("sh")
        .process_group(0)
        .args([
            "-c",
            r#"while kill -0 "$1" 2>/dev/null; do sleep 0.2; done; shift; exec "$@""#,
            "sh",
            &std::process::id().to_string(),
        ])
        .arg(program)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(newer("2.0.28", "2.0.25"));
        assert!(newer("18.10.4", "18.8.9"));
        assert!(newer("2.0.0", "1.1.1"));
        assert!(!newer("2.1.299", "2.1.299"));
        assert!(!newer("1.19.0", "2.0.25"));
        assert!(!newer("2.0.28", ""));
        assert!(newer("v3.0", "2.9.9"));
        assert!(!newer("2.0.28-beta.1", "2.0.28"));
    }

    #[test]
    fn changes_since_your_version() {
        let log = "# Changelog\n\nIntro, not a release.\n\n## 1.0.2\n\n- Check now, in Settings.\n\n## 1.0.1 - 2026-10-25\n\n- Agents: denying a permission\n  ends the run (`shutdown`).\n- **Archiving** hides it; see [the docs](https://x.y/z).\n\n## 1.0.0\n\nThe first release.\n\n### Agents\n\n- Claude Code.\n";
        assert_eq!(
            changes_since(log, "1.0.1", "1.0.2"),
            "• Check now, in Settings."
        );
        assert_eq!(
            changes_since(log, "1.0.0", "1.0.2"),
            "Harlo 1.0.2\n• Check now, in Settings.\nHarlo 1.0.1\n• Agents: denying a permission ends the run (shutdown).\n• Archiving hides it; see the docs."
        );
        // A paragraph is an item too; subheadings are left out.
        assert_eq!(
            changes_since(log, "0.9.0", "1.0.0"),
            "The first release.\n• Claude Code."
        );
        // Nothing newer, or a release missing from it: nothing.
        assert_eq!(changes_since(log, "1.0.2", "1.0.2"), "");
        assert_eq!(changes_since(log, "1.0.2", "1.0.3"), "");
    }

    #[test]
    fn excerpts_cut_at_a_word() {
        assert_eq!(excerpt("short", 20), "short");
        assert_eq!(
            excerpt("• Archiving hides it, unless it's working", 24),
            "• Archiving hides it…"
        );
    }

    /// Each release in Harlo's own CHANGELOG.md reads as notes.
    #[test]
    fn harlos_changelog_reads() {
        let log = include_str!("../../../CHANGELOG.md");
        let versions: Vec<&str> = log
            .lines()
            .filter_map(|l| l.strip_prefix("## "))
            .filter_map(|l| l.split_whitespace().next())
            .collect();
        assert!(!versions.is_empty());
        // Newest first: each against the one before it (the first against 0).
        for (i, v) in versions.iter().enumerate() {
            let before = versions.get(i + 1).copied().unwrap_or("0");
            let notes = changes_since(log, before, v);
            assert!(
                !notes.is_empty() && !notes.starts_with(&format!("Harlo {v}")),
                "{v}: {notes:?}"
            );
        }
    }

    #[test]
    fn install_commands() {
        let f = Path::new("/tmp/h.rpm");
        let pk = vec!["pkexec".to_owned()];
        assert_eq!(
            InstallKind::Rpm
                .install_command(f, |p| p == "zypper", &pk)
                .map(|a| a.join(" ")),
            Some("pkexec zypper --non-interactive install --allow-unsigned-rpm /tmp/h.rpm".into())
        );
        assert_eq!(
            InstallKind::Arch
                .install_command(f, |_| true, &[])
                .map(|a| a.join(" ")),
            Some("pacman -U --noconfirm /tmp/h.rpm".into()),
            "as root"
        );
        assert!(
            InstallKind::AppImage("/a".into())
                .install_command(f, |_| true, &pk)
                .is_none()
        );
        assert_eq!(
            InstallKind::Deb.manual_command(Path::new("/d/h.deb"), |p| p == "apt-get"),
            Some("sudo apt-get install -y --allow-downgrades \"/d/h.deb\"".into())
        );
    }

    #[test]
    fn how_it_was_installed() {
        assert_eq!(
            InstallKind::detect(Path::new("/x/bin/harlo"), Some("/home/me/harlo.AppImage")),
            Some(InstallKind::AppImage("/home/me/harlo.AppImage".into()))
        );
        let root = tempfile::tempdir().expect("a folder");
        std::fs::create_dir_all(root.path().join("bin")).expect("bin");
        let exe = root.path().join("bin/harlo");
        assert_eq!(InstallKind::detect(&exe, None), None, "built from source");
        std::fs::write(root.path().join("install-kind"), "rpm\n").expect("marker");
        assert_eq!(InstallKind::detect(&exe, None), Some(InstallKind::Rpm));
    }
}
