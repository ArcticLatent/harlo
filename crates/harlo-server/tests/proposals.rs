//! Proposed notes: with notes on, a run that goes idle has its stretch
//! read in the background, and what the agent missed is proposed with the
//! session and message it comes from. Nothing proposed reaches a session
//! until the person accepts it; accepted, it is a note that replaces the
//! one it contradicts. A dismissed proposal doesn't come back when the
//! session is read again, and with notes off nothing is read at all.
//!
//! What was said with notes off is never read, and a look under way when
//! notes are switched publishes nothing. Of two decisions on a proposal
//! at once, one wins. A session driven by `/goal` alone is read too, with
//! the goal as the person's request. A turn that ends during a look is
//! read after it, and a look whose answer was cut short is made again.
//!
//! Each test runs its own `harlo-server` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo_client::{Client, Delivery, Message, PermissionReply, Prompt};
use serde_json::{Value, json};
use support::Sandbox;

/// How long a step of the test may take, in 50 ms looks.
const LOOKS: usize = 400;

async fn idles(client: &Client, session: &str) -> usize {
    client
        .messages(session)
        .await
        .expect("messages")
        .iter()
        .filter(|m| matches!(m, Message::Idle { .. }))
        .count()
}

/// Sends `text` and waits until the session is idle again.
async fn turn(client: &Client, session: &str, text: &str) {
    let before = idles(client, session).await;
    client
        .prompt(session, &Prompt::new(text, Delivery::Queue))
        .await
        .expect("prompt");
    for _ in 0..LOOKS {
        if idles(client, session).await > before {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the turn never ended");
}

/// Calls the sandbox's server; the JSON answer, if any.
async fn call(
    sandbox: &Sandbox,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Value {
    let mut request = reqwest::Client::new()
        .request(method, format!("{}{path}", sandbox.url()))
        .basic_auth("harlo", Some(&sandbox.password));
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.expect("request");
    assert!(
        response.status().is_success(),
        "{path}: {}",
        response.status()
    );
    response.json::<Value>().await.unwrap_or(Value::Null)
}

async fn listed(sandbox: &Sandbox, what: &str, project: &str) -> Vec<Value> {
    call(
        sandbox,
        reqwest::Method::GET,
        &format!("/harlo/{what}?projectID={project}"),
        None,
    )
    .await["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// Keeps `scripts` as the scripted model's answers from now on.
fn script(sandbox: &Sandbox, scripts: &Value) {
    std::fs::write(sandbox.dir.path().join("scripts.json"), scripts.to_string())
        .expect("the scripts");
}

/// The requests to propose notes the scripted model has answered.
fn looks(sandbox: &Sandbox) -> Vec<Value> {
    std::fs::read_to_string(sandbox.dir.path().join("model.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .map(|r| r["body"].clone())
        .filter(|b| {
            b["messages"][0]["content"]
                .as_str()
                .is_some_and(|s| s.starts_with(harlo_engine::memory::PROPOSE_SYSTEM))
        })
        .collect()
}

/// Waits until the scripted model has answered `count` requests to
/// propose notes, and the server has had time to store what came of the
/// last; returns the last request's words.
async fn looked(sandbox: &Sandbox, count: usize) -> String {
    for _ in 0..LOOKS {
        let seen = looks(sandbox);
        if seen.len() >= count {
            tokio::time::sleep(Duration::from_millis(300)).await;
            assert_eq!(looks(sandbox).len(), count, "one look for each stretch");
            return seen[count - 1]["messages"][1]["content"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the session was never read for notes ({count})");
}

/// The system prompt of the model request that answered `prompt`.
fn system_prompt_for(sandbox: &Sandbox, prompt: &str) -> String {
    support::turn_requests(sandbox.dir.path())
        .into_iter()
        .find(|r| r["messages"].to_string().contains(prompt))
        .and_then(|r| r["messages"][0]["content"].as_str().map(str::to_owned))
        .unwrap_or_else(|| panic!("no request answered {prompt:?}"))
}

/// The ID of the person's message saying `text` in `session`.
async fn message_id(sandbox: &Sandbox, session: &str, text: &str) -> String {
    call(
        sandbox,
        reqwest::Method::GET,
        &format!("/api/session/{session}/message"),
        None,
    )
    .await["data"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["type"] == "user" && m["text"] == text)
        .and_then(|m| m["id"].as_str())
        .unwrap_or_else(|| panic!("no message {text:?}"))
        .to_owned()
}

#[tokio::test(flavor = "multi_thread")]
#[expect(
    clippy::too_many_lines,
    reason = "one story: missed, proposed, edited, kept, replacing a note"
)]
async fn a_correction_the_agent_missed_is_proposed_and_kept_only_when_accepted() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project_dir = sandbox.project();
    let folder = project_dir.to_str().expect("utf-8");
    call(
        &sandbox,
        reqwest::Method::PUT,
        "/harlo/memory",
        Some(json!({ "enabled": true })),
    )
    .await;

    // The agent keeps a note itself, as before; that stretch holds
    // nothing more to propose.
    let a = client.create_session(folder).await.expect("A");
    turn(
        &client,
        &a.id,
        r#"tool remember {"text":"Cache in Redis: it is already running."} ;; say Kept."#,
    )
    .await;
    let project = a.project_id.clone();
    let kept = listed(&sandbox, "notes", &project).await;
    assert_eq!(kept.len(), 1, "{kept:?}");
    let redis = kept[0]["id"].as_str().expect("id").to_owned();
    looked(&sandbox, 1).await;
    assert!(listed(&sandbox, "proposals", &project).await.is_empty());

    // The person corrects it and the agent carries on without keeping it.
    script(
        &sandbox,
        &json!({
            "No, cache in SQLite instead: nothing else to run.":
                r#"tool read {"path":"a.txt"} ;; say SQLite it is."#,
            "<notes>": format!(
                r#"[{{"text":"Cache in SQLite: nothing else to run.","kind":"correction","scope":"project","message":"{{person}}","replaces":"{redis}"}}]"#
            ),
        }),
    );
    turn(
        &client,
        &a.id,
        "No, cache in SQLite instead: nothing else to run.",
    )
    .await;
    let asked = looked(&sandbox, 2).await;
    // The model read the new stretch only, beside the note that holds
    // and what was said just before.
    assert!(
        asked.contains(&format!(
            "[{redis}] (project note) Cache in Redis: it is already running."
        )),
        "{asked}"
    );
    let (before, stretch) = asked.split_once("<transcript>").expect("a transcript");
    assert!(
        before.contains("<before>") && before.contains("Kept."),
        "{asked}"
    );
    assert!(
        stretch.contains("No, cache in SQLite instead") && !stretch.contains("Kept."),
        "{asked}"
    );

    let proposed = listed(&sandbox, "proposals", &project).await;
    assert_eq!(proposed.len(), 1, "{proposed:?}");
    let proposal = &proposed[0];
    let said = message_id(
        &sandbox,
        &a.id,
        "No, cache in SQLite instead: nothing else to run.",
    )
    .await;
    assert_eq!(proposal["text"], "Cache in SQLite: nothing else to run.");
    assert_eq!(proposal["kind"], "correction");
    assert_eq!(proposal["state"], "pending");
    assert_eq!(proposal["projectID"], project.as_str());
    assert_eq!(proposal["source"]["session"], a.id.as_str());
    assert_eq!(proposal["source"]["message"], said.as_str());
    assert_eq!(
        proposal["excerpt"],
        "No, cache in SQLite instead: nothing else to run."
    );
    assert_eq!(proposal["replaces"], redis.as_str());
    let id = proposal["id"].as_str().expect("id").to_owned();

    // Proposed isn't kept: the notes are as they were, and a new session
    // is told the old one.
    assert_eq!(listed(&sandbox, "notes", &project).await, kept);
    let b = client.create_session(folder).await.expect("B");
    turn(&client, &b.id, "say Hello from B.").await;
    let system = system_prompt_for(&sandbox, "say Hello from B.");
    assert!(
        system.contains("Cache in Redis") && !system.contains("Cache in SQLite"),
        "{system}"
    );

    // Accepted with its words changed, it is a note with the proposal's
    // source, and the note it contradicts is replaced by it.
    let note = call(
        &sandbox,
        reqwest::Method::POST,
        &format!("/harlo/proposals/{id}/accept"),
        Some(json!({ "text": "Cache in SQLite: one file, nothing else to run." })),
    )
    .await["data"]
        .clone();
    assert_eq!(
        note["text"],
        "Cache in SQLite: one file, nothing else to run."
    );
    assert_eq!(note["source"], proposal["source"]);
    let now = listed(&sandbox, "notes", &project).await;
    assert_eq!(now.len(), 2, "{now:?}");
    let old = now.iter().find(|n| n["id"] == redis.as_str()).expect("old");
    assert_eq!(old["supersededBy"], note["id"], "{now:?}");
    assert!(listed(&sandbox, "proposals", &project).await.is_empty());
    // Decided, it can't be decided again.
    let again = reqwest::Client::new()
        .delete(format!("{}/harlo/proposals/{id}", sandbox.url()))
        .basic_auth("harlo", Some(&sandbox.password))
        .send()
        .await
        .expect("request");
    assert_eq!(again.status(), reqwest::StatusCode::BAD_REQUEST);

    let c = client.create_session(folder).await.expect("C");
    turn(&client, &c.id, "say Hello from C.").await;
    let system = system_prompt_for(&sandbox, "say Hello from C.");
    assert!(
        system.contains("Cache in SQLite: one file, nothing else to run.")
            && !system.contains(&redis),
        "{system}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dismissed_proposal_stays_away_and_notes_off_means_no_look() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project_dir = sandbox.project();
    let folder = project_dir.to_str().expect("utf-8");
    call(
        &sandbox,
        reqwest::Method::PUT,
        "/harlo/memory",
        Some(json!({ "enabled": true })),
    )
    .await;
    let nextest = r#"[{"text":"Tests run with nextest.","kind":"decision","message":"{person}"}]"#;
    let work = r#"tool read {"path":"a.txt"} ;; say Done."#;
    script(
        &sandbox,
        &json!({
            "Run the tests with nextest from now on.": work,
            "And again, with nextest.": work,
            "Format with rustfmt's defaults.": work,
            "One more thing, with notes off.": work,
            "<notes>": nextest,
        }),
    );
    let a = client.create_session(folder).await.expect("A");
    let project = a.project_id.clone();

    turn(&client, &a.id, "Run the tests with nextest from now on.").await;
    looked(&sandbox, 1).await;
    let proposed = listed(&sandbox, "proposals", &project).await;
    assert_eq!(proposed.len(), 1, "{proposed:?}");
    let id = proposed[0]["id"].as_str().expect("id");
    call(
        &sandbox,
        reqwest::Method::DELETE,
        &format!("/harlo/proposals/{id}"),
        None,
    )
    .await;
    assert!(listed(&sandbox, "proposals", &project).await.is_empty());
    assert!(listed(&sandbox, "notes", &project).await.is_empty());

    // The session goes on and is read again; the model proposes the same
    // thing from a later message. It stays dismissed.
    turn(&client, &a.id, "And again, with nextest.").await;
    let asked = looked(&sandbox, 2).await;
    assert!(asked.contains("And again, with nextest."), "{asked}");
    assert!(listed(&sandbox, "proposals", &project).await.is_empty());

    // Something else is still proposed.
    let mut scripts: Value = serde_json::from_str(
        &std::fs::read_to_string(sandbox.dir.path().join("scripts.json")).expect("scripts"),
    )
    .expect("json");
    scripts["<notes>"] = json!(
        r#"[{"text":"Format with rustfmt's defaults.","kind":"decision","message":"{person}"}]"#
    );
    script(&sandbox, &scripts);
    turn(&client, &a.id, "Format with rustfmt's defaults.").await;
    looked(&sandbox, 3).await;
    let proposed = listed(&sandbox, "proposals", &project).await;
    assert_eq!(proposed.len(), 1, "{proposed:?}");
    assert_eq!(proposed[0]["text"], "Format with rustfmt's defaults.");

    // With notes off no stretch is read.
    call(
        &sandbox,
        reqwest::Method::PUT,
        "/harlo/memory",
        Some(json!({ "enabled": false })),
    )
    .await;
    turn(&client, &a.id, "One more thing, with notes off.").await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(looks(&sandbox).len(), 3);
}

/// Turns notes on or off.
async fn notes(sandbox: &Sandbox, on: bool) {
    call(
        sandbox,
        reqwest::Method::PUT,
        "/harlo/memory",
        Some(json!({ "enabled": on })),
    )
    .await;
}

/// Waits until the scripted model has been asked to propose notes
/// `count` times, answered or not; returns the last request's words.
async fn asked(sandbox: &Sandbox, count: usize) -> String {
    for _ in 0..LOOKS {
        let seen = looks(sandbox);
        if seen.len() >= count {
            return seen[count - 1]["messages"][1]["content"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the session was never read for notes ({count})");
}

/// What the scripted agent does for a message: two steps.
const WORK: &str = r#"tool read {"path":"a.txt"} ;; say Done."#;

#[tokio::test(flavor = "multi_thread")]
async fn what_was_said_with_notes_off_is_never_read() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project_dir = sandbox.project();
    let a = client
        .create_session(project_dir.to_str().expect("utf-8"))
        .await
        .expect("A");
    let project = a.project_id.clone();
    script(
        &sandbox,
        &json!({
            "Before notes: deploy by hand.": WORK,
            "Run the tests with nextest.": WORK,
            "While off: the password hint is tulip.": r#"tool read {"path":"a.txt"} ;; say Tulip noted."#,
            "Format with rustfmt's defaults.": WORK,
        }),
    );

    // Said before notes were ever on, then notes are turned on.
    turn(&client, &a.id, "Before notes: deploy by hand.").await;
    notes(&sandbox, true).await;
    turn(&client, &a.id, "Run the tests with nextest.").await;
    let first = looked(&sandbox, 1).await;
    assert!(first.contains("Run the tests with nextest."), "{first}");
    assert!(!first.contains("deploy by hand"), "{first}");

    // Said with notes off; a model that names that message gets nowhere.
    notes(&sandbox, false).await;
    turn(&client, &a.id, "While off: the password hint is tulip.").await;
    let hidden = message_id(&sandbox, &a.id, "While off: the password hint is tulip.").await;
    let mut scripts: Value = serde_json::from_str(
        &std::fs::read_to_string(sandbox.dir.path().join("scripts.json")).expect("scripts"),
    )
    .expect("json");
    scripts["<notes>"] = json!(format!(
        r#"[{{"text":"The password hint is tulip.","kind":"decision","message":"{hidden}"}}]"#
    ));
    script(&sandbox, &scripts);
    notes(&sandbox, true).await;
    turn(&client, &a.id, "Format with rustfmt's defaults.").await;
    let second = looked(&sandbox, 2).await;
    assert!(
        second.contains("Format with rustfmt's defaults."),
        "{second}"
    );
    // Neither in the stretch nor in what came before it.
    for unread in ["tulip", "Tulip", "deploy by hand", &hidden] {
        assert!(!second.contains(unread), "{unread}\n{second}");
    }
    assert!(listed(&sandbox, "proposals", &project).await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_look_under_way_when_notes_are_switched_publishes_nothing() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project_dir = sandbox.project();
    let a = client
        .create_session(project_dir.to_str().expect("utf-8"))
        .await
        .expect("A");
    let project = a.project_id.clone();
    notes(&sandbox, true).await;
    // The first answer comes late.
    script(
        &sandbox,
        &json!({
            "Run the tests with nextest.": WORK,
            "Format with rustfmt's defaults.": WORK,
            "<notes>": [
                r#"[{"text":"Tests run with nextest.","kind":"decision","message":"{person}"}]"#,
                r#"[{"text":"Format with rustfmt's defaults.","kind":"decision","message":"{person}"}]"#,
            ],
            "<notes_wait>": [1500],
        }),
    );
    turn(&client, &a.id, "Run the tests with nextest.").await;
    asked(&sandbox, 1).await;
    // Off and on again before the model answers.
    notes(&sandbox, false).await;
    notes(&sandbox, true).await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert_eq!(looks(&sandbox).len(), 1);
    let found = listed(&sandbox, "proposals", &project).await;
    assert!(found.is_empty(), "{found:?}");

    // What is said from now on is read as before; the earlier stretch,
    // said before notes were turned on again, is not.
    turn(&client, &a.id, "Format with rustfmt's defaults.").await;
    let second = looked(&sandbox, 2).await;
    assert!(!second.contains("nextest"), "{second}");
    let found = listed(&sandbox, "proposals", &project).await;
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["text"], "Format with rustfmt's defaults.");
}

/// Sends a decision on a proposal; whether the server took it.
async fn decide(sandbox: &Sandbox, method: reqwest::Method, path: &str) -> bool {
    let response = reqwest::Client::new()
        .request(method, format!("{}{path}", sandbox.url()))
        .basic_auth("harlo", Some(&sandbox.password))
        .send()
        .await
        .expect("request");
    let taken = response.status().is_success();
    assert!(
        taken || response.status() == reqwest::StatusCode::BAD_REQUEST,
        "{path}: {}",
        response.status()
    );
    taken
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn of_two_decisions_on_a_proposal_at_once_one_wins() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project_dir = sandbox.project();
    let a = client
        .create_session(project_dir.to_str().expect("utf-8"))
        .await
        .expect("A");
    let project = a.project_id.clone();
    notes(&sandbox, true).await;

    // Four turns, three proposals from each.
    let mut scripts = json!({});
    let mut answers = Vec::new();
    for turn in 0..4 {
        scripts[format!("Decision round {turn}.")] = json!(WORK);
        let three: Vec<Value> = (0..3)
            .map(|n| json!({ "text": format!("Rule {turn}-{n} holds."), "kind": "decision", "message": "{person}" }))
            .collect();
        answers.push(Value::Array(three).to_string());
    }
    scripts["<notes>"] = json!(answers);
    script(&sandbox, &scripts);
    for round in 0..4 {
        turn(&client, &a.id, &format!("Decision round {round}.")).await;
        looked(&sandbox, round + 1).await;
    }
    let pending = listed(&sandbox, "proposals", &project).await;
    assert_eq!(pending.len(), 12, "{pending:?}");

    let mut kept = 0;
    for (index, proposal) in pending.iter().enumerate() {
        let id = proposal["id"].as_str().expect("id");
        let accept = format!("/harlo/proposals/{id}/accept");
        let dismiss = format!("/harlo/proposals/{id}");
        let before = listed(&sandbox, "notes", &project).await.len();
        // Even ones are accepted twice; odd ones accepted and dismissed.
        let (first, second) = if index % 2 == 0 {
            tokio::join!(
                decide(&sandbox, reqwest::Method::POST, &accept),
                decide(&sandbox, reqwest::Method::POST, &accept)
            )
        } else {
            tokio::join!(
                decide(&sandbox, reqwest::Method::POST, &accept),
                decide(&sandbox, reqwest::Method::DELETE, &dismiss)
            )
        };
        assert!(first != second, "exactly one decision is taken: {id}");
        let now = listed(&sandbox, "notes", &project).await;
        let accepted = index % 2 == 0 || first;
        assert_eq!(now.len(), before + usize::from(accepted), "{now:?}");
        let text = proposal["text"].as_str().expect("text");
        let notes_of_it = now.iter().filter(|n| n["text"] == text).count();
        assert_eq!(notes_of_it, usize::from(accepted), "{now:?}");
        kept += usize::from(accepted);
    }
    assert!(listed(&sandbox, "proposals", &project).await.is_empty());
    assert_eq!(listed(&sandbox, "notes", &project).await.len(), kept);
}

/// Session `id`'s messages, as the server sends them.
async fn stored(sandbox: &Sandbox, id: &str) -> Vec<Value> {
    call(
        sandbox,
        reqwest::Method::GET,
        &format!("/api/session/{id}/message"),
        None,
    )
    .await["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_driven_by_a_goal_alone_is_read_with_the_goal_as_the_request() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project_dir = sandbox.project();
    let a = client
        .create_session(project_dir.to_str().expect("utf-8"))
        .await
        .expect("A");
    let project = a.project_id.clone();
    notes(&sandbox, true).await;
    // The reviewer sends the agent back once. The model then proposes one
    // note from the reviewer's words and one from the person's goal.
    script(
        &sandbox,
        &json!({
            "<goal>": r#"tool write {"path":"a2.txt","content":"one\n"} ;; tool shell {"command":"cat a2.txt"} ;; say a2.txt is written."#,
            "<round>": r#"tool write {"path":"b.txt","content":"two\n"} ;; say b.txt is written."#,
            "<reviewer>": [
                r#"{"verdict":"continue","reason":"b.txt is missing.","next_step":"Write b.txt."}"#,
                r#"{"verdict":"done","reason":"Both files are there."}"#,
            ],
            "<notes>": r#"[{"text":"Write b.txt before anything else.","kind":"decision","message":"{follow_up}"},{"text":"Show each file with cat once it is written.","kind":"decision","message":"{person}"}]"#,
        }),
    );
    client
        .run_command(
            &a.id,
            "goal",
            "Ship both files, and show each with cat once it is written",
            Delivery::Queue,
        )
        .await
        .expect("the goal");
    let mut done = false;
    for _ in 0..LOOKS {
        for wanted in client.permissions(&a.id).await.expect("permissions") {
            client
                .reply_permission(&a.id, &wanted.id, PermissionReply::Once, None)
                .await
                .expect("allow");
        }
        if !looks(&sandbox).is_empty() {
            done = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(done, "the goal's session was never read for notes");
    let read = looked(&sandbox, 1).await;
    assert!(project_dir.join("b.txt").exists(), "the goal did its work");

    // No message of the person's: the goal they set stands for it, and
    // what the reviewer sent back is shown as the server's.
    let all = stored(&sandbox, &a.id).await;
    assert!(all.iter().all(|m| m["type"] != "user"), "{all:?}");
    let of_goal = |round: bool| {
        all.iter()
            .find(|m| m["metadata"]["source"] == "goal" && m["metadata"]["round"].is_u64() == round)
            .and_then(|m| m["id"].as_str())
            .expect("a goal prompt")
            .to_owned()
    };
    let (set, sent_back) = (of_goal(false), of_goal(true));
    assert!(
        read.contains(&format!(
            "Person ({set}):\n/goal Ship both files, and show each with cat once it is written\n"
        )),
        "{read}"
    );
    assert!(
        read.contains(&format!(
            "Goal follow-up ({sent_back}):\nThe reviewer says the goal isn't met yet: b.txt is missing.\nNext: Write b.txt.\n"
        )),
        "{read}"
    );
    assert_eq!(read.matches("Person (").count(), 1, "{read}");

    let found = listed(&sandbox, "proposals", &project).await;
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        found[0]["text"],
        "Show each file with cat once it is written."
    );
    assert_eq!(found[0]["source"]["session"], a.id.as_str());
    assert_eq!(found[0]["source"]["message"], set.as_str());
    assert_eq!(
        found[0]["excerpt"],
        "/goal Ship both files, and show each with cat once it is written"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_that_ends_during_a_look_is_read_after_it() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project_dir = sandbox.project();
    let a = client
        .create_session(project_dir.to_str().expect("utf-8"))
        .await
        .expect("A");
    let project = a.project_id.clone();
    notes(&sandbox, true).await;
    // The first look's answer comes late; the second proposes a note.
    script(
        &sandbox,
        &json!({
            "Run the tests with nextest.": WORK,
            "Format with rustfmt's defaults.": WORK,
            "<notes>": [
                "[]",
                r#"[{"text":"Format with rustfmt's defaults.","kind":"decision","message":"{person}"}]"#,
            ],
            "<notes_wait>": [3000],
        }),
    );
    turn(&client, &a.id, "Run the tests with nextest.").await;
    asked(&sandbox, 1).await;
    // The second turn ends while the first look still waits.
    turn(&client, &a.id, "Format with rustfmt's defaults.").await;
    assert_eq!(looks(&sandbox).len(), 1, "the first look had ended already");
    assert!(listed(&sandbox, "proposals", &project).await.is_empty());

    // No third message: the second turn is read once the first look ends.
    let second = asked(&sandbox, 2).await;
    let second_again = looked(&sandbox, 2).await;
    assert_eq!(second, second_again);
    let (before, stretch) = second.split_once("<transcript>").expect("a transcript");
    assert!(
        stretch.contains("Format with rustfmt's defaults.") && !stretch.contains("nextest"),
        "{second}"
    );
    assert!(before.contains("Run the tests with nextest."), "{second}");
    let found = listed(&sandbox, "proposals", &project).await;
    assert_eq!(found.len(), 1, "{found:?}");
    let said = message_id(&sandbox, &a.id, "Format with rustfmt's defaults.").await;
    assert_eq!(found[0]["source"]["message"], said.as_str());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stretch_whose_answer_was_cut_short_is_read_again() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project_dir = sandbox.project();
    let a = client
        .create_session(project_dir.to_str().expect("utf-8"))
        .await
        .expect("A");
    let project = a.project_id.clone();
    notes(&sandbox, true).await;
    // An empty list, an answer cut short, then the whole answer twice.
    let whole = r#"[{"text":"Tests run with nextest.","kind":"decision","message":"{person}"}]"#;
    script(
        &sandbox,
        &json!({
            "Say hello first.": WORK,
            "Run the tests with nextest.": WORK,
            "And keep the output short.": WORK,
            "One more thing.": WORK,
            "<notes>": ["[]", r#"[{"text":"Tests run with nextest.","kind":"deci"#, whole, whole],
        }),
    );

    // An empty list is an answer: that stretch isn't read again.
    turn(&client, &a.id, "Say hello first.").await;
    looked(&sandbox, 1).await;

    // Cut short: nothing is stored, and the stretch stays to be read.
    turn(&client, &a.id, "Run the tests with nextest.").await;
    let cut = looked(&sandbox, 2).await;
    assert!(cut.contains("Run the tests with nextest."), "{cut}");
    assert!(listed(&sandbox, "proposals", &project).await.is_empty());

    turn(&client, &a.id, "And keep the output short.").await;
    let again = looked(&sandbox, 3).await;
    let (before, stretch) = again.split_once("<transcript>").expect("a transcript");
    assert!(
        stretch.contains("Run the tests with nextest.")
            && stretch.contains("And keep the output short."),
        "{again}"
    );
    assert!(
        !stretch.contains("Say hello first.") && before.contains("Say hello first."),
        "{again}"
    );
    let found = listed(&sandbox, "proposals", &project).await;
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["text"], "Tests run with nextest.");

    // The whole answer once more, for the next stretch alone: the
    // proposal is stored once.
    turn(&client, &a.id, "One more thing.").await;
    let last = looked(&sandbox, 4).await;
    let stretch = last.split_once("<transcript>").expect("a transcript").1;
    assert!(
        stretch.contains("One more thing.") && !stretch.contains("nextest"),
        "{last}"
    );
    let found = listed(&sandbox, "proposals", &project).await;
    assert_eq!(found.len(), 1, "{found:?}");
}
