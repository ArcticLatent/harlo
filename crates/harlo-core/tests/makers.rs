//! The logos that stand for a session's model: every provider the server
//! names gets its maker's logo, and each logo exists in the window and in
//! the phone app.

// Rust guideline compliant 2026-10-09

use std::path::Path;

use harlo_core::makers::{LOGOS, logo, logo_number};

#[test]
fn providers_show_their_makers_logo() {
    assert_eq!(logo("anthropic"), Some("anthropic"));
    assert_eq!(logo("openai"), Some("openai"));
    // A Claude subscription's sessions, through the server's integration.
    assert_eq!(logo(harlo_core::claude::INTEGRATION), Some("claude-code"));
    // One maker under several provider IDs.
    assert_eq!(logo("zai"), Some("zai-coding-plan"));
    assert_eq!(logo("zai-coding-plan"), Some("zai-coding-plan"));
    assert_eq!(logo("opencode-go"), Some("opencode"));
    assert_eq!(logo("fireworks-ai"), Some("fireworks"));
    assert_eq!(logo("moonshotai"), Some("kimi-for-coding"));
    // Not a prefix of another name, and no logo for what Harlo doesn't know.
    assert_eq!(logo("openaix"), None);
    assert_eq!(logo("ollama"), None);
    assert_eq!(logo(""), None);

    assert_eq!(logo_number("anthropic"), 1);
    assert_eq!(logo_number("claude-agent"), 4);
    assert_eq!(logo_number("ollama"), 0);
}

#[test]
fn every_logo_is_in_the_window_and_in_the_phone_app() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sidebar = std::fs::read_to_string(root.join("crates/harlo/ui/sidebar.slint"))
        .expect("the sidebar's source");
    // The window numbers them in `LOGOS`' order.
    let mut from = 0;
    for name in LOGOS {
        let file = format!("images/providers/{name}.svg");
        assert!(
            root.join("crates/harlo/ui").join(&file).is_file(),
            "{file} is missing"
        );
        let at = sidebar[from..]
            .find(&file)
            .unwrap_or_else(|| panic!("the sidebar lists {file} out of order, or not at all"));
        from += at + file.len();
        let asset = format!("ios/Harlo/Shared/Shared.xcassets/Maker-{name}.imageset");
        assert!(root.join(&asset).is_dir(), "{asset} is missing");
    }
}
