//! The commit harlo-agent is built from, for `--version` (and the
//! benchmark's records): `+changes` when its sources had uncommitted
//! changes then.

use std::process::Command;

fn main() {
    let git = |args: &[&str]| Command::new("git").args(args).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    let commit = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = git(&["status", "--porcelain", "--", "."]).is_some_and(|s| !s.is_empty());
    println!("cargo:rustc-env=HARLO_AGENT_COMMIT={commit}{}", if dirty { "+changes" } else { "" });
    for p in ["src", "Cargo.toml", "../../.git/HEAD", "../../.git/index"] {
        println!("cargo:rerun-if-changed={p}");
    }
}
