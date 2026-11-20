//! harlo's built-in Claude subscription extension for pi (pi-claude-bridge).
//!
//! harlo ships the extension (`packaging/pi-claude-bridge/build.sh`) and
//! loads it into every pi it starts here. Anthropic's Agent SDK, which it
//! runs Claude Code through, isn't shipped: the first time, [`prepare`]
//! copies the shipped tree into harlo's data folder and downloads the SDK
//! there, checked against the integrity pinned at build time. The SDK runs
//! the user's own Claude Code; for someone without one, [`prepare_claude`]
//! downloads the SDK's copy (about 150 MB), which also signs them in.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::Value;

/// The setting that turns it off ("0"); on otherwise.
pub const SETTING: &str = "pi_claude_bridge";

/// The extension's package name, as pi lists it when installed in pi too.
pub const PACKAGE: &str = "pi-claude-bridge";

/// The tree shipped with harlo: `HARLO_PI_CLAUDE_BRIDGE` (empty: none, for
/// tests), else `lib/harlo/pi-claude-bridge` beside harlo's `bin`, else
/// `target/pi-claude-bridge` for a build from source.
#[must_use]
pub fn shipped() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("HARLO_PI_CLAUDE_BRIDGE") {
        return Some(PathBuf::from(dir)).filter(|d| !d.as_os_str().is_empty() && d.join("node_modules").join(PACKAGE).is_dir());
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [dir.join("../lib/harlo/pi-claude-bridge"), dir.join("../pi-claude-bridge"), dir.join("pi-claude-bridge")]
        .into_iter()
        .find(|d| d.join("node_modules").join(PACKAGE).is_dir())
}

/// Where prepared copies go: `$XDG_DATA_HOME/harlo/pi-claude-bridge`.
#[must_use]
pub fn data_root() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .map(|d| d.join("harlo").join("pi-claude-bridge"))
}

/// The extension ready to load (`-e`), preparing it first if needed
/// (blocking: the first time, a download of about 5 MB).
///
/// # Errors
///
/// When harlo wasn't shipped with it, or the Agent SDK can't be fetched,
/// doesn't match its pinned integrity or can't be unpacked.
pub fn prepare() -> Result<PathBuf, String> {
    let shipped = shipped().ok_or("harlo was built without it.")?;
    prepare_in(&shipped, &data_root().ok_or("no home folder")?)
}

