//! A real session's history, turned into timeline rows.
//!
//! Runs against a private `harlo-server` with scratch `HOME`/`XDG_*` folders
//! and a scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

#[path = "../../harlo-client/tests/support/mod.rs"]
mod support;

use harlo_core::server::{Delivery, Prompt};
use harlo_core::timeline::{Change, Entry, Live, entries};
use harlo_core::workspace::{EventKind, ToolStatus};
use support::{Sandbox, wait_for, wait_idle};

#[tokio::test(flavor = "multi_thread")]
async fn a_finished_run_becomes_prompt_tool_and_answer_rows() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("a new session");
    let script = r#"tool glob {"pattern":"*"} ;; say Found **the files**."#;
    client
        .prompt(&session.id, &Prompt::new(script, Delivery::Queue))
        .await
        .expect("the prompt is accepted");
    wait_for("the run to start", || async {
        let messages = client.messages(&session.id).await.ok()?;
        (messages.len() > 1).then_some(())
    })
    .await;
    wait_idle(&client, &session.id).await;

    let messages = client.messages(&session.id).await.expect("the history");
    let rows = entries(&messages);
    let prompt = rows
        .iter()
        .position(|row| matches!(row, Entry::Prompt { text, .. } if text == script))
        .unwrap_or_else(|| panic!("the prompt row: {rows:#?}"));
    let tool = rows
        .iter()
        .position(|row| {
            matches!(row, Entry::Tool { label, detail, status: ToolStatus::Completed, took: Some(_), .. }
                if label == "Find files" && detail == "*")
        })
        .unwrap_or_else(|| panic!("a finished Glob row with its pattern: {rows:#?}"));
    let answer = rows
        .iter()
        .position(|row| matches!(row, Entry::Answer { text, .. } if text.contains("**the files**")))
        .unwrap_or_else(|| panic!("the Markdown answer: {rows:#?}"));
    assert!(prompt < tool && tool < answer, "oldest first: {rows:#?}");

    // Keys are stable, so reloading keeps the same rows.
    let again = entries(&client.messages(&session.id).await.expect("the history"));
    assert_eq!(rows, again);
}

/// Applies live changes the way the timeline does; returns how many reloads were asked for.
fn apply(rows: &mut Vec<Entry>, changes: Vec<Change>) -> usize {
    let mut reloads = 0;
    for change in changes {
        match change {
            Change::Put(entry) => match rows.iter_mut().find(|row| row.key() == entry.key()) {
                Some(row) => *row = entry,
                None => rows.push(entry),
            },
            Change::Extend {
                key,
                thought,
                delta,
            } => match rows.iter_mut().find(|row| row.key() == key) {
                Some(Entry::Answer { text, .. } | Entry::Thought { text, .. }) => {
                    text.push_str(&delta);
                }
                Some(_) => panic!("extending a row that isn't text: {key}"),
                None if thought => rows.push(Entry::Thought { key, text: delta }),
                None => rows.push(Entry::Answer { key, text: delta }),
            },
            Change::Reload => reloads += 1,
        }
    }
    reloads
}

/// Tool times measured from events differ slightly from the stored ones.
/// Rows without what only the stored history knows: how long tools took,
/// and which agent answered a prompt, which the reload at a run's end fills.
fn without_times(rows: &[Entry]) -> Vec<Entry> {
    rows.iter()
        .cloned()
        .map(|mut row| {
            match &mut row {
                Entry::Tool { took, .. } => *took = None,
                Entry::Prompt { agent, .. } => *agent = None,
                _ => {}
            }
            row
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn streamed_rows_match_the_stored_history() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("a new session");
    let mut events = client.events().await.expect("the event stream");
    let script = r#"think Checking **the files** first | say Let me look. | tool glob {"pattern":"*.txt"} ;; pace 20 | say Found `a.txt` and **b.txt**, which is everything."#;
    client
        .prompt(&session.id, &Prompt::new(script, Delivery::Queue))
        .await
        .expect("the prompt is accepted");

    let mut live = Live::new();
    let mut rows = Vec::new();
    let mut reloads = 0;
    tokio::time::timeout(support::WAIT, async {
        loop {
            let event = events
                .next()
                .await
                .expect("the stream stays open")
                .expect("events decode");
            if event.kind.session_id() != Some(session.id.as_str()) {
                continue;
            }
            reloads += apply(&mut rows, live.apply(&event.kind, event.time));
            if matches!(event.kind, EventKind::Done { .. }) {
                break;
            }
        }
    })
    .await
    .expect("the run ends in time");

    assert_eq!(reloads, 1, "only the run's end asks for a reload");
    let stored = entries(&client.messages(&session.id).await.expect("the history"));
    assert_eq!(without_times(&rows), without_times(&stored));
    assert!(
        rows.iter().any(|row| matches!(
            row,
            Entry::Tool {
                took: Some(_),
                status: ToolStatus::Completed,
                ..
            }
        )),
        "the streamed tool row has its time: {rows:#?}"
    );
}
