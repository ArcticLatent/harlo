//! The user's shell commands and background jobs against a private server:
//! a command runs with no model and joins the conversation; a command the
//! agent sends to the background is listed, reported and stopped.
//!
//! Each test runs its own `harlo-server` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

mod support;

use harlo_client::{
    Delivery, Effect, EventKind, JobKind, JobStatus, Message, Prompt, Rule, ShellRun, ShellStatus,
};
use support::{Sandbox, WAIT, wait_for};

#[tokio::test(flavor = "multi_thread")]
async fn a_shell_command_runs_with_no_model_and_joins_the_conversation() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let id = session.id.as_str();
    let mut events = client.events().await.expect("the event stream");

    client
        .run_shell(id, "echo hello; ls; exit 3")
        .await
        .expect("ran");

    // The server says it started and how it ended.
    let (mut started, mut ended) = (None, None);
    tokio::time::timeout(WAIT, async {
        while ended.is_none() {
            let Some(Ok(event)) = events.next().await else {
                panic!("the events ended")
            };
            match event.kind {
                EventKind::ShellStarted { session_id, run } if session_id == id => {
                    started = Some(run);
                }
                EventKind::ShellEnded { session_id, run } if session_id == id => {
                    ended = Some(run);
                }
                _ => {}
            }
        }
    })
    .await
    .expect("both events");
    let started = started.expect("started");
    let ended = ended.expect("ended");
    assert_eq!(started.status, ShellStatus::Running);
    assert_eq!(started.command, "echo hello; ls; exit 3");
    assert_eq!(ended.shell_id, started.shell_id);
    assert_eq!(ended.status, ShellStatus::Exited);
    assert_eq!(ended.exit, Some(3));
    assert!(ended.output.starts_with("hello\n"), "{}", ended.output);
    assert!(ended.output.contains("a.txt"), "{}", ended.output);

    // Its message has the same, and the model was never asked.
    let messages = client.messages(id).await.expect("messages");
    let shell: Vec<&ShellRun> = messages
        .iter()
        .filter_map(|m| match m {
            Message::Shell { run, .. } => Some(run),
            _ => None,
        })
        .collect();
    assert_eq!(shell, [&ended]);
    // Its output waits for the agent's next turn; it starts none itself.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    assert_eq!(sandbox.model_requests(), 0);

    // The next prompt brings it: in the same request, just before it.
    client
        .prompt(id, &Prompt::new("say seen it", Delivery::Queue))
        .await
        .expect("prompt");
    support::wait_idle(&client, id).await;
    let log = std::fs::read_to_string(sandbox.root().join("model.jsonl")).expect("the log");
    let turn = log
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|r| r["body"].to_string().contains("say seen it") && r["body"]["tools"].is_array())
        .expect("the turn's request");
    let users: Vec<String> = turn["body"]["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["role"] == "user")
        .map(|m| m["content"].to_string())
        .collect();
    assert_eq!(users.len(), 2, "{users:#?}");
    assert!(
        users[0].contains("executed by the user") && users[0].contains("hello"),
        "{}",
        users[0]
    );
    assert!(users[1].contains("say seen it"), "{}", users[1]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_in_the_background_is_listed_reported_and_stopped() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let id = session.id.as_str();
    client
        .set_permissions(id, &[Rule::any("shell", Effect::Allow)])
        .await
        .expect("rules");
    assert!(client.jobs(id).await.expect("none yet").is_empty());
    let mut events = client.events().await.expect("the event stream");

    let script = r#"tool shell {"command":"sleep 60","background":true} ;; say started"#;
    client
        .prompt(id, &Prompt::new(script, Delivery::Steer))
        .await
        .expect("prompt");
    let job = wait_for("a running job", || async {
        client
            .jobs(id)
            .await
            .ok()?
            .into_iter()
            .find(|job| job.status == JobStatus::Running)
    })
    .await;
    assert_eq!(job.session_id, id);
    assert_eq!(
        job.kind,
        JobKind::Shell {
            command: "sleep 60".into()
        }
    );

    client.cancel_job(&job.id).await.expect("stopped");
    let stopped = tokio::time::timeout(WAIT, async {
        loop {
            let Some(Ok(event)) = events.next().await else {
                panic!("the events ended")
            };
            if let EventKind::JobUpdated(updated) = event.kind
                && updated.id == job.id
                && updated.status != JobStatus::Running
            {
                return updated;
            }
        }
    })
    .await
    .expect("its end reported");
    assert_eq!(stopped.status, JobStatus::Cancelled);
    assert!(stopped.completed.is_some());
    let listed = client.jobs(id).await.expect("the list");
    assert!(
        listed
            .iter()
            .all(|j| j.id != job.id || j.status == JobStatus::Cancelled),
        "{listed:?}"
    );
}
