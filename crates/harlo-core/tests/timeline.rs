//! Timeline rows built from a session's messages and its live events.

// Rust guideline compliant 2026-10-09

use std::time::Duration;

use harlo_core::server::Delivery;
use harlo_core::timeline::{
    Change, Compaction, Entry, Live, entries, summarize, tool_detail, tool_label, tools_summary,
};
use harlo_core::workspace::{EventKind, InboxItem, Message, Part, ToolCall, ToolStatus};
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
            attachments: vec!["a.txt".into()],
            skills: vec!["browser".into()],
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
            kind: "shell".into(),
        },
        Message::Compaction {
            id: "msg_5".into(),
            status: "completed".into(),
            summary: Some("## Objective".into()),
            error: None,
        },
        Message::Skill {
            id: "msg_6".into(),
            name: "Browser".into(),
        },
        Message::Idle {
            id: "msg_7".into(),
            outcome: "interrupted".into(),
        },
    ];
    let rows = entries(&messages);
    let keys: Vec<&str> = rows.iter().map(Entry::key).collect();
    assert_eq!(
        keys,
        [
            "msg_1",
            "msg_2/thought/0",
            "msg_2/tool/call_edit",
            "msg_2/tool/call_bash",
            "msg_2/text/1",
            "msg_5",
            "msg_6",
            "msg_7"
        ],
        "blank text, successful ends and unknown kinds are skipped; text counts per kind"
    );
    assert!(matches!(&rows[0], Entry::Prompt { attachments, skills, .. }
            if attachments == &["a.txt"] && skills == &["browser"]));
    assert_eq!(
        rows[2],
        Entry::Tool {
            key: "msg_2/tool/call_edit".into(),
            name: "edit".into(),
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
    assert!(
        matches!(&rows[5], Entry::Compaction { state: Compaction::Done(s), .. } if s == "## Objective")
    );
    assert!(
        matches!(&rows[6], Entry::Note { text, warning: false, .. } if text == "Loaded the Browser skill")
    );
    assert!(matches!(&rows[7], Entry::Stopped { outcome, .. } if outcome == "interrupted"));
}

#[test]
fn thoughts_show_their_first_line() {
    assert_eq!(
        summarize("**Updating README connectivity notes**\n\nThen the rest."),
        "Updating README connectivity notes"
    );
    assert_eq!(summarize("\n\n## Plan\nsteps"), "Plan");
    assert_eq!(summarize("**Planning** the `look`"), "Planning the look");
    assert_eq!(
        summarize("*Checking* snake_case names"),
        "Checking snake_case names"
    );
    assert_eq!(summarize(""), "");
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

#[test]
fn a_run_of_tools_is_summed_up() {
    assert_eq!(
        tools_summary(["edit", "edit", "bash", "write", "edit", "todowrite"]),
        "Edited 4 files, ran 1 command, updated the todos"
    );
    assert_eq!(
        tools_summary(["read", "grep", "glob", "webfetch", "mystery"]),
        "Read 1 file, searched 2 times, looked up 1 page, used 1 tool"
    );
    assert_eq!(tools_summary([]), "");
}

fn session(kind: impl FnOnce(String) -> EventKind) -> EventKind {
    kind("ses_1".into())
}

/// The streamed Glob call's row, as it stands at each step.
fn glob(status: ToolStatus, detail: &str, took_ms: Option<u64>) -> Change {
    Change::Put(Entry::Tool {
        key: "msg_a/tool/call_1".into(),
        name: "glob".into(),
        label: "Glob".into(),
        detail: detail.into(),
        status,
        took: took_ms.map(Duration::from_millis),
    })
}

/// A run's events, as they arrive: a delivered prompt, a thought, a tool call and an answer.
fn stream() -> Vec<(EventKind, f64)> {
    let mut events = Vec::new();
    let mut at = |kind: EventKind, time: f64| events.push((kind, time));
    at(
        session(|session_id| EventKind::InboxAdded {
            session_id,
            item: InboxItem {
                id: "msg_u".into(),
                kind: "user".into(),
                text: Some("Look around".into()),
                attachments: vec!["shot.png".into()],
                skills: Vec::new(),
                delivery: Delivery::Queue,
            },
        }),
        0.0,
    );
    at(
        session(|session_id| EventKind::InboxRemoved {
            session_id,
            item_id: "msg_u".into(),
            delivered: true,
        }),
        1.0,
    );
    for (delta, ordinal) in [("Check", 0), ("ing", 0)] {
        at(
            session(|session_id| EventKind::ReasoningDelta {
                session_id,
                message_id: "msg_a".into(),
                ordinal,
                delta: delta.into(),
            }),
            2.0,
        );
    }
    at(
        session(|session_id| EventKind::ToolStarted {
            session_id,
            message_id: "msg_a".into(),
            call_id: "call_1".into(),
            name: "glob".into(),
        }),
        1_000.0,
    );
    at(
        session(|session_id| EventKind::ToolCalled {
            session_id,
            message_id: "msg_a".into(),
            call_id: "call_1".into(),
            input: json!({ "pattern": "*.txt" }),
        }),
        1_100.0,
    );
    at(
        session(|session_id| EventKind::ToolEnded {
            session_id,
            message_id: "msg_a".into(),
            call_id: "call_1".into(),
            failed: false,
            error: None,
        }),
        3_500.0,
    );
    at(
        session(|session_id| EventKind::TextDelta {
            session_id,
            message_id: "msg_b".into(),
            ordinal: 0,
            delta: "Found it.".into(),
        }),
        3_600.0,
    );
    at(
        session(|session_id| EventKind::Done {
            session_id,
            outcome: "succeeded".into(),
        }),
        3_700.0,
    );

    events
}

#[test]
fn live_events_build_the_same_rows_as_the_history() {
    let mut live = Live::new();
    let changes: Vec<Change> = stream()
        .iter()
        .flat_map(|(kind, time)| live.apply(kind, *time))
        .collect();
    assert_eq!(
        changes,
        [
            Change::Put(Entry::Prompt {
                key: "msg_u".into(),
                text: "Look around".into(),
                attachments: vec!["shot.png".into()],
                skills: Vec::new(),
            }),
            Change::Extend {
                key: "msg_a/thought/0".into(),
                thought: true,
                delta: "Check".into(),
            },
            Change::Extend {
                key: "msg_a/thought/0".into(),
                thought: true,
                delta: "ing".into(),
            },
            glob(ToolStatus::Streaming, "", None),
            glob(ToolStatus::Running, "*.txt", None),
            glob(ToolStatus::Completed, "*.txt", Some(2_500)),
            Change::Extend {
                key: "msg_b/text/0".into(),
                thought: false,
                delta: "Found it.".into(),
            },
            Change::Reload,
        ]
    );
}

#[test]
fn events_it_cannot_place_ask_for_a_reload() {
    let mut live = Live::new();
    // A prompt delivered before this reducer saw it queued.
    let changes = live.apply(
        &session(|session_id| EventKind::InboxRemoved {
            session_id,
            item_id: "msg_old".into(),
            delivered: true,
        }),
        0.0,
    );
    assert_eq!(changes, [Change::Reload]);
    // A tool that ended without being seen to start.
    let changes = live.apply(
        &session(|session_id| EventKind::ToolEnded {
            session_id,
            message_id: "m".into(),
            call_id: "c".into(),
            failed: true,
            error: Some("boom".into()),
        }),
        0.0,
    );
    assert_eq!(changes, [Change::Reload]);
    // A cancelled prompt changes nothing.
    let changes = live.apply(
        &session(|session_id| EventKind::InboxRemoved {
            session_id,
            item_id: "msg_x".into(),
            delivered: false,
        }),
        0.0,
    );
    assert!(changes.is_empty());
}

#[test]
fn compaction_and_retries_show_as_rows() {
    let mut live = Live::new();
    let started = live.apply(
        &session(|session_id| EventKind::CompactionStarted { session_id }),
        0.0,
    );
    let ended = live.apply(
        &session(|session_id| EventKind::CompactionEnded {
            session_id,
            summary: "## Objective".into(),
        }),
        0.0,
    );
    assert!(
        matches!(&*started, [Change::Put(Entry::Compaction { state: Compaction::Running, key })] if key == "ses_1/compaction")
    );
    assert!(
        matches!(&*ended, [Change::Put(Entry::Compaction { state: Compaction::Done(s), .. })] if s == "## Objective")
    );

    let retry = live.apply(
        &session(|session_id| EventKind::RetryScheduled {
            session_id,
            attempt: 2,
            at: 0.0,
            error: "rate limited".into(),
        }),
        0.0,
    );
    assert!(
        matches!(&*retry, [Change::Put(Entry::Note { text, warning: true, .. })]
        if text == "Retrying (attempt 2): rate limited")
    );
}
