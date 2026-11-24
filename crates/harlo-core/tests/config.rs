//! Setting the model new sessions use, whether undo keeps snapshots and
//! whether automations are allowed, in the server's global config file, each
//! test in its own scratch folder.

// Rust guideline compliant 2026-10-09

use std::fs;

use harlo_core::config;

#[test]
fn a_missing_file_is_created_with_the_model() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let folder = scratch.path().join("harlo");
    assert_eq!(config::model(&folder).expect("no file yet"), None);

    config::set_model(&folder, Some("anthropic/opus")).expect("set");
    let text = fs::read_to_string(folder.join("harlo.json")).expect("written");
    let json: serde_json::Value = serde_json::from_str(&text).expect("plain JSON");
    assert_eq!(json["model"], "anthropic/opus");
    assert!(json.get("$schema").is_none(), "{text}");
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
    fs::write(folder.join("harlo.json"), before).expect("a file");

    config::set_model(folder, Some("b/two")).expect("set");
    let after = fs::read_to_string(folder.join("harlo.json")).expect("read");
    assert_eq!(after, before.replace("a/one", "b/two"));

    // Taking it away leaves the rest.
    config::set_model(folder, None).expect("removed");
    let after = fs::read_to_string(folder.join("harlo.json")).expect("read");
    assert!(!after.contains("model"), "{after}");
    assert!(after.contains("// dark") && after.contains("\"theme\": \"dracula\""));
    assert_eq!(config::model(folder).expect("read"), None);
    assert!(
        !folder.join(".harlo.json.pending").exists(),
        "nothing left beside it"
    );
}

#[test]
fn the_jsonc_file_is_used_when_there_is_one() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let folder = scratch.path();
    fs::write(folder.join("harlo.json"), "{\"model\": \"a/json\"}").expect("json");
    fs::write(
        folder.join("harlo.jsonc"),
        "{\n  // wins\n  \"model\": \"a/jsonc\"\n}",
    )
    .expect("jsonc");
    assert_eq!(
        config::model(folder).expect("read").as_deref(),
        Some("a/jsonc")
    );

    config::set_model(folder, Some("b/new")).expect("set");
    let jsonc = fs::read_to_string(folder.join("harlo.jsonc")).expect("read");
    assert!(
        jsonc.contains("// wins") && jsonc.contains("b/new"),
        "{jsonc}"
    );
    let json = fs::read_to_string(folder.join("harlo.json")).expect("read");
    assert!(json.contains("a/json"), "the other file stays");
}

#[test]
fn an_empty_file_gets_the_model_added() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let folder = scratch.path();
    fs::write(folder.join("harlo.json"), "{}").expect("empty");
    config::set_model(folder, Some("a/b")).expect("set");
    assert_eq!(config::model(folder).expect("read").as_deref(), Some("a/b"));
}

#[test]
fn a_broken_file_is_left_alone() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let folder = scratch.path();
    let broken = "{ \"model\": ";
    fs::write(folder.join("harlo.json"), broken).expect("broken");
    let error = config::set_model(folder, Some("a/b")).expect_err("can't edit it");
    assert!(error.to_string().contains("isn't valid JSON"), "{error}");
    assert_eq!(
        fs::read_to_string(folder.join("harlo.json")).expect("read"),
        broken
    );

    fs::write(folder.join("harlo.json"), "[1, 2]").expect("a list");
    let error = config::set_model(folder, Some("a/b")).expect_err("not an object");
    assert!(error.to_string().contains("settings object"), "{error}");
}

#[test]
fn snapshots_are_on_unless_turned_off() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let folder = scratch.path();
    assert!(config::snapshots(folder).expect("no file yet"));
    let before = "{\n  // mine\n  \"model\": \"a/one\"\n}\n";
    fs::write(folder.join("harlo.json"), before).expect("a file");
    assert!(config::snapshots(folder).expect("not set"));

    config::set_snapshots(folder, false).expect("off");
    let text = fs::read_to_string(folder.join("harlo.json")).expect("read");
    assert!(
        text.contains("\"snapshots\": false") && text.contains("// mine"),
        "{text}"
    );
    assert!(!config::snapshots(folder).expect("read"));

    // On again takes the setting away: on is the server's default.
    config::set_snapshots(folder, true).expect("on");
    let text = fs::read_to_string(folder.join("harlo.json")).expect("read");
    assert!(!text.contains("snapshots"), "{text}");
    assert!(text.contains("\"model\": \"a/one\""));
    assert!(config::snapshots(folder).expect("read"));
}

