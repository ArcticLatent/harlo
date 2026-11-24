//! Project settings and worktrees against a private server: a project's
//! name, colour, icon and worktree command; making and removing worktrees;
//! archiving sessions.
//!
//! Each test runs its own `harlo-server` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

mod support;

use harlo_client::{Client, Project, ProjectChange};
use support::{Sandbox, wait_for};

/// The sandbox's project, once the server knows it from a session there.
async fn project(sandbox: &Sandbox, client: &Client) -> Project {
    client
        .create_session(&sandbox.project_str())
        .await
        .expect("a session");
    wait_for("the project", || async {
        client
            .projects()
            .await
            .ok()?
            .into_iter()
            .find(|p| p.directory == sandbox.project_str())
    })
    .await
}

#[tokio::test]
async fn a_projects_name_colour_icon_and_command_change_and_clear() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = project(&sandbox, &client).await;
    assert_eq!(
        (project.color.as_deref(), project.icon.as_deref()),
        (None, None)
    );

    let icon = "data:image/png;base64,iVBORw0KGgo=";
    let changed = client
        .update_project(
            &project.id,
            &ProjectChange {
                name: Some("Demo".to_owned()),
                color: Some("blue".to_owned()),
                icon: Some(icon.to_owned()),
                start: Some("npm install".to_owned()),
            },
        )
        .await
        .expect("changed");
    assert_eq!(changed.name.as_deref(), Some("Demo"));
    assert_eq!(changed.color.as_deref(), Some("blue"));
    assert_eq!(changed.icon.as_deref(), Some(icon));
    assert_eq!(changed.start.as_deref(), Some("npm install"));

    // A change leaves the rest as it was.
    let recoloured = client
        .update_project(
            &project.id,
            &ProjectChange {
                color: Some("pink".to_owned()),
                ..ProjectChange::default()
            },
        )
        .await
        .expect("recoloured");
    assert_eq!(recoloured.color.as_deref(), Some("pink"));
    assert_eq!(recoloured.icon.as_deref(), Some(icon), "the icon stays");
    assert_eq!(recoloured.name.as_deref(), Some("Demo"));

    // Empty clears.
    let cleared = client
        .update_project(
            &project.id,
            &ProjectChange {
                name: Some(String::new()),
                color: Some(String::new()),
                icon: Some(String::new()),
                start: Some(String::new()),
            },
        )
        .await
        .expect("cleared");
    assert_eq!(
        (cleared.name, cleared.color, cleared.icon, cleared.start),
        (None, None, None, None)
    );
}

#[tokio::test]
async fn worktrees_are_made_and_removed_with_changes_only_when_forced() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = project(&sandbox, &client).await;

    let folder = client
        .create_worktree(&project.id, Some("feature-a"), None)
        .await
        .expect("a worktree");
    assert!(folder.ends_with("feature-a"), "{folder}");
    assert!(
        std::path::Path::new(&folder).join("a.txt").is_file(),
        "a checkout of the project"
    );
    let listed = client.worktrees(&project.id).await.expect("the list");
    assert!(listed.contains(&folder), "{listed:?}");

    // With changes, it stays unless forced.
    std::fs::write(std::path::Path::new(&folder).join("new.txt"), "change").expect("a change");
    let refused = client
        .remove_worktree(&project.id, &folder, false)
        .await
        .expect_err("it has changes");
    assert!(refused.needs_force(), "{refused}");
    assert!(std::path::Path::new(&folder).exists());

    client
        .remove_worktree(&project.id, &folder, true)
        .await
        .expect("forced");
    assert!(!std::path::Path::new(&folder).exists());
    let listed = client.worktrees(&project.id).await.expect("the list");
    assert!(!listed.contains(&folder), "{listed:?}");
}

/// Whether the session `id` is in the list of sessions.
async fn listed(client: &Client, id: &str) -> bool {
    client
        .root_sessions()
        .await
        .expect("the sessions")
        .into_iter()
        .any(|s| s.id == id)
}

#[tokio::test]
async fn an_archived_session_leaves_the_list_and_comes_back() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("a session");
    assert!(listed(&client, &session.id).await);
    client
        .set_archived(&session.id, true)
        .await
        .expect("archived");
    assert!(
        !listed(&client, &session.id).await,
        "archived sessions are left out"
    );
    client
        .set_archived(&session.id, false)
        .await
        .expect("restored");
    assert!(listed(&client, &session.id).await);
}
