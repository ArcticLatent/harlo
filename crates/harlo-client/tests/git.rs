//! What the Git settings rest on, against a private server: text written
//! with no session (commit messages), and the server's `snapshots` switch.
//!
//! Each test runs its own `harlo-server` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

mod support;

use harlo_client::{Delivery, ModelRef, Prompt, message_id};
use support::{Sandbox, wait_idle};

#[tokio::test]
async fn text_is_written_with_no_session() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let before = client.root_sessions().await.expect("the sessions").len();
    let model = ModelRef {
        provider_id: "fake".into(),
        id: "fake-coder".into(),
        variant: None,
    };
    let text = client
        .generate_text("Write a commit message for: +alpha", Some(&model))
        .await
        .expect("written");
    // The scripted model answers a prompt that isn't a script with "ok.".
    assert_eq!(text.trim(), "ok.");
    assert_eq!(sandbox.model_requests(), 1);
    assert_eq!(
        client.root_sessions().await.expect("the sessions").len(),
        before,
        "nothing saved"
    );
    // The server's default model when none is given.
    let text = client.generate_text("Again", None).await.expect("written");
    assert_eq!(text.trim(), "ok.");
}

/// With `snapshots` off in the server's config, undoing a turn can't put the
/// agent's files back; on (the default), it can.
#[tokio::test(flavor = "multi_thread")]
async fn snapshots_off_leaves_files_as_they_are_on_undo() {
    for snapshots in [false, true] {
        let mut sandbox = Sandbox::start().await;
        let config = sandbox.root().join("config/harlo/harlo.json");
        let mut settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&config).expect("config")).expect("JSON");
        settings["snapshots"] = snapshots.into();
        std::fs::write(&config, settings.to_string()).expect("config");
        sandbox.restart_server().await;

        let client = sandbox.client();
        let session = client
            .create_session(&sandbox.project_str())
            .await
            .expect("a session");
        let id = message_id();
        let script =
            r#"tool edit {"path":"a.txt","oldString":"alpha","newString":"ALPHA"} ;; say edited"#;
        client
            .prompt(
                &session.id,
                &Prompt {
                    id: id.clone(),
                    ..Prompt::new(script, Delivery::Steer)
                },
            )
            .await
            .expect("a prompt");
        wait_idle(&client, &session.id).await;
        let read = || std::fs::read_to_string(sandbox.project().join("a.txt")).expect("a.txt");
        assert_eq!(read(), "ALPHA\n");
        let staged = client
            .stage_revert(&session.id, &id, true)
            .await
            .expect("staged");
        if snapshots {
            assert_eq!(staged.files.len(), 1);
            assert_eq!(read(), "alpha\n", "put back");
        } else {
            assert!(staged.files.is_empty(), "{staged:?}");
            assert_eq!(read(), "ALPHA\n", "left as it is");
        }
    }
}

#[tokio::test]
async fn a_folders_branch_is_named() {
    let sandbox = Sandbox::start().await;
    let project = sandbox.project_str();
    let current = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&project)
        .output()
        .expect("git");
    let current = String::from_utf8_lossy(&current.stdout).trim().to_owned();
    let branch = sandbox.client().branch(&project).await.expect("the branch");
    assert_eq!(branch.as_deref(), Some(current.as_str()));
}