#[test]
fn automations_are_on_until_turned_off_at_the_top() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let folder = scratch.path();
    assert!(config::automations(folder).expect("no file yet"));
    let before = "{\n  // mine\n  \"model\": \"a/one\"\n}\n";
    fs::write(folder.join("harlo.json"), before).expect("a file");
    assert!(config::automations(folder).expect("not set"));

    config::set_automations(folder, false).expect("off");
    let text = fs::read_to_string(folder.join("harlo.json")).expect("read");
    assert!(text.contains("// mine"), "{text}");
    let json: serde_json::Value = serde_json::from_str(&text.replace("// mine", "")).expect("JSON");
    assert_eq!(json["automations"], false);
    assert!(!config::automations(folder).expect("read"));

    // On takes the setting away again.
    config::set_automations(folder, true).expect("on");
    let text = fs::read_to_string(folder.join("harlo.json")).expect("read");
    assert!(!text.contains("automations"), "{text}");
    assert!(text.contains("\"model\": \"a/one\""), "{text}");
    assert!(config::automations(folder).expect("read"));

    // The old experimental switch goes; other experimental switches stay.
    let other = "{\n  \"experimental\": { \"todo\": true, \"automations\": true }\n}\n";
    fs::write(folder.join("harlo.json"), other).expect("a file");
    config::set_automations(folder, true).expect("on");
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(folder.join("harlo.json")).expect("read"))
            .expect("JSON");
    assert_eq!(json["experimental"], serde_json::json!({ "todo": true }));
    assert!(json.get("automations").is_none());
}

#[test]
fn mcp_servers_are_added_turned_off_and_removed_under_mcp_servers() {
    use harlo_core::config::{McpEntry, McpRun};
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let folder = scratch.path();
    let before = "{\n  // mine\n  \"theme\": \"dracula\"\n}\n";
    fs::write(folder.join("harlo.json"), before).expect("a file");
    assert!(config::mcp_servers(folder).expect("read").is_empty());

    let remote = McpEntry {
        name: "docs".into(),
        run: McpRun::Remote {
            url: "https://mcp.example.invalid/mcp".into(),
            headers: vec![("Authorization".into(), "Bearer x".into())],
        },
        disabled: false,
        codemode: true,
    };
    let local = McpEntry {
        name: "browser".into(),
        run: McpRun::Local {
            command: vec!["npx".into(), "-y".into(), "some-mcp@1".into()],
            environment: vec![("TOKEN".into(), "t".into())],
        },
        disabled: false,
        codemode: true,
    };
    config::set_mcp_server(folder, &remote).expect("add");
    config::set_mcp_server(folder, &local).expect("add");
    let text = fs::read_to_string(folder.join("harlo.json")).expect("read");
    assert!(text.contains("// mine"), "comments stay: {text}");
    let json: serde_json::Value = serde_json::from_str(&text.replace("// mine", "")).expect("JSON");
    assert_eq!(json["mcp"]["servers"]["docs"]["type"], "remote");
    assert_eq!(
        json["mcp"]["servers"]["browser"]["command"][2],
        "some-mcp@1"
    );
    assert_eq!(json["theme"], "dracula");
    assert_eq!(
        config::mcp_servers(folder).expect("read"),
        vec![remote.clone(), local.clone()]
    );

    config::set_mcp_disabled(folder, "docs", true).expect("off");
    let read = config::mcp_servers(folder).expect("read");
    assert!(read[0].disabled);
    config::set_mcp_disabled(folder, "docs", false).expect("on");
    assert!(!config::mcp_servers(folder).expect("read")[0].disabled);
    assert!(
        !fs::read_to_string(folder.join("harlo.json"))
            .expect("read")
            .contains("disabled")
    );
    config::set_mcp_disabled(folder, "missing", true).expect("left alone");

    // Code Mode is the server's default; off lists the tools one by one.
    config::set_mcp_codemode(folder, "browser", false).expect("listed");
    let json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(folder.join("harlo.json"))
            .expect("read")
            .replace("// mine", ""),
    )
    .expect("JSON");
    assert_eq!(json["mcp"]["servers"]["browser"]["codemode"], false);
    assert!(!config::mcp_servers(folder).expect("read")[1].codemode);
    config::set_mcp_codemode(folder, "browser", true).expect("scripts");
    assert!(config::mcp_servers(folder).expect("read")[1].codemode);
    assert!(
        !fs::read_to_string(folder.join("harlo.json"))
            .expect("read")
            .contains("codemode")
    );
    assert_eq!(config::mcp_servers(folder).expect("read").len(), 2);

    config::remove_mcp_server(folder, "docs").expect("removed");
    config::remove_mcp_server(folder, "browser").expect("removed");
    let after = fs::read_to_string(folder.join("harlo.json")).expect("read");
    assert!(!after.contains("mcp"), "nothing left: {after}");
    assert!(after.contains("dracula"));
}

#[test]
fn the_catalog_entries_are_valid_servers() {
    for offered in harlo_core::mcp_catalog::OFFERED {
        let entry = offered.entry();
        assert_eq!(entry.name, offered.name);
        assert!(!entry.disabled);
        match entry.run {
            harlo_core::config::McpRun::Remote { url, .. } => {
                assert!(url.starts_with("https://"), "{url}");
            }
            harlo_core::config::McpRun::Local { command, .. } => {
                assert!(!command.is_empty());
            }
        }
    }
}
