//! Lists the bundled OpenCode themes, so a version bump that adds or drops
//! one needs no code change.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let folder = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets it"))
        .join("themes")
        .join("opencode");
    println!("cargo::rerun-if-changed={}", folder.display());
    let mut files: Vec<PathBuf> = fs::read_dir(&folder)
        .expect("the bundled themes")
        .map(|entry| entry.expect("a theme file").path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    let mut out = String::from("&[\n");
    for file in files {
        writeln!(out, "    include_str!({:?}),", file.display().to_string()).expect("a string");
    }
    out.push(']');
    let target =
        PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets it")).join("opencode_themes.rs");
    fs::write(target, out).expect("the theme list");
}
