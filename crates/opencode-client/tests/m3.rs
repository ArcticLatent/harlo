//! M3 endpoints against a private server: what a folder offers, choosing a
//! model and agent, attachments, the inbox, and compaction.
//!
//! Each test runs its own `opencode serve` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

mod support;

use opencode_client::{Attachment, Delivery, EventKind, Mention, Message, ModelRef, Part, Prompt};
use support::{Sandbox, WAIT, wait_for, wait_idle};

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_lists_models_agents_commands_skills_and_files() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let folder = sandbox.project_str();

    let models = client.models(&folder).await.expect("models");
    assert!(
        models
            .iter()
            .any(|m| m.provider_id == "fake" && m.id == "fake-coder" && m.name == "Fake Coder"),
        "the configured model is listed: {models:#?}"
    );
    let default = client.default_model(&folder).await.expect("the default");
    assert_eq!(
        default.map(|m| (m.provider_id, m.id)),
        Some(("fake".to_owned(), "fake-coder".to_owned()))
    );

    let agents = client.agents(&folder).await.expect("agents");
    assert!(agents.iter().any(|a| a.id == "build" && a.primary));
    assert!(agents.iter().any(|a| a.id == "plan" && a.primary));
    assert!(
        agents.iter().any(|a| !a.primary),
        "subagents are listed too"
    );
    assert!(
        !agents.iter().any(|a| a.id == "title"),
        "hidden agents are left out"
    );

    let commands = client.commands(&folder).await.expect("commands");
    assert!(
        commands
            .iter()
            .any(|c| c.name == "init" && c.description.is_some())
    );
    let skills = client.skills(&folder).await.expect("skills");
    assert!(
        skills
            .iter()
            .all(|s| !s.id.is_empty() && !s.name.is_empty())
    );
    let resources = client.mcp_resources(&folder).await.expect("MCP resources");
    assert!(resources.is_empty(), "no MCP servers are configured");

    let found = client.find_files(&folder, "a.t", 10).await.expect("files");
    assert!(
        found.iter().any(|f| f.path == "a.txt" && !f.directory),
        "{found:#?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_takes_a_model_and_an_agent() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let model = ModelRef {
        provider_id: "fake".into(),
        id: "fake-coder".into(),
        variant: None,
    };
    client.set_model(&session.id, &model).await.expect("model");
    client.set_agent(&session.id, "plan").await.expect("agent");
    let session = client.session(&session.id).await.expect("the session");
    assert_eq!(session.model, Some(model));
    assert_eq!(session.agent.as_deref(), Some("plan"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_scripted_model_offers_efforts_and_a_turn_sends_the_chosen_one() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let folder = sandbox.project_str();
    let models = client.models(&folder).await.expect("models");
    let fake = models
        .iter()
        .find(|m| m.provider_id == "fake" && m.id == "fake-coder")
        .expect("the scripted model");
    // An OpenAI-compatible model the config adds, with no variants of its own.
    assert_eq!(fake.variants, ["low", "medium", "high"]);

    let session = client.create_session(&folder).await.expect("session");
    let model = ModelRef {
        provider_id: "fake".into(),
        id: "fake-coder".into(),
        variant: Some("high".into()),
    };
    client.set_model(&session.id, &model).await.expect("model");
    client
        .prompt(
            &session.id,
            &Prompt::new("say thought hard", Delivery::Queue),
        )
        .await
        .expect("prompt");
    wait_idle(&client, &session.id).await;
    let log = std::fs::read_to_string(sandbox.root().join("model.jsonl")).expect("the log");
    let sent = log
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|r| r["body"].to_string().contains("thought hard"))
        .expect("the turn's request");
    assert_eq!(sent["body"]["reasoning_effort"], "high", "{sent}");
}

