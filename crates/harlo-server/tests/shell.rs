//! Commands agents run: one that outruns its time carries on in the
//! background instead of being stopped, and its result comes back to the
//! session when it ends.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo_client::{Client, Delivery, Effect, JobKind, JobStatus, Message, Prompt, Rule};
use support::Sandbox;

/// Waits until `count` turns of `session` have ended.
async fn idles(client: &Client, session: &str, count: usize) {
    for _ in 0..400 {
        let ended = client
            .messages(session)
            .await
            .expect("messages")
            .iter()
            .filter(|m| matches!(m, Message::Idle { .. }))
            .count();
        if ended >= count {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{count} turns never ended");
}

/// What the session's tool calls returned, in order.
async fn tool_outputs(sandbox: &Sandbox, session: &str) -> Vec<String> {
    let listed: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/api/session/{session}/message", sandbox.url()))
        .basic_auth("harlo", Some(&sandbox.password))
        .send()
        .await
        .expect("messages")
        .json()
        .await
        .expect("JSON");
    listed["data"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|m| m["content"].as_array().cloned().unwrap_or_default())
        .filter(|part| part["type"] == "tool")
        .map(|part| {
            part["state"]["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_that_outruns_its_time_carries_on_in_the_background() {
    // (Two minutes in use; a fraction of a second here.)
    let sandbox = Sandbox::start_with_env(&[("HARLO_SHELL_PATIENCE_MS", "400")]).await;
    let client = sandbox.client();
    let project = sandbox.project();
    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    let id = session.id.as_str();
    client
        .set_permissions(id, &[Rule::any("shell", Effect::Allow)])
        .await
        .expect("rules");

    let command = "echo started; seq 1 3000; sleep 1.5; echo finished; touch done.txt";
    let call = serde_json::json!({ "command": command });
    client
        .prompt(
            id,
            &Prompt::new(
                format!("tool shell {call} ;; say Carrying on."),
                Delivery::Queue,
            ),
        )
        .await
        .expect("prompt");

    // The turn ends while the command is still going: the model was told
    // where it had got to and that the rest follows.
    idles(&client, id, 1).await;
    let outputs = tool_outputs(&sandbox, id).await;
    let [told] = outputs.as_slice() else {
        panic!("one call: {outputs:?}")
    };
    // (Its last lines only: all of it comes with its end.)
    assert!(
        told.contains("lines before these") && told.contains("\n3000\n"),
        "{told}"
    );
    assert!(
        !told.contains("started") && !told.contains("\n2000\n"),
        "{told}"
    );
    assert!(told.contains("carries on in the background"), "{told}");
    assert!(!told.contains("finished"), "{told}");
    assert!(!project.join("done.txt").exists(), "still running");

    // It ran to its end, and the session ran again on its result.
    idles(&client, id, 2).await;
    assert!(project.join("done.txt").exists(), "it wasn't stopped");
    let result = support::turn_requests(sandbox.dir.path())
        .into_iter()
        .map(|r| r["messages"].to_string())
        .find(|m| m.contains("state=\\\"completed\\\""))
        .expect("the result reached the model");
    assert!(
        result.contains("finished") && result.contains("Exit code: 0"),
        "{result}"
    );
    // Cut as any command's output is, keeping its start and its end.
    assert!(
        result.contains("started") && result.contains("lines left out"),
        "{result}"
    );
    assert!(!result.contains("\\n1500\\n"), "{result}");
    let jobs = client.jobs(id).await.expect("jobs");
    let [job] = jobs.as_slice() else {
        panic!("one job: {jobs:?}")
    };
    assert_eq!(job.status, JobStatus::Completed);
    assert_eq!(
        job.kind,
        JobKind::Shell {
            command: command.to_owned()
        }
    );

    // A command given a limit of its own is stopped at it, as it asked.
    let call =
        serde_json::json!({ "command": "echo once; sleep 5; touch late.txt", "timeout": 300 });
    client
        .prompt(
            id,
            &Prompt::new(
                format!("tool shell {call} ;; say Stopped it."),
                Delivery::Queue,
            ),
        )
        .await
        .expect("prompt");
    idles(&client, id, 3).await;
    let outputs = tool_outputs(&sandbox, id).await;
    let stopped = outputs.last().expect("a call");
    assert!(
        stopped.contains("once") && stopped.contains("it took too long"),
        "{stopped}"
    );
    assert_eq!(
        client.jobs(id).await.expect("jobs").len(),
        1,
        "no job for it"
    );
    assert!(!project.join("late.txt").exists());
}

/// The ID of the session's call that is running, once there is one.
async fn running_call(sandbox: &Sandbox, session: &str) -> String {
    for _ in 0..200 {
        let listed: serde_json::Value = reqwest::Client::new()
            .get(format!("{}/api/session/{session}/message", sandbox.url()))
            .basic_auth("harlo", Some(&sandbox.password))
            .send()
            .await
            .expect("messages")
            .json()
            .await
            .expect("JSON");
        let running = listed["data"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|m| m["content"].as_array().cloned().unwrap_or_default())
            .find(|part| part["type"] == "tool" && part["state"]["status"] == "running");
        if let Some(id) = running.as_ref().and_then(|part| part["id"].as_str()) {
            return id.to_owned();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no call ever ran");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_sends_a_running_command_to_the_background() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    let id = session.id.as_str();
    client
        .set_permissions(id, &[Rule::any("shell", Effect::Allow)])
        .await
        .expect("rules");

    // A limit of its own doesn't keep a person from asking.
    let call = serde_json::json!({
        "command": "echo started; sleep 2; touch done.txt", "timeout": 60_000,
    });
    client
        .prompt(
            id,
            &Prompt::new(
                format!("tool shell {call} ;; say Carrying on."),
                Delivery::Queue,
            ),
        )
        .await
        .expect("prompt");
    let running = running_call(&sandbox, id).await;
    // (Give the command a moment to print.)
    tokio::time::sleep(Duration::from_millis(300)).await;
    client
        .move_to_background(id, &running)
        .await
        .expect("moved");

    // The turn goes on without it, told why.
    idles(&client, id, 1).await;
    assert!(!project.join("done.txt").exists(), "still running");
    let outputs = tool_outputs(&sandbox, id).await;
    let [told] = outputs.as_slice() else {
        panic!("one call: {outputs:?}")
    };
    assert!(told.contains("started"), "{told}");
    assert!(
        told.contains("The person sent it to the background"),
        "{told}"
    );

    // It runs to its end as a job, and the session hears of it.
    idles(&client, id, 2).await;
    assert!(project.join("done.txt").exists(), "it wasn't stopped");
    let jobs = client.jobs(id).await.expect("jobs");
    assert_eq!(jobs.len(), 1, "{jobs:?}");
    assert_eq!(jobs[0].status, JobStatus::Completed);

    // Once it has ended there is nothing left to send.
    let late = client.move_to_background(id, &running).await;
    assert!(late.is_err(), "{late:?}");
}
