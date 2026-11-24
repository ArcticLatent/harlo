//! Timeline rows built from a session's messages and its live events.

// Rust guideline compliant 2026-10-09

use std::time::Duration;

use harlo_core::server::Delivery;
use harlo_core::server::FileDiff;
use harlo_core::timeline::{
    Change, Compaction, Entry, Live, Todo, TodoStatus, current_todos, entries, summarize,
    tool_detail, tool_label, tools_summary,
};
use harlo_core::workspace::{EventKind, InboxItem, Message, Part, Report, ToolCall, ToolStatus};
use serde_json::json;

fn call(name: &str, input: serde_json::Value, took_ms: Option<f64>) -> Part {
    Part::Tool(ToolCall {
        id: format!("call_{name}"),
        name: name.into(),
        status: ToolStatus::Completed,
        input,
        took_ms,
        files: Vec::new(),
        child: None,
        pictures: Vec::new(),
        error: None,
        calls: Vec::new(),
    })
}

#[test]
fn a_failed_call_keeps_why() {
    let Part::Tool(read) = call("read", json!({"filePath": "src/lexer.rs"}), Some(3.0)) else {
        unreachable!("a tool call")
    };
    let failed = ToolCall {
        status: ToolStatus::Failed,
        error: Some("  File not found: src/lexer.rs\n".into()),
        ..read
    };
    let rows = entries(&[Message::Assistant {
        id: "msg_a".into(),
        agent: "build".into(),
        parts: vec![
            Part::Tool(failed.clone()),
            Part::Tool(ToolCall {
                id: "call_quiet".into(),
                error: Some(" ".into()),
                ..failed
            }),
        ],
        files: Vec::new(),
    }]);
    assert!(matches!(&rows[0], Entry::Tool { error: Some(why), .. }
            if why == "File not found: src/lexer.rs"));
    // A blank reason says nothing, rather than an empty line.
    assert!(matches!(&rows[1], Entry::Tool { error: None, .. }));
}

