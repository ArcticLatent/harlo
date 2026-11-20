//! harlo's programs on another machine, for pi's sessions there.
//!
//! harlo puts its helper there (pi's sandbox, the check for commands that
//! can't be undone, and the wiki: a static build, so it runs whatever C
//! library the machine has; once per build of it). The code index goes
//! beside it on the first use of the wiki there.

// Rust guideline compliant 2026-10-09

use anyhow::{Context, Result};

use crate::remote::{self, HostInfo};
use crate::remote_helper::{ARCHES, Helper};

/// harlo's helper: pi's sandbox there, and the wiki there.
static HELPER: Helper = Helper::new("harlo-helper", true, "helper", "--version");

/// harlo's code index, which the wiki there finds beside the helper (its
/// checks read declarations with it).
static INDEX: Helper = Helper::new("harlo-index", true, "code index", "version");

/// harlo's helper on `host_name`, put there first if this build of it isn't.
///
/// Its path there (blocking).
///
/// # Errors
///
/// When the machine can't be reached, there's no build for its kind, or it
/// can't be written there.
pub fn helper_on(host_name: &str) -> Result<String> {
    let host = remote::host(host_name).with_context(|| format!("unknown machine {host_name}"))?;
    HELPER.on(&host)
}

/// For harlo-harness: harlo's helper on `host_name` for a pi session there
/// (its sandbox), put there first if needed.
///
/// # Errors
///
/// Why it can't be put on that machine.
pub fn remote_helper(host_name: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    helper_on(host_name).map_err(|e| format!("harlo's sandbox can't start on {host_name}: {e:#}").into())
}

/// harlo's code index on `host_name`, put there first if needed.
///
/// Its path there (blocking). The wiki there finds it beside the helper;
/// without it, it reads declarations with its own scanner.
///
/// # Errors
///
/// When the machine can't be reached, there's no build for its kind, or it
/// can't be written there.
pub fn index_on(host_name: &str) -> Result<String> {
    let host = remote::host(host_name).with_context(|| format!("unknown machine {host_name}"))?;
    INDEX.on(&host)
}

/// Why harlo's helper can't run on a machine its check found (None: it
/// can).
pub fn not_there(info: &HostInfo) -> Option<String> {
    if info.os != "Linux" {
        return Some(format!("harlo's helper runs on Linux machines, not {} ones, so far.", if info.os.is_empty() { "these" } else { &info.os }));
    }
    if !ARCHES.contains(&info.arch.as_str()) {
        return Some(format!("harlo's helper runs on x86_64 and ARM (aarch64) machines, not {} ones.", info.arch));
    }
    HELPER.local(&info.arch).is_none().then(|| format!("This harlo has no copy of its helper for {} machines (harlo's releases have one).", info.arch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_machines_get_the_helper() {
        let info = |os: &str, arch: &str| HostInfo { os: os.into(), arch: arch.into(), ..Default::default() };
        assert!(not_there(&info("Darwin", "arm64")).unwrap().contains("Linux"));
        assert!(not_there(&info("Linux", "armv7l")).unwrap().contains("armv7l"));
        // (Whether this build has the copy depends on what's been built.)
        let here = not_there(&info("Linux", std::env::consts::ARCH));
        assert_eq!(here.is_none(), HELPER.local(std::env::consts::ARCH).is_some());
    }
}