#[tokio::test(flavor = "multi_thread")]
async fn files_and_pasted_images_go_with_a_prompt() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let text = "say read @a.txt";
    let mut prompt = Prompt::new(text, Delivery::Queue);
    prompt.files = vec![
        Attachment::file(&format!("{}/a.txt", sandbox.project_str()))
            .mentioned(Mention::find(text, "@a.txt")),
        Attachment::inline("image/png", &tiny_png(), "pasted.png"),
    ];
    let mut events = client.events().await.expect("events");
    client.prompt(&session.id, &prompt).await.expect("sent");
    let queued = tokio::time::timeout(WAIT, async {
        loop {
            let event = events.next().await.expect("open").expect("decodes");
            if let EventKind::InboxAdded { item, .. } = event.kind
                && item.id == prompt.id
            {
                break item;
            }
        }
    })
    .await
    .expect("the prompt is queued");
    assert_eq!(
        queued.attachments,
        ["a.txt", "pasted.png"],
        "the queued prompt names its attachments, so the live row can show them"
    );
    let queued: Vec<_> = queued.pictures.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(queued, ["pasted.png"], "only the picture is a picture");
    wait_idle(&client, &session.id).await;

    let messages = client.messages(&session.id).await.expect("history");
    let attachments = messages
        .iter()
        .find_map(|m| match m {
            Message::User {
                attachments,
                pictures,
                ..
            } => Some((attachments.clone(), pictures.clone())),
            _ => None,
        })
        .expect("the prompt");
    assert_eq!(attachments.0, ["a.txt", "pasted.png"]);
    let [picture] = attachments.1.as_slice() else {
        panic!("one picture: {:?}", attachments.1);
    };
    assert_eq!(
        (picture.name.as_str(), picture.mime.as_str()),
        ("pasted.png", "image/png")
    );
    assert_eq!(&*picture.data, tiny_png().as_slice(), "its bytes, as sent");
    let sent = std::fs::read_to_string(sandbox.root().join("model.jsonl")).expect("model log");
    assert!(sent.contains("alpha"), "the file's text reached the model");
    assert!(
        sent.contains("data:image/png;base64,"),
        "the image reached the model"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_picture_a_tool_reads_comes_back_with_its_bytes() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    std::fs::write(sandbox.project().join("pic.png"), tiny_png()).expect("pic.png");
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let mut events = client.events().await.expect("events");
    let script = r#"tool read {"path":"pic.png"} ;; say seen"#;
    client
        .prompt(&session.id, &Prompt::new(script, Delivery::Steer))
        .await
        .expect("sent");
    let live = tokio::time::timeout(WAIT, async {
        loop {
            let event = events.next().await.expect("open").expect("decodes");
            if let EventKind::ToolEnded { pictures, .. } = event.kind {
                break pictures;
            }
        }
    })
    .await
    .expect("the read ends");
    wait_idle(&client, &session.id).await;
    let saved = client
        .messages(&session.id)
        .await
        .expect("history")
        .into_iter()
        .flat_map(|m| match m {
            Message::Assistant { parts, .. } => parts,
            _ => Vec::new(),
        })
        .find_map(|part| match part {
            Part::Tool(call) => Some(call.pictures),
            _ => None,
        })
        .expect("the read");
    for pictures in [live, saved] {
        let [picture] = pictures.as_slice() else {
            panic!("one picture: {pictures:?}");
        };
        assert_eq!(picture.name, "pic.png");
        assert_eq!(picture.mime, "image/png");
        assert_eq!(&*picture.data, tiny_png().as_slice());
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_queued_prompt_can_steer_or_be_cancelled() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let mut events = client.events().await.expect("events");
    client
        .prompt(
            &session.id,
            &Prompt::new("pace 80 | long 3", Delivery::Queue),
        )
        .await
        .expect("a slow run");
    wait_for("the run to start", || async {
        let active = client.active().await.ok()?;
        active.contains(&session.id).then_some(())
    })
    .await;

    let queued = Prompt::new("say queued", Delivery::Queue);
    client.prompt(&session.id, &queued).await.expect("queued");
    let dropped = Prompt::new("say never", Delivery::Queue);
    client.prompt(&session.id, &dropped).await.expect("queued");
    let inbox = client.inbox(&session.id).await.expect("inbox");
    assert_eq!(
        inbox.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
        [queued.id.as_str(), dropped.id.as_str()]
    );

    client
        .set_delivery(&session.id, &queued.id, Delivery::Steer)
        .await
        .expect("steer");
    client
        .cancel_inbox(&session.id, &dropped.id)
        .await
        .expect("cancel");

    let (mut changed, mut cancelled) = (false, false);
    tokio::time::timeout(WAIT, async {
        while !(changed && cancelled) {
            let event = events.next().await.expect("open").expect("decodes");
            match event.kind {
                EventKind::InboxDeliveryChanged {
                    item_id, delivery, ..
                } if item_id == queued.id => {
                    assert_eq!(delivery, Delivery::Steer);
                    changed = true;
                }
                EventKind::InboxRemoved {
                    item_id,
                    delivered: false,
                    ..
                } if item_id == dropped.id => cancelled = true,
                _ => {}
            }
        }
    })
    .await
    .expect("both changes are reported");
    wait_idle(&client, &session.id).await;
    let messages = client.messages(&session.id).await.expect("history");
    let prompts: Vec<&str> = messages
        .iter()
        .filter_map(|m| match m {
            Message::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(prompts.contains(&"say queued"), "{prompts:?}");
    assert!(!prompts.contains(&"say never"), "{prompts:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn compaction_summarizes_the_session() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    client
        .prompt(
            &session.id,
            &Prompt::new("say something to summarize", Delivery::Queue),
        )
        .await
        .expect("sent");
    wait_idle(&client, &session.id).await;

    let mut events = client.events().await.expect("events");
    client.compact(&session.id).await.expect("compact");
    let mut kinds = Vec::new();
    tokio::time::timeout(WAIT, async {
        loop {
            let event = events.next().await.expect("open").expect("decodes");
            match &event.kind {
                EventKind::CompactionStarted { .. } => kinds.push("started"),
                EventKind::CompactionEnded { summary, .. } => {
                    assert!(summary.contains("## Objective"), "{summary}");
                    kinds.push("ended");
                }
                EventKind::CompactionFailed { error, .. } => panic!("compaction failed: {error}"),
                EventKind::Done { session_id, .. } if *session_id == session.id => break,
                _ => {}
            }
        }
    })
    .await
    .expect("compaction finishes");
    assert_eq!(kinds, ["started", "ended"]);

    let messages = client.messages(&session.id).await.expect("history");
    assert!(messages.iter().any(|m| matches!(m,
        Message::Compaction { status, summary: Some(_), .. } if status == "completed")));
}

/// An 8×8 red PNG, small enough to build by hand.
fn tiny_png() -> Vec<u8> {
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAYAAADED76LAAAAEklEQVR4nGP4z8DwHx9mGBkKAMLXf4EvceABAAAAAElFTkSuQmCC";
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(PNG)
        .expect("valid base64")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_stops_when_interrupted_and_keeps_its_partial_answer() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let mut events = client.events().await.expect("events");
    client
        .prompt(
            &session.id,
            &Prompt::new("pace 60 | long 6", Delivery::Queue),
        )
        .await
        .expect("a slow run");
    let outcome = tokio::time::timeout(WAIT, async {
        let mut interrupted = false;
        loop {
            let event = events.next().await.expect("open").expect("decodes");
            match event.kind {
                EventKind::TextDelta { session_id, .. }
                    if session_id == session.id && !interrupted =>
                {
                    client.interrupt(&session.id).await.expect("interrupt");
                    interrupted = true;
                }
                EventKind::Done {
                    session_id,
                    outcome,
                } if session_id == session.id => break outcome,
                _ => {}
            }
        }
    })
    .await
    .expect("the run stops");
    assert_eq!(outcome, "interrupted");

    let messages = client.messages(&session.id).await.expect("history");
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::Idle { outcome, .. } if outcome == "interrupted")),
        "{messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::Assistant { .. })),
        "the partial answer is kept"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_runs_in_the_session() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    client
        .run_command(&session.id, "init", "keep it short", Delivery::Queue)
        .await
        .expect("the command starts");
    wait_for("the command's prompt", || async {
        let messages = client.messages(&session.id).await.ok()?;
        messages
            .iter()
            .any(|m| matches!(m, Message::User { .. }))
            .then_some(())
    })
    .await;
    wait_idle(&client, &session.id).await;
    let messages = client.messages(&session.id).await.expect("history");
    let prompt = messages
        .iter()
        .find_map(|m| match m {
            Message::User { text, .. } => Some(text.clone()),
            _ => None,
        })
        .expect("the command became a prompt");
    assert!(
        prompt.contains("keep it short"),
        "the arguments are in the command's prompt: {prompt}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_prompt_can_load_a_project_skill() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let folder = sandbox.project_str();
    let skill_dir = sandbox
        .root()
        .join("project/.opencode/skills/release-notes");
    std::fs::create_dir_all(&skill_dir).expect("skill folder");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: release-notes\ndescription: Writes release notes.\n---\n\nKeep them short.\n",
    )
    .expect("skill file");
    let skill = wait_for("the project's skill to be listed", || async {
        let skills = client.skills(&folder).await.ok()?;
        skills.into_iter().find(|s| s.name == "release-notes")
    })
    .await;

    let session = client.create_session(&folder).await.expect("session");
    let mut prompt = Prompt::new("say noted", Delivery::Queue);
    prompt.skills = vec![skill.id.clone()];
    client.prompt(&session.id, &prompt).await.expect("sent");
    wait_idle(&client, &session.id).await;
    let messages = client.messages(&session.id).await.expect("history");
    let skills = messages
        .iter()
        .find_map(|m| match m {
            Message::User { skills, .. } => Some(skills.clone()),
            _ => None,
        })
        .expect("the prompt");
    assert_eq!(skills, ["release-notes"]);
    let sent = std::fs::read_to_string(sandbox.root().join("model.jsonl")).expect("model log");
    assert!(
        sent.contains("Keep them short."),
        "the skill's text reached the model"
    );
}
