//! M6 endpoints against a private server: what a session's model sees, its
//! running usage, and each model's context limit.
//!
//! Each test runs its own `harlo-server` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

mod support;

use harlo_client::{
    CommandSetup, Delivery, EventKind, FileChange, Message, ModelRef, Part, Prompt, Tokens,
};
use support::{Sandbox, WAIT, wait_for, wait_idle};

/// The scripted model reports these for every step.
const STEP: Tokens = Tokens {
    input: 10,
    output: 5,
    reasoning: 0,
    cache_read: 0,
    cache_write: 0,
};

/// A step's tokens and the session's totals arrive as events and in the
/// session's context, along with the model and agent the step used.
#[tokio::test(flavor = "multi_thread")]
async fn usage_and_context_follow_the_steps() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    assert_eq!(session.tokens, Tokens::default(), "nothing used yet");

    let mut events = client.events().await.expect("events");
    client
        .prompt(&session.id, &Prompt::new("say hello", Delivery::Steer))
        .await
        .expect("prompt");
    let (step, total) = tokio::time::timeout(WAIT, async {
        let (mut step, mut total) = (None, None);
        while let Some(event) = events.next().await {
            match event.expect("event").kind {
                EventKind::StepEnded {
                    session_id, tokens, ..
                } if session_id == session.id => step = Some(tokens),
                EventKind::UsageUpdated {
                    session_id, tokens, ..
                } if session_id == session.id => total = Some(tokens),
                _ => {}
            }
            if let (Some(step), Some(total)) = (step, total) {
                return (step, total);
            }
        }
        panic!("event stream ended");
    })
    .await
    .expect("usage was announced");
    assert_eq!(step, STEP);
    assert!(total.total() >= STEP.total());
    wait_idle(&client, &session.id).await;

    let read = client.session(&session.id).await.expect("session");
    assert!(read.tokens.total() >= STEP.total(), "{:?}", read.tokens);
    let context = client.context(&session.id).await.expect("context");
    assert!(context.messages >= 2, "the prompt and the answer");
    assert_eq!(context.last_step, Some(STEP));
    assert_eq!(
        context.model,
        Some(ModelRef {
            provider_id: "fake".into(),
            id: "fake-coder".into(),
            variant: None,
        })
    );
    assert_eq!(context.agent.as_deref(), Some("build"));
    assert!(context.loaded.is_empty());
}

/// A subfolder's `AGENTS.md`, loaded once the agent reads a file there,
/// shows among the session's loaded instructions.
#[tokio::test(flavor = "multi_thread")]
async fn instructions_loaded_on_the_way_are_listed() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let sub = sandbox.project().join("sub");
    std::fs::create_dir_all(&sub).expect("subfolder");
    std::fs::write(sub.join("AGENTS.md"), "Be brief here.\n").expect("AGENTS.md");
    std::fs::write(sub.join("x.txt"), "x\n").expect("x.txt");
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let script = r#"tool read {"path":"sub/x.txt"} ;; say read it"#;
    client
        .prompt(&session.id, &Prompt::new(script, Delivery::Steer))
        .await
        .expect("prompt");
    wait_idle(&client, &session.id).await;
    let context = client.context(&session.id).await.expect("context");
    let expected = sub.join("AGENTS.md");
    assert!(
        context
            .loaded
            .iter()
            .any(|p| std::path::Path::new(p) == expected),
        "{:?}",
        context.loaded
    );
}

/// Models carry their context window's size, when configured.
#[tokio::test(flavor = "multi_thread")]
async fn models_carry_their_context_limit() {
    let sandbox = Sandbox::start().await;
    let models = sandbox
        .client()
        .models(&sandbox.project_str())
        .await
        .expect("models");
    let fake = models
        .iter()
        .find(|m| m.provider_id == "fake" && m.id == "fake-coder")
        .expect("the scripted model");
    assert_eq!(fake.context, Some(128_000));
}

