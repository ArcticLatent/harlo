//! Goal checkpoints through a restart: each time a goal's reviewer has
//! looked, the goal keeps where the work stood; a server that stops in
//! the middle of a round pauses the goal, and resuming it tells the agent
//! the checkpoint and what the session did after it.
//!
//! A checkpoint belongs to its session: a goal in another worktree of the
//! same project never gets it.
//!
//! The test runs its own `harlo-server` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo_client::{Client, Delivery, PermissionReply};
use serde_json::{Value, json};
use support::Sandbox;

/// How long a step of the test may take, in 50 ms looks.
const LOOKS: usize = 400;

/// Session `id`'s goal as the server keeps it, checkpoint included.
async fn goal(sandbox: &Sandbox, id: &str) -> Value {
    reqwest::Client::new()
        .post(format!("{}/api/rpc/goal/get", sandbox.url()))
        .basic_auth("harlo", Some(&sandbox.password))
        .json(&json!({ "input": { "sessionID": id } }))
        .send()
        .await
        .expect("request")
        .json::<Value>()
        .await
        .expect("json")["output"]
        .clone()
}

/// Session `id`'s messages, as the server sends them.
async fn messages(sandbox: &Sandbox, id: &str) -> Vec<Value> {
    reqwest::Client::new()
        .get(format!("{}/api/session/{id}/message", sandbox.url()))
        .basic_auth("harlo", Some(&sandbox.password))
        .send()
        .await
        .expect("request")
        .json::<Value>()
        .await
        .expect("json")["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// Keeps `scripts` as the scripted model's answers from now on.
fn script(sandbox: &Sandbox, scripts: &Value) {
    std::fs::write(sandbox.dir.path().join("scripts.json"), scripts.to_string())
        .expect("the scripts");
}

/// Waits until `check` of session `id`'s goal holds, allowing the
/// commands its agent asks about, and returns the goal.
async fn goal_when(
    sandbox: &Sandbox,
    client: &Client,
    id: &str,
    what: &str,
    check: impl Fn(&Value) -> bool,
) -> Value {
    for _ in 0..LOOKS {
        for asked in client.permissions(id).await.expect("permissions") {
            client
                .reply_permission(id, &asked.id, PermissionReply::Once, None)
                .await
                .expect("allow");
        }
        let now = goal(sandbox, id).await;
        if check(&now) {
            return now;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{what}: {}", goal(sandbox, id).await);
}

/// What the agent was told when a goal with `objective` was taken up
/// again: the last such prompt the scripted model got.
fn resume_prompt(sandbox: &Sandbox, objective: &str) -> String {
    support::turn_requests(sandbox.dir.path())
        .iter()
        .rev()
        .flat_map(|body| {
            body["messages"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .rev()
        })
        .filter_map(|m| {
            m["content"].as_str().map(str::to_owned).or_else(|| {
                m["content"].as_array().map(|parts| {
                    parts
                        .iter()
                        .filter_map(|p| p["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                })
            })
        })
        .find(|text| {
            text.contains("Pick the goal up again where it stopped.") && text.contains(objective)
        })
        .unwrap_or_else(|| panic!("no resume prompt for {objective}"))
}

#[tokio::test(flavor = "multi_thread")]
#[expect(
    clippy::too_many_lines,
    reason = "one story: two goals, a restart, both resumed"
)]
async fn a_goal_stopped_by_a_restart_resumes_from_its_checkpoint_in_its_own_session() {
    let mut sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let a_dir = sandbox.project();
    let b_dir = sandbox.dir.path().join("worktree-b");
    support::run(
        &a_dir,
        &[
            "worktree",
            "add",
            "-q",
            b_dir.to_str().expect("utf-8"),
            "-b",
            "b",
        ],
    );

    // Worktree B: a goal that waits for the person, with its own checkpoint.
    script(
        &sandbox,
        &json!({ "<reviewer>": r#"{"verdict":"blocked","reason":"It needs the release date."}"# }),
    );
    let b = client
        .create_session(b_dir.to_str().expect("utf-8"))
        .await
        .expect("B");
    client
        .run_command(&b.id, "goal", "Write the notes", Delivery::Queue)
        .await
        .expect("B's goal");
    let waiting = goal_when(&sandbox, &client, &b.id, "B never paused", |g| {
        g["status"] == "paused"
    })
    .await;
    assert_eq!(waiting["checkpoint"]["round"], 1, "{waiting}");
    assert_eq!(
        waiting["checkpoint"]["missing"], "It needs the release date.",
        "{waiting}"
    );

    // Worktree A: a first run, reviewed and sent back; the second run
    // changes a file and is still answering when the server stops.
    script(
        &sandbox,
        &json!({
            "<reviewer>": r#"{"verdict":"continue","reason":"b.txt is missing.","next_step":"Write b.txt."}"#,
            "<goal>": r#"tool write {"path":"a2.txt","content":"one\n"} ;; tool shell {"command":"cat a2.txt"} ;; say a2.txt is written."#,
            "<round>": r#"tool write {"path":"b.txt","content":"two\n"} ;; pace 60 | long 40"#,
        }),
    );
    let a = client
        .create_session(a_dir.to_str().expect("utf-8"))
        .await
        .expect("A");
    assert_eq!(a.project_id, b.project_id, "worktrees share the project");
    client
        .run_command(&a.id, "goal", "Ship both files", Delivery::Queue)
        .await
        .expect("A's goal");
    let reviewed = goal_when(&sandbox, &client, &a.id, "A was never reviewed", |g| {
        g["rounds"] == 1
    })
    .await;
    let kept = &reviewed["checkpoint"];
    assert_eq!(reviewed["status"], "active", "{reviewed}");
    assert_eq!(kept["round"], 1, "{reviewed}");
    assert_eq!(
        kept["completed"],
        json!(["changed a2.txt", "ran `cat a2.txt` (exit 0)"]),
        "{reviewed}"
    );
    assert_eq!(kept["command"], "ran `cat a2.txt` (exit 0)", "{reviewed}");
    assert_eq!(kept["said"], "a2.txt is written.", "{reviewed}");
    assert_eq!(kept["missing"], "b.txt is missing.", "{reviewed}");
    assert_eq!(kept["next"], "Write b.txt.", "{reviewed}");
    let through = kept["through"].as_str().expect("through").to_owned();

    let mut midway = false;
    for _ in 0..LOOKS {
        let now = messages(&sandbox, &a.id).await;
        if a_dir.join("b.txt").exists()
            && now
                .last()
                .is_some_and(|m| m.to_string().contains("Section 0"))
        {
            midway = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(midway, "the second round never got going");
    sandbox.restart().await;

    // The goal waits, paused, with the checkpoint the reviewer left; B's
    // goal is as it was.
    let stopped = goal(&sandbox, &a.id).await;
    assert_eq!(stopped["status"], "paused", "{stopped}");
    assert_eq!(
        stopped["reason"], "The server stopped while the agent worked on it.",
        "{stopped}"
    );
    assert_eq!(stopped["checkpoint"], *kept, "{stopped}");
    assert_eq!(goal(&sandbox, &b.id).await, waiting);

    // Resumed, A's agent is told the checkpoint, and what the round the
    // server stopped in had done after it.
    script(&sandbox, &json!({ "<resume>": "say Picking it up." }));
    let client = sandbox.client();
    client
        .run_command(&a.id, "goal", "resume", Delivery::Queue)
        .await
        .expect("resume A");
    let reached = goal_when(&sandbox, &client, &a.id, "A was never reached", |g| {
        g["status"] == "complete"
    })
    .await;
    let told = resume_prompt(&sandbox, "Ship both files");
    for expected in [
        "<checkpoint>",
        "- changed a2.txt\n- ran `cat a2.txt` (exit 0)\n",
        "The agent's last words: a2.txt is written.",
        "Last command: ran `cat a2.txt` (exit 0)",
        "Still missing: b.txt is missing.",
        "Next: Write b.txt.",
        "<since>",
        "- changed b.txt\n",
        "- it stopped short: The server stopped before this step ended.",
    ] {
        assert!(told.contains(expected), "{expected}\n{told}");
    }
    // The new checkpoint is past the old one and holds both rounds.
    let last = &reached["checkpoint"];
    assert_eq!(last["round"], 2, "{reached}");
    assert_eq!(last["verified"], "The work is done and shown.", "{reached}");
    assert_eq!(
        last["completed"],
        json!([
            "changed a2.txt",
            "ran `cat a2.txt` (exit 0)",
            "changed b.txt"
        ]),
        "{reached}"
    );
    assert!(
        last["through"].as_str() > Some(through.as_str()),
        "{reached}"
    );
    assert!(last.get("missing").is_none(), "{reached}");

    // B's agent gets B's checkpoint and nothing of A's unfinished work.
    client
        .run_command(&b.id, "goal", "resume", Delivery::Queue)
        .await
        .expect("resume B");
    goal_when(&sandbox, &client, &b.id, "B was never reached", |g| {
        g["status"] == "complete"
    })
    .await;
    let told = resume_prompt(&sandbox, "Write the notes");
    assert!(
        told.contains("Still missing: It needs the release date."),
        "{told}"
    );
    for others in ["a2.txt", "b.txt", "Ship both files", "<since>"] {
        assert!(!told.contains(others), "{others}\n{told}");
    }
}

/// Rewrites session `id`'s stored goal with `change`, in the store of a
/// sandbox whose server is stopped.
fn rewrite_goal(database: &std::path::Path, id: &str, change: impl FnOnce(&mut Value)) {
    let store = harlo_server::Store::open(database).expect("the store");
    let key = format!("goal/{id}");
    let stored = store.setting(&key).expect("read").expect("a goal");
    let mut goal: Value = serde_json::from_str(&stored).expect("json");
    change(&mut goal);
    store.put_setting(&key, &goal.to_string()).expect("write");
}

#[tokio::test(flavor = "multi_thread")]
#[expect(
    clippy::too_many_lines,
    reason = "one story: earlier work, an old goal with a prompt stored, one with none"
)]
async fn a_goal_from_before_checkpoints_never_counts_what_the_session_did_before_it() {
    let mut sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let a = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("A");

    // The session edits a file and runs a command before it has any goal.
    client
        .prompt(
            &a.id,
            &harlo_client::Prompt::new(
                r#"tool write {"path":"unrelated.txt","content":"x\n"} ;; tool shell {"command":"echo unrelated-command"} ;; say Unrelated work is done."#,
                Delivery::Queue,
            ),
        )
        .await
        .expect("prompt");
    let mut answered = false;
    for _ in 0..LOOKS {
        for asked in client.permissions(&a.id).await.expect("permissions") {
            client
                .reply_permission(&a.id, &asked.id, PermissionReply::Once, None)
                .await
                .expect("allow");
        }
        if messages(&sandbox, &a.id)
            .await
            .iter()
            .any(|m| m["type"] == "idle")
        {
            answered = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(answered, "the unrelated work never ended");
    assert!(project.join("unrelated.txt").exists());

    // A goal: one round reviewed and sent back, the next cut short.
    script(
        &sandbox,
        &json!({
            "<reviewer>": r#"{"verdict":"continue","reason":"b.txt is missing.","next_step":"Write b.txt."}"#,
            "<goal>": r#"tool write {"path":"a2.txt","content":"one\n"} ;; tool shell {"command":"cat a2.txt"} ;; say a2.txt is written."#,
            "<round>": r#"tool write {"path":"b.txt","content":"two\n"} ;; pace 60 | long 40"#,
        }),
    );
    client
        .run_command(&a.id, "goal", "Ship both files", Delivery::Queue)
        .await
        .expect("the goal");
    goal_when(&sandbox, &client, &a.id, "never reviewed", |g| {
        g["rounds"] == 1
    })
    .await;
    let mut midway = false;
    for _ in 0..LOOKS {
        if project.join("b.txt").exists()
            && messages(&sandbox, &a.id)
                .await
                .last()
                .is_some_and(|m| m.to_string().contains("Section 0"))
        {
            midway = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(midway, "the second round never got going");

    // The server stops, and the goal is as an earlier Harlo kept it: it
    // says neither where it began nor what a reviewer saw.
    let database = sandbox.database();
    sandbox
        .restart_after(|_| {
            rewrite_goal(&database, &a.id, |goal| {
                let fields = goal.as_object_mut().expect("an object");
                assert!(fields.remove("from").is_some(), "it said where it began");
                assert!(fields.remove("checkpoint").is_some(), "it had a checkpoint");
            });
        })
        .await;
    let old = goal(&sandbox, &a.id).await;
    assert_eq!(old["status"], "paused", "{old}");
    assert!(
        old.get("from").is_none() && old.get("checkpoint").is_none(),
        "{old}"
    );

    // Resumed, its work is read from its first stored prompt on: both
    // rounds, and nothing of what the session did before.
    script(&sandbox, &json!({ "<resume>": "say Picking it up." }));
    let client = sandbox.client();
    client
        .run_command(&a.id, "goal", "resume", Delivery::Queue)
        .await
        .expect("resume");
    let reached = goal_when(&sandbox, &client, &a.id, "never reached", |g| {
        g["status"] == "complete"
    })
    .await;
    let told = resume_prompt(&sandbox, "Ship both files");
    for expected in [
        "No checkpoint was kept yet.",
        "- changed a2.txt\n- ran `cat a2.txt` (exit 0)\n- changed b.txt\n",
        "- it stopped short: The server stopped before this step ended.",
    ] {
        assert!(told.contains(expected), "{expected}\n{told}");
    }
    assert!(
        !told.contains("unrelated") && !told.contains("<checkpoint>"),
        "{told}"
    );
    assert_eq!(
        reached["checkpoint"]["completed"],
        json!([
            "changed a2.txt",
            "ran `cat a2.txt` (exit 0)",
            "changed b.txt"
        ]),
        "{reached}"
    );
    assert!(!reached.to_string().contains("unrelated"), "{reached}");

    // An old goal with no prompt of its own stored: nothing says where
    // its work began, so the agent is told the objective and no more.
    sandbox
        .restart_after(|_| {
            rewrite_goal(&database, &a.id, |goal| {
                *goal = json!({
                    "id": "goal_0000older", "objective": "Finish the old task",
                    "status": "paused", "reason": "Paused by the person.",
                    "rounds": 3, "maxRounds": 50,
                });
            });
        })
        .await;
    script(
        &sandbox,
        &json!({ "<resume>": r#"tool write {"path":"c.txt","content":"three\n"} ;; say c.txt is written."# }),
    );
    let client = sandbox.client();
    client
        .run_command(&a.id, "goal", "resume", Delivery::Queue)
        .await
        .expect("resume the old goal");
    let reached = goal_when(
        &sandbox,
        &client,
        &a.id,
        "the old goal was never reached",
        |g| g["status"] == "complete",
    )
    .await;
    let told = resume_prompt(&sandbox, "Finish the old task");
    assert!(
        told.contains("<objective>\nFinish the old task\n</objective>"),
        "{told}"
    );
    for absent in ["<since>", "<checkpoint>", "unrelated", "a2.txt", "b.txt"] {
        assert!(!told.contains(absent), "{absent}\n{told}");
    }
    // Its first checkpoint holds the round it was resumed for, alone.
    assert_eq!(reached["checkpoint"]["round"], 1, "{reached}");
    assert_eq!(
        reached["checkpoint"]["completed"],
        json!(["changed c.txt"]),
        "{reached}"
    );
}
