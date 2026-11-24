//! The phone palettes of every bundled theme, kept beside the iPhone app's
//! tests so they check the app reads each one well.
//!
//! Run with `HARLO_WRITE_PHONE_THEMES=1` to write them again after the
//! themes or the palette change; otherwise this checks they're current.

// Rust guideline compliant 2026-10-09

use std::collections::BTreeMap;
use std::path::PathBuf;

use harlo_core::phone::theme::palette;
use harlo_core::theme::{Look, Mode, bundled};
use serde_json::json;

/// Where the iPhone app's tests read them.
fn folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ios/HarloKit/Tests/HarloKitTests/Themes")
}

#[test]
fn the_phone_palettes_are_current() {
    let mut made = BTreeMap::new();
    for scheme in bundled() {
        let light = scheme.mode == Mode::Light;
        let text = serde_json::to_string_pretty(&json!({
            "name": scheme.name,
            "light": light,
            "colors": palette(&scheme.tokens_with(Look::default())),
        }))
        .expect("JSON");
        made.insert(format!("{}.json", scheme.id), text + "\n");
    }
    assert!(made.len() > 20, "{} themes", made.len());
    let dir = folder();
    if std::env::var_os("HARLO_WRITE_PHONE_THEMES").is_some() {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        for (name, text) in &made {
            std::fs::write(dir.join(name), text).expect("write");
        }
        return;
    }
    let mut kept: Vec<String> = std::fs::read_dir(&dir)
        .expect("the palettes; write them with HARLO_WRITE_PHONE_THEMES=1")
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    kept.sort_unstable();
    assert_eq!(
        kept,
        made.keys().cloned().collect::<Vec<_>>(),
        "write them again with HARLO_WRITE_PHONE_THEMES=1"
    );
    for (name, text) in &made {
        let on_disk = std::fs::read_to_string(dir.join(name)).expect("read");
        assert_eq!(
            &on_disk, text,
            "{name}: write them again with HARLO_WRITE_PHONE_THEMES=1"
        );
    }
}
