//! Safe wrappers over the C library calls the agent makes.
//!
//! Signalling process groups and reading user ids go through libc, which
//! makes each call `unsafe` to write even though none of these touches
//! memory the caller owns. This module holds those calls, each with the
//! reason it is sound, so the rest of the crate needs no `unsafe` for them.

// Rust guideline compliant 2026-10-09

/// Sends `signal` to every process in the process group `pgid`.
///
/// Returns whether the signal was delivered. A `pgid` of 0 or below is
/// refused and returns `false`: `killpg` would read 0 as the agent's own
/// group, and a negative group is never one the agent started.
pub fn signal_group(pgid: i32, signal: libc::c_int) -> bool {
    if pgid <= 0 {
        return false;
    }
    // SAFETY: killpg(2) takes two integers and reads or writes no memory of
    // this process; any values are sound (an unknown group or signal only
    // makes it fail with ESRCH or EINVAL).
    unsafe { libc::killpg(pgid, signal) == 0 }
}

/// Whether the process group `pgid` still has a process in it.
pub fn group_alive(pgid: i32) -> bool {
    signal_group(pgid, 0)
}

/// The real user id of this process.
pub fn uid() -> u32 {
    // SAFETY: getuid(2) takes no arguments, always succeeds and touches no
    // memory of this process.
    unsafe { libc::getuid() }
}

/// The effective user id of this process (0 when it runs as root).
pub fn euid() -> u32 {
    // SAFETY: geteuid(2) takes no arguments, always succeeds and touches no
    // memory of this process.
    unsafe { libc::geteuid() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt as _;

    #[test]
    fn signals_reach_only_real_groups() {
        // Not a group the agent could have started: refused, nothing sent.
        assert!(!signal_group(0, 0));
        assert!(!signal_group(-1, 0));
        // A group that ends is seen as gone once it has been reaped.
        let mut child = std::process::Command::new("sleep").arg("30").process_group(0).spawn().unwrap();
        let pgid = i32::try_from(child.id()).unwrap();
        assert!(group_alive(pgid));
        assert!(signal_group(pgid, libc::SIGKILL));
        child.wait().unwrap();
        assert!(!group_alive(pgid));
    }

    #[test]
    fn user_ids_match_the_process_owner() {
        let owner = std::fs::metadata("/proc/self").map(|m| std::os::unix::fs::MetadataExt::uid(&m)).unwrap();
        assert_eq!(uid(), owner);
        assert_eq!(euid(), owner);
    }
}