/// Agents carry their colour, whether others can hand them work, and whether
/// their rules stop them editing, from built-in and project agent files.
#[tokio::test(flavor = "multi_thread")]
async fn agents_say_how_they_can_be_used() {
    let sandbox = Sandbox::start().await;
    let agents_dir = sandbox.project().join(".harlo/agents");
    std::fs::create_dir_all(&agents_dir).expect("agents folder");
    std::fs::write(
        agents_dir.join("reviewer.md"),
        "---\ndescription: Reviews changes\nmode: all\ncolor: \"#ff6b6b\"\n\
         permissions:\n  - action: edit\n    resource: \"*\"\n    effect: deny\n---\n\
         Review the changes.\n",
    )
    .expect("agent file");
    let client = sandbox.client();
    // A folder's first requests can come before the server has loaded it, and
    // get no agents, or only the built-in ones; ask until the file's is there.
    let folder = sandbox.project_str();
    let agents = wait_for("the project's agent", || async {
        let agents = client.agents(&folder).await.expect("agents");
        agents.iter().any(|a| a.id == "reviewer").then_some(agents)
    })
    .await;
    let agent = |id: &str| {
        agents
            .iter()
            .find(|a| a.id == id)
            .unwrap_or_else(|| panic!("{id} in {agents:?}"))
    };
    let (build, plan, explore, general) = (
        agent("build"),
        agent("plan"),
        agent("explore"),
        agent("general"),
    );
    assert!(build.primary && !build.subagent && !build.read_only);
    assert!(plan.primary && plan.read_only, "plan doesn't edit");
    assert!(!explore.primary && explore.subagent && explore.read_only);
    assert!(general.subagent && !general.read_only);

    let reviewer = agent("reviewer");
    assert!(reviewer.primary && reviewer.subagent, "mode all");
    assert!(reviewer.read_only);
    assert_eq!(reviewer.color.as_deref(), Some("#ff6b6b"));
    assert_eq!(reviewer.description.as_deref(), Some("Reviews changes"));
}

/// Commands say which agent runs them and whether as a subagent, from the
/// configuration file and from command files' front matter alike.
#[tokio::test]
async fn commands_say_how_they_run() {
    let sandbox = Sandbox::start().await;
    let dot = sandbox.project().join(".harlo");
    std::fs::create_dir_all(dot.join("commands/group")).expect("commands folder");
    std::fs::create_dir_all(dot.join("command")).expect("command folder");
    std::fs::write(
        dot.join("commands/plan-it.md"),
        "---\ndescription: Plan first\nagent: plan\nsubagent: true\n---\nPlan $ARGUMENTS.\n",
    )
    .expect("command file");
    std::fs::write(
        dot.join("commands/group/deep.md"),
        "---\nsubtask: true\n---\nLook.\n",
    )
    .expect("nested command file");
    std::fs::write(dot.join("command/plain.md"), "No front matter.\n").expect("plain file");
    std::fs::write(
        dot.join("harlo.json"),
        r#"{"experimental":{"todo":true},"commands":{"tidy":{"template":"Tidy up.","agent":"explore"}}}"#,
    )
    .expect("config file");
    let client = sandbox.client();
    let folder = sandbox.project_str();
    let commands = wait_for("the project's commands", || async {
        let commands = client.commands(&folder).await.expect("commands");
        let names: Vec<_> = commands.iter().map(|c| c.name.as_str()).collect();
        ["plan-it", "group/deep", "plain", "tidy"]
            .iter()
            .all(|n| names.contains(n))
            .then_some(commands)
    })
    .await;
    assert!(!commands.is_empty());

    let setup = client.setup(&folder).await.expect("setup");
    assert!(setup.todo_tool, "experimental.todo");
    let command = |name: &str| {
        setup
            .commands
            .get(name)
            .cloned()
            .unwrap_or_else(|| panic!("{name} in {:?}", setup.commands))
    };
    let plan = command("plan-it");
    assert_eq!(plan.agent.as_deref(), Some("plan"));
    assert!(plan.subagent);
    assert!(command("group/deep").subagent, "the old name, subtask");
    assert_eq!(command("plain"), CommandSetup::default());
    let tidy = command("tidy");
    assert_eq!(tidy.agent.as_deref(), Some("explore"));
    assert!(!tidy.subagent);

    // Without the setting, the todo tool is off.
    let elsewhere = sandbox.root().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("another folder");
    let other = client
        .setup(elsewhere.to_str().expect("a UTF-8 path"))
        .await
        .expect("setup");
    assert!(!other.todo_tool);
    assert!(!other.commands.contains_key("plan-it"));
}

