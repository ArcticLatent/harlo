//! Timeline rows built from a session's messages.

// Rust guideline compliant 2026-10-09

use std::time::Duration;

use harlo_core::timeline::{Entry, entries, tool_detail, tool_label};
use harlo_core::workspace::{Message, Part, ToolCall, ToolStatus};
use serde_json::json;

fn call(name: &str, input: serde_json::Value, took_ms: Option<f64>) -> Part {
    Part::Tool(ToolCall {
        id: format!("call_{name}"),
        name: name.into(),
        status: ToolStatus::Completed,
        input,
        took_ms,
    })
}

#[test]
fn messages_become_rows_in_order() {
    let messages = vec![
        Message::User {
            id: "msg_1".into(),
            text: "Fix the port".into(),
        },
        Message::Assistant {
            id: "msg_2".into(),
            parts: vec![
                Part::Reasoning("**Updating README connectivity notes**\n\nThen the rest.".into()),
                call(
                    "edit",
                    json!({ "filePath": "SPEC.md", "oldString": "a", "newString": "b" }),
                    Some(15_000.0),
                ),
                Part::Text("   ".into()),
                call(
                    "bash",
                    json!({ "command": "python3 - <<'PY'\nimport io" }),
                    None,
                ),
                Part::Text("Changed the **port**.".into()),
            ],
            files: Vec::new(),
        },
        Message::Idle {
            id: "msg_3".into(),
            outcome: "succeeded".into(),
        },
        Message::Other {
            id: "msg_4".into(),
            kind: "compaction".into(),
        },
        Message::Idle {
            id: "msg_5".into(),
            outcome: "interrupted".into(),
        },
    ];
    let rows = entries(&messages);
    let keys: Vec<&str> = rows.iter().map(Entry::key).collect();
    assert_eq!(
        keys,
        ["msg_1", "msg_2/0", "msg_2/1", "msg_2/3", "msg_2/4", "msg_5"],
        "blank text, successful ends and unknown kinds are skipped"
    );
    assert_eq!(
        rows[1],
        Entry::Thought {
            key: "msg_2/0".into(),
            summary: "Updating README connectivity notes".into(),
        }
    );
    assert_eq!(
        rows[2],
        Entry::Tool {
            key: "msg_2/1".into(),
            label: "Edit".into(),
            detail: "SPEC.md".into(),
            status: ToolStatus::Completed,
            took: Some(Duration::from_secs(15)),
        }
    );
    assert!(
        matches!(&rows[3], Entry::Tool { label, detail, took: None, .. }
            if label == "Bash" && detail == "python3 - <<'PY'"),
        "{:?}",
        rows[3]
    );
    assert!(matches!(&rows[5], Entry::Stopped { outcome, .. } if outcome == "interrupted"));
}

#[test]
fn tools_have_readable_labels_and_details() {
    assert_eq!(tool_label("read"), "Read");
    assert_eq!(tool_label("todowrite"), "Todo");
    assert_eq!(tool_label("shell"), "Bash");
    assert_eq!(tool_label(""), "");

    assert_eq!(
        tool_detail("grep", &json!({ "pattern": "fn main" })),
        "fn main"
    );
    assert_eq!(
        tool_detail("read", &json!({ "filePath": "/a/b.rs" })),
        "/a/b.rs"
    );
    assert_eq!(
        tool_detail("todowrite", &json!({ "todos": [1, 2, 3] })),
        "3 items"
    );
    assert_eq!(
        tool_detail("mystery", &json!({ "n": 1, "what": "this" })),
        "this"
    );
    assert_eq!(tool_detail("mystery", &json!({})), "");
}

#[test]
fn long_details_are_shortened() {
    let long = "x".repeat(300);
    let rows = entries(&[Message::Assistant {
        id: "m".into(),
        parts: vec![call("bash", json!({ "command": long }), None)],
        files: Vec::new(),
    }]);
    let Entry::Tool { detail, .. } = &rows[0] else {
        panic!("a tool row: {rows:?}");
    };
    assert_eq!(detail.chars().count(), 121);
    assert!(detail.ends_with('…'));
}
