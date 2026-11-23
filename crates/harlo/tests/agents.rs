//! Settings → Agents against a private `opencode serve` with scratch
//! `HOME`/`XDG_*` folders: the project's agents, and an approval saved with
//! "Allow always" listed with its project and removed. Never the shared
//! service.

// Rust guideline compliant 2026-10-09

#[path = "../../opencode-client/tests/support/mod.rs"]
mod server;
mod support;

use std::sync::Arc;

use harlo::app::{Load, Message};
use harlo_core::server::{
    Connected, Delivery, Effect, PermissionReply, Prompt, Rule, Update, load,
};
use server::{Sandbox, WAIT, wait_for, wait_idle};
use support::tasks::Tasks;
use support::{demo, ui};

/// A subagent the project defines in a Markdown file, with its own model.
const REVIEWER: &str = "---\ndescription: Reviews what changed\nmode: subagent\n\
                        model: fake/fake-coder\n---\nReview the changes.\n";

/// Answers an edit's approval in a new session of `sandbox` with "Allow
/// always", which saves an approval for the project.
async fn allow_always(sandbox: &Sandbox) {
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    client
        .set_permissions(&session.id, &[Rule::any("edit", Effect::Ask)])
        .await
        .expect("rules");
    std::fs::write(sandbox.project().join("e.txt"), "one\ntwo\n").expect("e.txt");
    let script =
        r#"tool edit {"path":"e.txt","oldString":"two","newString":"three"} ;; say edited"#;
    client
        .prompt(&session.id, &Prompt::new(script, Delivery::Steer))
        .await
        .expect("prompt");
    let asked = wait_for("an approval", || async {
        client
            .permissions(&session.id)
            .await
            .ok()?
            .into_iter()
            .next()
    })
    .await;
    client
        .reply_permission(&session.id, &asked.id, PermissionReply::Always, None)
        .await
        .expect("reply");
    wait_idle(&client, &session.id).await;
}

#[test]
fn agents_and_saved_approvals_are_shown_and_an_approval_removed() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let agents = sandbox.project().join(".opencode/agents");
    std::fs::create_dir_all(&agents).expect("agents folder");
    std::fs::write(agents.join("reviewer.md"), REVIEWER).expect("reviewer.md");
    tasks.block_on(allow_always(&sandbox));
    let snapshot = tasks.block_on(load(&sandbox.client())).expect("snapshot");

    let mut app = demo::new_app();
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        version: harlo_core::OPENCODE_VERSION.to_owned(),
        snapshot,
    }))));
    app.new_folder = Some(sandbox.project_str());
    let ui = ui(app);
    ui.press("Settings");
    ui.press("Agents");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.app().catalog().is_some()
            && matches!(ui.app().sheets.approvals.list, Load::Loaded(_))),
        "the agents and approvals"
    );
    assert!(ui.shows("Build"), "a built-in agent");
    assert!(ui.shows("Primary"));
    assert!(ui.shows("reviewer"), "the project's own agent");
    assert!(ui.shows("Subagent · Fake Coder"), "its own model");
    assert!(ui.shows("Reviews what changed"));

    let approvals = match &ui.app().sheets.approvals.list {
        Load::Loaded(list) => list.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(approvals.len(), 1);
    let label = format!("Edit: {}", approvals[0].resource);
    assert!(ui.shows(&label), "{approvals:#?}");
    assert!(ui.shows("project"), "its project's name");
    ui.press("Remove");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("None saved.")),
        "removed: {:?}",
        ui.app().sheets.approvals.error
    );
    assert!(
        tasks
            .block_on(sandbox.client().saved_approvals())
            .expect("list")
            .is_empty()
    );
}
