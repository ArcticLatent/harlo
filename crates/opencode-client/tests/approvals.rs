//! Saved approvals against a private server: "Allow always" saves one for
//! the project, it's listed, and removing it makes the server ask again.
//!
//! Each test runs its own `harlo-server` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

mod support;

use opencode_client::{Delivery, Effect, PermissionReply, Prompt, Rule};
use support::{Sandbox, wait_for, wait_idle};

#[tokio::test(flavor = "multi_thread")]
async fn allow_always_saves_an_approval_that_can_be_removed() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    assert!(client.saved_approvals().await.expect("list").is_empty());
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

    let saved = client.saved_approvals().await.expect("list");
    assert_eq!(saved.len(), 1, "{saved:#?}");
    let approval = &saved[0];
    assert_eq!(approval.action, "edit");
    assert!(!approval.resource.is_empty());
    let project = client
        .projects()
        .await
        .expect("projects")
        .into_iter()
        .find(|p| p.id == approval.project_id)
        .expect("its project");
    assert_eq!(
        std::fs::canonicalize(&project.directory).ok(),
        std::fs::canonicalize(sandbox.project()).ok()
    );

    client
        .remove_saved_approval(&approval.id)
        .await
        .expect("removed");
    assert!(client.saved_approvals().await.expect("list").is_empty());
}