/// Handing work to a subagent: the call names the child session, the child
/// is listed under its parent, and its report comes back live and in history.
#[tokio::test(flavor = "multi_thread")]
async fn a_subagent_runs_in_a_child_session_and_reports_back() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let parent = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let mut events = client.events().await.expect("events");
    let script = r#"tool subagent {"agent":"explore","description":"Find the docs","prompt":"say Found them."} ;; say Asked Explore."#;
    client
        .prompt(&parent.id, &Prompt::new(script, Delivery::Steer))
        .await
        .expect("prompt");
    let (child, (report_id, report)) = tokio::time::timeout(WAIT, async {
        let (mut child, mut report) = (None, None);
        while let Some(event) = events.next().await {
            match event.expect("event").kind {
                EventKind::ToolEnded {
                    session_id,
                    child: Some(id),
                    ..
                } if session_id == parent.id => child = Some(id),
                EventKind::SubagentReported {
                    session_id,
                    id,
                    report: r,
                } if session_id == parent.id => {
                    report = Some((id, r));
                }
                _ => {}
            }
            if let (Some(child), Some(report)) = (&child, &report) {
                return (child.clone(), report.clone());
            }
        }
        panic!("event stream ended");
    })
    .await
    .expect("the subagent started and reported");
    assert_eq!(report.child, child);
    assert_eq!(report.agent, "Explore");
    assert_eq!(report.task, "Find the docs");
    assert_eq!(report.state, "completed");
    assert!(!report.text.contains("<subagent"), "{report:?}");

    let children = client.child_sessions(&parent.id).await.expect("children");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].id, child);
    assert_eq!(children[0].parent_id.as_deref(), Some(parent.id.as_str()));
    assert_eq!(children[0].agent.as_deref(), Some("explore"));
    assert_eq!(children[0].title.as_deref(), Some("Find the docs"));
    let roots = client.root_sessions().await.expect("roots");
    assert!(
        !roots.iter().any(|s| s.id == child),
        "children aren't roots"
    );

    wait_idle(&client, &parent.id).await;
    let history = client.messages(&parent.id).await.expect("history");
    let called = history.iter().find_map(|m| match m {
        Message::Assistant { parts, .. } => parts.iter().find_map(|p| match p {
            Part::Tool(call) if call.name == "subagent" => call.child.clone(),
            _ => None,
        }),
        _ => None,
    });
    assert_eq!(called.as_deref(), Some(child.as_str()));
    assert!(
        history
            .iter()
            .any(|m| matches!(m, Message::Report { id, report: r } if r.child == child && *id == report_id)),
        "the report is in the parent's history, by the same ID"
    );
}

/// Files through the server: a folder's children, reading and writing a
/// file, and a watched file's changes reported as events.
#[tokio::test]
async fn files_list_read_write_and_report_changes() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let folder = sandbox.project_str();
    let project = sandbox.project();
    std::fs::create_dir_all(project.join("src/deep")).expect("folders");
    std::fs::write(project.join("src/main.rs"), "fn main() {}\n").expect("a file");
    std::fs::write(project.join("odd name ü.txt"), "héllo\n").expect("a file");

    std::fs::write(project.join(".gitignore"), "build/\n").expect("ignore file");
    std::fs::create_dir_all(project.join("build")).expect("an ignored folder");
    let mut root = client.list_dir(&folder, "").await.expect("root");
    root.sort_by(|a, b| a.path.cmp(&b.path));
    eprintln!("root: {root:?}");
    assert!(root.iter().any(|e| e.path == "src" && e.directory));
    assert!(
        root.iter()
            .any(|e| e.path == "odd name ü.txt" && !e.directory)
    );
    let src = client.list_dir(&folder, "src").await.expect("src");
    eprintln!("src: {src:?}");
    assert!(src.iter().any(|e| e.path == "src/main.rs" && !e.directory));
    assert!(src.iter().any(|e| e.path == "src/deep" && e.directory));

    let bytes = client
        .read_file(&folder, "odd name ü.txt")
        .await
        .expect("read");
    assert_eq!(bytes, "héllo\n".as_bytes());
    let _missing = client
        .read_file(&folder, "missing.txt")
        .await
        .expect_err("a missing file can't be read");

    client
        .write_file(&folder, "src/new/lib.rs", b"pub fn f() {}\n".to_vec())
        .await
        .expect("write");
    assert_eq!(
        std::fs::read_to_string(project.join("src/new/lib.rs")).expect("written"),
        "pub fn f() {}\n"
    );

    let mut events = client.events().await.expect("events");
    client
        .watch_file(&folder, "src/main.rs")
        .await
        .expect("watch");
    // Give the watcher a moment before the change it should see.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    std::fs::write(project.join("src/main.rs"), "fn main() { run(); }\n").expect("change");
    let (directory, file, change) = tokio::time::timeout(WAIT, async {
        while let Some(event) = events.next().await {
            if let EventKind::FileChanged {
                directory,
                file,
                change,
            } = event.expect("event").kind
            {
                return (directory, file, change);
            }
        }
        panic!("event stream ended");
    })
    .await
    .expect("the change was reported");
    eprintln!("changed: {directory} {file} {change:?}");
    assert_eq!(directory, folder);
    assert!(file.ends_with("src/main.rs"), "{file}");
    assert_eq!(change, FileChange::Changed);
    client
        .unwatch_file(&folder, "src/main.rs")
        .await
        .expect("unwatch");
}
