//! Setting the model new sessions use, and whether undo keeps snapshots,
//! in OpenCode's global config file, each test in its own scratch folder.

// Rust guideline compliant 2026-10-09

use std::fs;

use harlo_core::config;

#[test]
fn a_missing_file_is_created_with_the_model() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let folder = scratch.path().join("opencode");
    assert_eq!(config::model(&folder).expect("no file yet"), None);

    config::set_model(&folder, Some("anthropic/opus")).expect("set");
    let text = fs::read_to_string(folder.join("opencode.json")).expect("written");
    let json: serde_json::Value = serde_json::from_str(&text).expect("plain JSON");
    assert_eq!(json["model"], "anthropic/opus");
    assert_eq!(json["$schema"], "https://opencode.ai/config.json");
    assert_eq!(
        config::model(&folder).expect("read").as_deref(),
        Some("anthropic/opus")
    );
}

#[test]
fn a_change_keeps_comments_and_the_other_settings() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let folder = scratch.path();
    let before =
        "{\n  // my default\n  \"model\": \"a/one\",\n  \"theme\": \"dracula\", // dark\n}\n";
    fs::write(folder.join("opencode.json"), before).expect("a file");

    config::set_model(folder, Some("b/two")).expect("set");
    let after = fs::read_to_string(folder.join("opencode.json")).expect("read");
    assert_eq!(after, before.replace("a/one", "b/two"));

    // Taking it away leaves the rest.
    config::set_model(folder, None).expect("removed");
    let after = fs::read_to_string(folder.join("opencode.json")).expect("read");
    assert!(!after.contains("model"), "{after}");
    assert!(after.contains("// dark") && after.contains("\"theme\": \"dracula\""));
    assert_eq!(config::model(folder).expect("read"), None);
    assert!(
        !folder.join(".opencode.json.harlo").exists(),
        "nothing left beside it"
    );
}

#[test]
fn the_jsonc_file_is_used_when_there_is_one() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let folder = scratch.path();
    fs::write(folder.join("opencode.json"), "{\"model\": \"a/json\"}").expect("json");
    fs::write(
        folder.join("opencode.jsonc"),
        "{\n  // wins\n  \"model\": \"a/jsonc\"\n}",
    )
    .expect("jsonc");
    assert_eq!(
        config::model(folder).expect("read").as_deref(),
        Some("a/jsonc")
    );

    config::set_model(folder, Some("b/new")).expect("set");
    let jsonc = fs::read_to_string(folder.join("opencode.jsonc")).expect("read");
    assert!(
        jsonc.contains("// wins") && jsonc.contains("b/new"),
        "{jsonc}"
    );
    let json = fs::read_to_string(folder.join("opencode.json")).expect("read");
    assert!(json.contains("a/json"), "the other file stays");
}

#[test]
fn an_empty_file_gets_the_model_added() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let folder = scratch.path();
    fs::write(folder.join("opencode.json"), "{}").expect("empty");
    config::set_model(folder, Some("a/b")).expect("set");
    assert_eq!(config::model(folder).expect("read").as_deref(), Some("a/b"));
}

#[test]
fn a_broken_file_is_left_alone() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let folder = scratch.path();
    let broken = "{ \"model\": ";
    fs::write(folder.join("opencode.json"), broken).expect("broken");
    let error = config::set_model(folder, Some("a/b")).expect_err("can't edit it");
    assert!(error.to_string().contains("isn't valid JSON"), "{error}");
    assert_eq!(
        fs::read_to_string(folder.join("opencode.json")).expect("read"),
        broken
    );

    fs::write(folder.join("opencode.json"), "[1, 2]").expect("a list");
    let error = config::set_model(folder, Some("a/b")).expect_err("not an object");
    assert!(error.to_string().contains("settings object"), "{error}");
}

#[test]
fn snapshots_are_on_unless_turned_off() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let folder = scratch.path();
    assert!(config::snapshots(folder).expect("no file yet"));
    let before = "{\n  // mine\n  \"model\": \"a/one\"\n}\n";
    fs::write(folder.join("opencode.json"), before).expect("a file");
    assert!(config::snapshots(folder).expect("not set"));

    config::set_snapshots(folder, false).expect("off");
    let text = fs::read_to_string(folder.join("opencode.json")).expect("read");
    assert!(
        text.contains("\"snapshots\": false") && text.contains("// mine"),
        "{text}"
    );
    assert!(!config::snapshots(folder).expect("read"));

    // On again takes the setting away: on is OpenCode's default.
    config::set_snapshots(folder, true).expect("on");
    let text = fs::read_to_string(folder.join("opencode.json")).expect("read");
    assert!(!text.contains("snapshots"), "{text}");
    assert!(text.contains("\"model\": \"a/one\""));
    assert!(config::snapshots(folder).expect("read"));
}