/// [`prepare`] from `shipped` into `root`.
///
/// A shipped tree without `sdk.json` is complete and loads from where it is.
///
/// # Errors
///
/// As [`prepare`].
pub fn prepare_in(shipped: &Path, root: &Path) -> Result<PathBuf, String> {
    let Ok(text) = std::fs::read_to_string(shipped.join("sdk.json")) else {
        return Ok(shipped.join("node_modules").join(PACKAGE));
    };
    let sdk: Value = serde_json::from_str(&text).map_err(|e| format!("sdk.json: {e}"))?;
    let (version, url, integrity) = (s(&sdk, "version"), s(&sdk, "url"), s(&sdk, "integrity"));
    let bridge: Value = std::fs::read_to_string(shipped.join("node_modules").join(PACKAGE).join("package.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let dest = root.join(format!("{}-sdk-{version}", s(&bridge, "version")));
    let ready = dest.join("node_modules").join(PACKAGE);
    if dest.join(".ready").is_file() {
        return Ok(ready);
    }
    let part = root.join(format!(".part-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&part);
    std::fs::create_dir_all(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    let done = (|| {
        let copied = Command::new("cp").arg("-a").arg(shipped.join("node_modules")).arg(&part).status().map_err(|e| format!("cp: {e}"))?;
        if !copied.success() {
            return Err("copying it failed".to_owned());
        }
        unpack(&format!("Anthropic's Agent SDK {version}"), url, integrity, &part, &part.join("node_modules/@anthropic-ai/claude-agent-sdk"))?;
        std::fs::write(part.join(".ready"), version).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::rename(&part, &dest).map_err(|e| e.to_string())
    })();
    if done.is_err() {
        let _ = std::fs::remove_dir_all(&part);
    }
    done.map(|()| ready)
}

/// Downloads npm's package of `what`, checks it and unpacks it into `into`
/// (its `package/` folder's contents), through `part`.
fn unpack(what: &str, url: &str, integrity: &str, part: &Path, into: &Path) -> Result<(), String> {
    let bytes = fetch(url).map_err(|e| format!("Downloading {what} failed: {e}"))?;
    if !integrity_matches(&bytes, integrity) {
        return Err(format!("{what} didn't match its pinned checksum."));
    }
    let tgz = part.join("package.tgz");
    std::fs::write(&tgz, &bytes).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(into).map_err(|e| e.to_string())?;
    let unpacked = Command::new("tar").arg("-xzf").arg(&tgz).arg("--strip-components=1").arg("-C").arg(into).status().map_err(|e| format!("tar: {e}"))?;
    let _ = std::fs::remove_file(&tgz);
    if unpacked.success() { Ok(()) } else { Err(format!("unpacking {what} failed")) }
}

/// This computer's platform as the SDK names its Claude Code packages
/// (`linux-x64`, `linux-arm64-musl`…).
#[must_use]
pub fn platform() -> Option<String> {
    let (arch, lib) = match std::env::consts::ARCH {
        "x86_64" => ("x64", "x86_64"),
        "aarch64" => ("arm64", "aarch64"),
        _ => return None,
    };
    let musl = Path::new(&format!("/lib/ld-musl-{lib}.so.1")).exists();
    Some(format!("linux-{arch}{}", if musl { "-musl" } else { "" }))
}

/// The SDK's copy of Claude Code, once downloaded ([`prepare_claude`]).
#[must_use]
pub fn downloaded_claude() -> Option<PathBuf> {
    let shipped = shipped()?;
    let sdk: Value = serde_json::from_str(&std::fs::read_to_string(shipped.join("sdk.json")).ok()?).ok()?;
    Some(data_root()?.join(format!("claude-code-{}", s(&sdk, "version"))).join("claude")).filter(|p| p.is_file())
}

/// Downloads the SDK's copy of Claude Code (for someone without their own),
/// once: its program.
///
/// # Errors
///
/// When harlo wasn't shipped with the extension, has no copy for this
/// computer, or the download fails or doesn't match its pinned integrity.
pub fn prepare_claude() -> Result<PathBuf, String> {
    let shipped = shipped().ok_or("harlo was built without it.")?;
    let platform = platform().ok_or("Claude Code has no copy for this computer.")?;
    prepare_claude_in(&shipped, &data_root().ok_or("no home folder")?, &platform)
}

/// [`prepare_claude`] from `shipped` into `root`, for `platform`.
///
/// # Errors
///
/// As [`prepare_claude`].
pub fn prepare_claude_in(shipped: &Path, root: &Path, platform: &str) -> Result<PathBuf, String> {
    let text = std::fs::read_to_string(shipped.join("sdk.json")).map_err(|e| format!("harlo was built without Claude Code's address (sdk.json: {e})."))?;
    let sdk: Value = serde_json::from_str(&text).map_err(|e| format!("sdk.json: {e}"))?;
    let cli = &sdk["cli"][platform];
    if !cli.is_object() {
        return Err(format!("Claude Code has no copy for {platform}."));
    }
    let dest = root.join(format!("claude-code-{}", s(&sdk, "version")));
    let program = dest.join("claude");
    if program.is_file() {
        return Ok(program);
    }
    let part = root.join(format!(".part-claude-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&part);
    std::fs::create_dir_all(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    let done = unpack("Claude Code", s(cli, "url"), s(cli, "integrity"), &part, &part.join("cli")).and_then(|()| {
        if !part.join("cli/claude").is_file() {
            return Err("Claude Code's package had no program in it.".to_owned());
        }
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::rename(part.join("cli"), &dest).map_err(|e| e.to_string())
    });
    let _ = std::fs::remove_dir_all(&part);
    done.map(|()| program)
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or_default()
}

/// The SDK's package (a `file://` address reads a local file, for tests).
fn fetch(url: &str) -> Result<Vec<u8>, String> {
    if let Some(path) = url.strip_prefix("file://") {
        return std::fs::read(path).map_err(|e| format!("{path}: {e}"));
    }
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_mins(15))).tls_config(tls).build().into();
    let mut resp = agent.get(url).call().map_err(|e| e.to_string())?;
    resp.body_mut().with_config().limit(1024 * 1024 * 1024).read_to_vec().map_err(|e| e.to_string())
}

/// Whether `bytes` match npm's `sha512-<base64>` integrity.
#[must_use]
pub fn integrity_matches(bytes: &[u8], integrity: &str) -> bool {
    use base64::Engine;
    use sha2::{Digest, Sha512};
    integrity.strip_prefix("sha512-").is_some_and(|want| base64::engine::general_purpose::STANDARD.encode(Sha512::digest(bytes)) == want)
}

/// Whether pi has the extension installed itself (in `packages` of its
/// settings): then pi loads its own copy and harlo's would clash.
#[must_use]
pub fn installed_in_pi(settings: &Value) -> bool {
    settings["packages"].as_array().is_some_and(|list| {
        list.iter().any(|p| {
            let source = p.as_str().or_else(|| p["source"].as_str()).unwrap_or_default();
            source.trim_start_matches("npm:").split('@').next() == Some(PACKAGE) || source.trim_end_matches('/').ends_with(&format!("/{PACKAGE}"))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("harlo-pi-bridge-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn checks_the_pinned_integrity() {
        // (sha512 of "abc".)
        let abc = "sha512-3a81oZNherrMQXNJriBBMRLm+k6JqX6iCp7u5ktV05ohkpkqJ0/BqDa6PCOj/uu9RU1EI2Q86A4qmslPpUyknw==";
        assert!(integrity_matches(b"abc", abc));
        assert!(!integrity_matches(b"abd", abc));
        assert!(!integrity_matches(b"abc", "sha1-x"));
    }

    #[test]
    fn prepares_once_with_the_sdk_fetched() {
        let d = scratch("prepare");
        let shipped = d.join("shipped");
        std::fs::create_dir_all(shipped.join("node_modules/pi-claude-bridge")).unwrap();
        std::fs::write(shipped.join("node_modules/pi-claude-bridge/package.json"), r#"{"version":"1.2.3"}"#).unwrap();
        // (The SDK's package: a tarball with npm's package/ folder.)
        std::fs::create_dir_all(d.join("pkg/package")).unwrap();
        std::fs::write(d.join("pkg/package/sdk.mjs"), "export {}").unwrap();
        assert!(Command::new("tar").arg("-czf").arg(d.join("sdk.tgz")).arg("-C").arg(d.join("pkg")).arg("package").status().unwrap().success());
        let bytes = std::fs::read(d.join("sdk.tgz")).unwrap();
        let integrity = {
            use base64::Engine;
            use sha2::{Digest, Sha512};
            format!("sha512-{}", base64::engine::general_purpose::STANDARD.encode(Sha512::digest(&bytes)))
        };
        let sdk = json!({ "version": "9.9.9", "url": format!("file://{}", d.join("sdk.tgz").display()), "integrity": integrity });
        std::fs::write(shipped.join("sdk.json"), sdk.to_string()).unwrap();
        let root = d.join("data");
        let ext = prepare_in(&shipped, &root).unwrap();
        assert_eq!(ext, root.join("1.2.3-sdk-9.9.9/node_modules/pi-claude-bridge"));
        assert!(root.join("1.2.3-sdk-9.9.9/node_modules/@anthropic-ai/claude-agent-sdk/sdk.mjs").is_file(), "the SDK unpacked beside it");
        // (Ready: not fetched again, even with the package gone.)
        std::fs::remove_file(d.join("sdk.tgz")).unwrap();
        assert_eq!(prepare_in(&shipped, &root).unwrap(), ext);

        // A wrong checksum: nothing left half done.
        let bad = json!({ "version": "9.9.8", "url": format!("file://{}", shipped.join("sdk.json").display()), "integrity": "sha512-AAAA" });
        std::fs::write(shipped.join("sdk.json"), bad.to_string()).unwrap();
        assert!(prepare_in(&shipped, &root).unwrap_err().contains("checksum"));
        assert!(!root.join("1.2.3-sdk-9.9.8").exists());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1, "no part left");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn downloads_claude_codes_copy_for_this_platform() {
        let d = scratch("claude");
        std::fs::create_dir_all(d.join("pkg/package")).unwrap();
        std::fs::write(d.join("pkg/package/claude"), "#!/bin/sh\n").unwrap();
        assert!(Command::new("tar").arg("-czf").arg(d.join("cli.tgz")).arg("-C").arg(d.join("pkg")).arg("package").status().unwrap().success());
        let integrity = {
            use base64::Engine;
            use sha2::{Digest, Sha512};
            format!("sha512-{}", base64::engine::general_purpose::STANDARD.encode(Sha512::digest(std::fs::read(d.join("cli.tgz")).unwrap())))
        };
        let cli = json!({ "url": format!("file://{}", d.join("cli.tgz").display()), "integrity": integrity });
        std::fs::write(d.join("sdk.json"), json!({ "version": "9.9.9", "cli": { "linux-x64": cli } }).to_string()).unwrap();
        let root = d.join("data");
        assert_eq!(prepare_claude_in(&d, &root, "linux-x64").unwrap(), root.join("claude-code-9.9.9/claude"));
        assert!(prepare_claude_in(&d, &root, "linux-arm64").unwrap_err().contains("no copy"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_complete_tree_loads_from_where_it_is() {
        let d = scratch("complete");
        std::fs::create_dir_all(d.join("node_modules/pi-claude-bridge")).unwrap();
        assert_eq!(prepare_in(&d, &d.join("data")).unwrap(), d.join("node_modules/pi-claude-bridge"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn sees_pis_own_copy() {
        assert!(installed_in_pi(&json!({ "packages": ["npm:pi-claude-bridge"] })));
        assert!(installed_in_pi(&json!({ "packages": [{ "source": "npm:pi-claude-bridge@0.13.0", "extensions": [] }] })));
        assert!(installed_in_pi(&json!({ "packages": ["/home/me/src/pi-claude-bridge/"] })));
        assert!(!installed_in_pi(&json!({ "packages": ["npm:pi-claude-bridge-extras", "npm:pi-lens"] })));
        assert!(!installed_in_pi(&json!({})));
    }
}