#[test]
fn messages_become_rows_in_order() {
    let messages = vec![
        Message::User {
            id: "msg_1".into(),
            text: "Fix the port".into(),
            attachments: vec!["a.txt".into()],
            pictures: Vec::new(),
            skills: vec!["browser".into()],
        },
        Message::Assistant {
            id: "msg_2".into(),
            agent: "build".into(),
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
            error: None,
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
            error: None,
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
            files: Vec::new(),
            todos: None,
            pictures: Vec::new(),
            error: None,
            script: None,
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
        tool_detail(
            "todowrite",
            &json!({ "todos": [
                { "content": "a", "status": "completed" },
                { "content": "b", "status": "in_progress" },
                { "content": "c", "status": "pending" },
                { "content": "d", "status": "cancelled" },
            ] })
        ),
        "1 of 3 done",
        "a dropped item counts for neither"
    );
    assert_eq!(
        tool_detail("todowrite", &json!({ "todos": [] })),
        "Cleared the list"
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
        agent: "build".into(),
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
    let done = |names: &[&'static str]| {
        names
            .iter()
            .map(|n| (*n, ToolStatus::Completed))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        tools_summary(done(&[
            "edit",
            "edit",
            "bash",
            "write",
            "edit",
            "todowrite"
        ])),
        "4 files edited · 1 command run · todos updated"
    );
    assert_eq!(
        tools_summary(done(&["read", "grep", "glob", "webfetch", "mystery"])),
        "1 file read · 2 searches · 1 page fetched · 1 tool used"
    );
    assert_eq!(tools_summary([]), "");
}

#[test]
fn failed_calls_are_counted_as_failed() {
    assert_eq!(
        tools_summary([
            ("read", ToolStatus::Completed),
            ("edit", ToolStatus::Failed),
            ("todowrite", ToolStatus::Failed),
        ]),
        "1 file read · 2 failed"
    );
    assert_eq!(
        tools_summary([("bash", ToolStatus::Failed)]),
        "1 failed",
        "nothing described as done"
    );
}

fn session(kind: impl FnOnce(String) -> EventKind) -> EventKind {
    kind("ses_1".into())
}

/// The streamed Glob call's row, as it stands at each step.
fn glob(status: ToolStatus, detail: &str, took_ms: Option<u64>) -> Change {
    Change::Put(Entry::Tool {
        key: "msg_a/tool/call_1".into(),
        name: "glob".into(),
        label: "Find files".into(),
        detail: detail.into(),
        status,
        took: took_ms.map(Duration::from_millis),
        files: Vec::new(),
        todos: None,
        pictures: Vec::new(),
        error: None,
        script: None,
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
                pictures: Vec::new(),
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
            files: Vec::new(),
            child: None,
            pictures: Vec::new(),
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
                pictures: Vec::new(),
                skills: Vec::new(),
                agent: None,
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
            files: Vec::new(),
            child: None,
            pictures: Vec::new(),
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

fn files_of(entry: &Entry) -> Vec<(String, u64, u64)> {
    match entry {
        Entry::Tool { files, .. } => files
            .iter()
            .map(|f| (f.file.clone(), f.additions, f.deletions))
            .collect(),
        other => panic!("not a tool row: {other:?}"),
    }
}

#[test]
fn edits_and_writes_carry_their_changes() {
    let edit = FileDiff {
        file: "a.txt".into(),
        patch: "@@ -1 +1 @@\n-alpha\n+ALPHA\n".into(),
        additions: 1,
        deletions: 1,
        status: "modified".into(),
    };
    let stored = Part::Tool(ToolCall {
        files: vec![edit.clone()],
        child: None,
        ..match call("edit", json!({ "path": "a.txt" }), Some(5.0)) {
            Part::Tool(call) => call,
            _ => unreachable!("a tool call"),
        }
    });
    let messages = vec![Message::Assistant {
        id: "msg_1".into(),
        agent: "build".into(),
        parts: vec![
            stored,
            call(
                "write",
                json!({ "path": "new.txt", "content": "one\ntwo\n" }),
                Some(5.0),
            ),
        ],
        files: Vec::new(),
    }];
    let rows = entries(&messages);
    assert_eq!(files_of(&rows[0]), [("a.txt".to_owned(), 1, 1)]);
    assert_eq!(files_of(&rows[1]), [("new.txt".to_owned(), 2, 0)]);

    // Live, the edit's patch comes with the success; a write's from its input.
    let mut live = Live::new();
    for (call_id, name, input, files) in [
        ("c1", "edit", json!({ "path": "a.txt" }), vec![edit]),
        (
            "c2",
            "write",
            json!({ "path": "new.txt", "content": "x\n" }),
            Vec::new(),
        ),
    ] {
        let _ = live.apply(
            &session(|session_id| EventKind::ToolStarted {
                session_id,
                message_id: "m".into(),
                call_id: call_id.into(),
                name: name.into(),
            }),
            0.0,
        );
        let called = live.apply(
            &session(|session_id| EventKind::ToolCalled {
                session_id,
                message_id: "m".into(),
                call_id: call_id.into(),
                input,
            }),
            1.0,
        );
        let [Change::Put(running)] = called.as_slice() else {
            panic!("{called:?}");
        };
        assert!(files_of(running).is_empty(), "nothing shown while it runs");
        let ended = live.apply(
            &session(|session_id| EventKind::ToolEnded {
                session_id,
                message_id: "m".into(),
                call_id: call_id.into(),
                failed: false,
                error: None,
                files,
                child: None,
                pictures: Vec::new(),
            }),
            2.0,
        );
        let [Change::Put(done)] = ended.as_slice() else {
            panic!("{ended:?}");
        };
        assert_eq!(files_of(done).len(), 1, "{done:?}");
    }
}

fn report(child: &str) -> Report {
    Report {
        child: child.into(),
        agent: "Explore".into(),
        task: "Find the docs".into(),
        state: "completed".into(),
        text: "Found them.".into(),
    }
}

#[test]
fn handing_work_to_a_subagent_is_a_card_and_its_report_a_row() {
    let input = json!({ "agent": "explore", "description": "Find the docs", "prompt": "Look." });
    let rows = entries(&[
        Message::Assistant {
            id: "msg_a".into(),
            agent: "build".into(),
            parts: vec![Part::Tool(ToolCall {
                child: Some("ses_child".into()),
                ..match call("subagent", input.clone(), Some(5.0)) {
                    Part::Tool(call) => call,
                    _ => unreachable!("a tool call"),
                }
            })],
            files: Vec::new(),
        },
        Message::Report {
            id: "msg_r".into(),
            report: report("ses_child"),
        },
    ]);
    assert_eq!(
        rows,
        [
            Entry::Subagent {
                key: "msg_a/tool/call_subagent".into(),
                agent: "explore".into(),
                task: "Find the docs".into(),
                child: Some("ses_child".into()),
                status: ToolStatus::Completed,
            },
            Entry::Report {
                key: "msg_r".into(),
                report: report("ses_child"),
            },
        ]
    );
}

#[test]
fn a_follow_up_names_the_child_it_goes_to() {
    let input = json!({
        "agent": "explore",
        "description": "Look again",
        "prompt": "And the tests?",
        "sessionID": "ses_child",
    });
    let rows = entries(&[Message::Assistant {
        id: "msg_a".into(),
        agent: "build".into(),
        parts: vec![call("subagent", input, None)],
        files: Vec::new(),
    }]);
    assert!(
        matches!(rows.as_slice(), [Entry::Subagent { child: Some(c), .. }] if c == "ses_child"),
        "{rows:?}"
    );
}

#[test]
fn a_live_subagent_call_and_report_need_no_reload() {
    let mut live = Live::new();
    let _ = live.apply(
        &session(|session_id| EventKind::ToolStarted {
            session_id,
            message_id: "msg_a".into(),
            call_id: "call_1".into(),
            name: "subagent".into(),
        }),
        1.0,
    );
    let _ = live.apply(
        &session(|session_id| EventKind::ToolCalled {
            session_id,
            message_id: "msg_a".into(),
            call_id: "call_1".into(),
            input: json!({ "agent": "explore", "description": "Find the docs", "prompt": "Look." }),
        }),
        2.0,
    );
    let ended = live.apply(
        &session(|session_id| EventKind::ToolEnded {
            session_id,
            message_id: "msg_a".into(),
            call_id: "call_1".into(),
            failed: false,
            error: None,
            files: Vec::new(),
            child: Some("ses_child".into()),
            pictures: Vec::new(),
        }),
        3.0,
    );
    assert_eq!(
        ended,
        [Change::Put(Entry::Subagent {
            key: "msg_a/tool/call_1".into(),
            agent: "explore".into(),
            task: "Find the docs".into(),
            child: Some("ses_child".into()),
            status: ToolStatus::Completed,
        })]
    );
    let reported = live.apply(
        &session(|session_id| EventKind::SubagentReported {
            session_id,
            id: "msg_r".into(),
            report: report("ses_child"),
        }),
        4.0,
    );
    assert_eq!(
        reported,
        [Change::Put(Entry::Report {
            key: "msg_r".into(),
            report: report("ses_child"),
        })]
    );
    // The agent reading it later changes nothing on screen.
    let read = live.apply(
        &session(|session_id| EventKind::InboxRemoved {
            session_id,
            item_id: "msg_r".into(),
            delivered: true,
        }),
        5.0,
    );
    assert!(read.is_empty(), "{read:?}");
}

fn todos(items: &[(&str, &str)]) -> serde_json::Value {
    let items: Vec<_> = items
        .iter()
        .map(|(content, status)| json!({ "content": content, "status": status }))
        .collect();
    json!({ "todos": items })
}

#[test]
fn the_todo_list_is_the_last_one_written() {
    let first = call(
        "todowrite",
        todos(&[("Read the spec", "in_progress"), ("Write it", "pending")]),
        Some(1.0),
    );
    let Part::Tool(second) = call(
        "todowrite",
        todos(&[
            ("Read the spec", "completed"),
            ("Write it", "in_progress"),
            ("Ship it", "someday"),
        ]),
        Some(1.0),
    ) else {
        unreachable!("a tool call")
    };
    let failed = ToolCall {
        id: "call_failed".into(),
        status: ToolStatus::Failed,
        input: todos(&[]),
        ..second.clone()
    };
    let rows = entries(&[Message::Assistant {
        id: "msg_a".into(),
        agent: "build".into(),
        parts: vec![
            first,
            Part::Tool(ToolCall {
                id: "call_second".into(),
                ..second
            }),
            Part::Tool(failed),
        ],
        files: Vec::new(),
    }]);
    assert_eq!(
        current_todos(&rows),
        Some(
            [
                Todo {
                    content: "Read the spec".into(),
                    status: TodoStatus::Completed,
                },
                Todo {
                    content: "Write it".into(),
                    status: TodoStatus::InProgress,
                },
            ]
            .as_slice()
        ),
        "a failed call doesn't count, nor does an unknown status"
    );
    assert_eq!(current_todos(&rows[..0]), None, "none written yet");
}

#[test]
fn a_live_todo_call_shows_its_list_once_its_arguments_arrive() {
    let mut live = Live::new();
    let started = live.apply(
        &session(|session_id| EventKind::ToolStarted {
            session_id,
            message_id: "msg_a".into(),
            call_id: "call_1".into(),
            name: "todowrite".into(),
        }),
        1.0,
    );
    let rows: Vec<Entry> = started
        .into_iter()
        .filter_map(|c| match c {
            Change::Put(entry) => Some(entry),
            _ => None,
        })
        .collect();
    assert_eq!(current_todos(&rows), None, "still streaming");
    let called = live.apply(
        &session(|session_id| EventKind::ToolCalled {
            session_id,
            message_id: "msg_a".into(),
            call_id: "call_1".into(),
            input: todos(&[("Plan", "in_progress")]),
        }),
        2.0,
    );
    let [Change::Put(entry)] = called.as_slice() else {
        panic!("one row: {called:?}")
    };
    assert_eq!(
        current_todos([entry]).map(<[Todo]>::len),
        Some(1),
        "shown while it runs"
    );
}

/// A prompt is answered by the agent of the first step after it.
#[test]
fn a_prompt_names_the_agent_that_answered_it() {
    let user = |id: &str| Message::User {
        id: id.into(),
        text: "Go".into(),
        attachments: Vec::new(),
        pictures: Vec::new(),
        skills: Vec::new(),
    };
    let step = |id: &str, agent: &str| Message::Assistant {
        id: id.into(),
        agent: agent.into(),
        parts: vec![Part::Text("Done".into())],
        files: Vec::new(),
    };
    let rows = entries(&[
        user("msg_1"),
        step("msg_2", "plan"),
        step("msg_3", "build"),
        user("msg_4"),
    ]);
    let agents: Vec<Option<&str>> = rows
        .iter()
        .filter_map(|row| match row {
            Entry::Prompt { agent, .. } => Some(agent.as_deref()),
            _ => None,
        })
        .collect();
    assert_eq!(agents, [Some("plan"), None]);
}

fn script_call(tool: &str, status: ToolStatus) -> harlo_core::server::ScriptCall {
    harlo_core::server::ScriptCall {
        tool: tool.into(),
        status,
        input: json!({ "path": "src/main.rs" }),
    }
}

#[test]
fn a_script_shows_its_code_its_calls_and_what_it_changed() {
    let code = "\n// Read both files at once\nconst [a, b] = await Promise.all([\n  tools.fs.read({ path: \"a\" }),\n  tools.fs.read({ path: \"b\" }),\n]);\nreturn a + b;\n";
    let Part::Tool(base) = call("execute", json!({ "code": code }), Some(40.0)) else {
        unreachable!("a tool call")
    };
    let failed = ToolCall {
        status: ToolStatus::Failed,
        error: Some("b: not found".into()),
        files: vec![FileDiff {
            file: "a".into(),
            patch: "@@ -1 +1 @@\n-x\n+y\n".into(),
            additions: 1,
            deletions: 1,
            status: "modified".into(),
        }],
        calls: vec![
            script_call("fs.read", ToolStatus::Completed),
            script_call("fs.read", ToolStatus::Failed),
        ],
        ..base
    };
    let rows = entries(&[Message::Assistant {
        id: "msg_a".into(),
        agent: "build".into(),
        parts: vec![Part::Tool(failed)],
        files: Vec::new(),
    }]);
    let Entry::Tool {
        label,
        detail,
        script: Some(script),
        files,
        ..
    } = &rows[0]
    else {
        panic!("a script row: {rows:?}")
    };
    assert_eq!(label, "Script");
    // Its first comment says what it's for.
    assert_eq!(detail, "Read both files at once");
    assert!(script.code.starts_with("// Read both"), "{script:?}");
    assert!(
        script.code.ends_with("return a + b;"),
        "trimmed: {script:?}"
    );
    assert_eq!(script.calls.len(), 2);
    assert_eq!(script.calls[1].status, ToolStatus::Failed);
    // A failed script's changes still happened, so they show.
    assert_eq!(files.len(), 1);

    // Without a comment, its first line.
    assert_eq!(
        tool_detail("execute", &json!({ "code": "\n  return 1;" })),
        "return 1;"
    );
    assert_eq!(
        harlo_core::timeline::call_detail(&json!({ "path": "src/main.rs", "x": 1 })),
        "src/main.rs"
    );
    assert_eq!(
        harlo_core::timeline::call_detail(&json!({ "n": 1 })),
        "{\"n\":1}"
    );
}

#[test]
fn a_running_script_shows_its_calls_and_files_as_they_come() {
    let mut live = Live::new();
    let started = live.apply(
        &session(|session_id| EventKind::ToolStarted {
            session_id,
            message_id: "msg_a".into(),
            call_id: "call_1".into(),
            name: "execute".into(),
        }),
        1_000.0,
    );
    assert!(matches!(&*started, [Change::Put(Entry::Tool { label, .. })] if label == "Script"));
    live.apply(
        &session(|session_id| EventKind::ToolCalled {
            session_id,
            message_id: "msg_a".into(),
            call_id: "call_1".into(),
            input: json!({ "code": "return await tools.fs.read({});" }),
        }),
        1_100.0,
    );
    let diff = FileDiff {
        file: "notes.md".into(),
        patch: "@@ -0,0 +1 @@\n+hi\n".into(),
        additions: 1,
        deletions: 0,
        status: "added".into(),
    };
    let progress = live.apply(
        &session(|session_id| EventKind::ToolProgress {
            session_id,
            message_id: "msg_a".into(),
            call_id: "call_1".into(),
            files: vec![diff.clone()],
            calls: vec![script_call("fs.read", ToolStatus::Running)],
        }),
        1_200.0,
    );
    let [
        Change::Put(Entry::Tool {
            status,
            files,
            script: Some(script),
            ..
        }),
    ] = &*progress
    else {
        panic!("one row: {progress:?}")
    };
    assert_eq!(*status, ToolStatus::Running);
    assert_eq!(script.calls[0].status, ToolStatus::Running);
    assert_eq!(files, &vec![diff]);
    assert_eq!(script.code, "return await tools.fs.read({});");

    // The end shows the calls so far, then reads the stored result for
    // the last ones.
    let ended = live.apply(
        &session(|session_id| EventKind::ToolEnded {
            session_id,
            message_id: "msg_a".into(),
            call_id: "call_1".into(),
            failed: false,
            error: None,
            files: Vec::new(),
            child: None,
            pictures: Vec::new(),
        }),
        1_500.0,
    );
    assert!(
        matches!(&*ended, [Change::Put(Entry::Tool { script: Some(script), status: ToolStatus::Completed, .. }), Change::Reload]
            if script.calls.len() == 1),
        "{ended:?}"
    );

    // Progress for a call it never saw start waits for the reload.
    let stray = live.apply(
        &session(|session_id| EventKind::ToolProgress {
            session_id,
            message_id: "msg_a".into(),
            call_id: "call_9".into(),
            files: Vec::new(),
            calls: Vec::new(),
        }),
        1_600.0,
    );
    assert!(stray.is_empty());
}
