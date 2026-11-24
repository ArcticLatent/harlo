//! M4 endpoints against a private server: a session's own permission rules,
//! what an approval carries, and typed questions.
//!
//! Each test runs its own `harlo-server` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

mod support;

use harlo_client::{Answer, Delivery, Effect, EventKind, FieldKind, PermissionReply, Prompt, Rule};
use support::{Sandbox, WAIT, wait_for, wait_idle};

/// A session's own rules change what asks first, and others hear of it.
#[tokio::test(flavor = "multi_thread")]
async fn a_sessions_rules_decide_what_asks_first() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    assert!(session.permissions.is_empty(), "no rules of its own yet");

    let mut events = client.events().await.expect("events");
    let trust = [Rule::any("*", Effect::Allow)];
    client
        .set_permissions(&session.id, &trust)
        .await
        .expect("rules");
    let heard = tokio::time::timeout(WAIT, async {
        while let Some(event) = events.next().await {
            if let EventKind::SessionPermissions { session_id, rules } = event.expect("event").kind
                && session_id == session.id
            {
                return rules;
            }
        }
        panic!("event stream ended");
    })
    .await
    .expect("the change was announced");
    assert_eq!(heard, trust);
    let read = client.session(&session.id).await.expect("session");
    assert_eq!(read.permissions, trust);

    // A command that asks by default now just runs.
    let script = r#"tool shell {"command":"echo hi >> trusted.txt"} ;; say ran it"#;
    client
        .prompt(&session.id, &Prompt::new(script, Delivery::Steer))
        .await
        .expect("prompt");
    wait_idle(&client, &session.id).await;
    assert_eq!(
        std::fs::read_to_string(sandbox.project().join("trusted.txt")).expect("trusted.txt"),
        "hi\n"
    );

    // No rules of its own: the agent's apply again.
    client
        .set_permissions(&session.id, &[])
        .await
        .expect("rules");
    let read = client.session(&session.id).await.expect("session");
    assert!(read.permissions.is_empty());
}

/// An edit's approval carries the patch, and a refusal can tell the agent why.
#[tokio::test(flavor = "multi_thread")]
async fn an_edit_approval_shows_its_patch_and_can_be_refused_with_a_reason() {
    let sandbox = Sandbox::start().await;
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
    assert_eq!(asked.action, "edit");
    assert_eq!(asked.files.len(), 1, "{asked:#?}");
    assert_eq!(asked.files[0].file, "e.txt");
    assert!(asked.files[0].patch.contains("+three"), "{asked:#?}");

    client
        .reply_permission(
            &session.id,
            &asked.id,
            PermissionReply::Reject,
            Some("leave e.txt alone"),
        )
        .await
        .expect("reply");
    wait_idle(&client, &session.id).await;
    assert_eq!(
        std::fs::read_to_string(sandbox.project().join("e.txt")).expect("e.txt"),
        "one\ntwo\n",
        "refused, so unchanged"
    );
    assert!(
        client
            .permissions(&session.id)
            .await
            .expect("list")
            .is_empty()
    );
}

/// A question's fields arrive typed; answers go back typed, or it's dismissed.
#[tokio::test(flavor = "multi_thread")]
async fn questions_arrive_typed_and_can_be_answered_or_dismissed() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("session");
    let question = serde_json::json!({ "questions": [
        { "question": "Which colour?", "header": "Colour",
          "options": [{ "label": "Red", "description": "warm" }, { "label": "Blue", "description": "cool" }] },
        { "question": "Which extras?", "header": "Extras", "multiple": true,
          "options": [{ "label": "Tests", "description": "add tests" }, { "label": "Docs", "description": "write docs" }] }
    ]});
    let ask = |reply: &str| format!("tool question {question} ;; say {reply}");
    client
        .prompt(&session.id, &Prompt::new(ask("answered"), Delivery::Steer))
        .await
        .expect("prompt");
    let form = wait_for("a question", || async {
        client.forms(&session.id).await.ok()?.into_iter().next()
    })
    .await;
    assert_eq!(form.fields.len(), 2, "{form:#?}");
    let colour = &form.fields[0];
    assert_eq!(colour.title.as_deref(), Some("Colour"));
    assert_eq!(colour.description.as_deref(), Some("Which colour?"));
    let FieldKind::Text {
        options, custom, ..
    } = &colour.kind
    else {
        panic!("a single choice: {colour:#?}");
    };
    assert_eq!(options[1].label, "Blue");
    assert_eq!(options[1].description.as_deref(), Some("cool"));
    assert!(*custom, "other answers are accepted");
    assert!(
        matches!(&form.fields[1].kind, FieldKind::Multiselect { options, .. } if options.len() == 2),
        "{form:#?}"
    );

    client
        .reply_form(
            &session.id,
            &form.id,
            &[
                (colour.key.clone(), Answer::Text("Blue".into())),
                (
                    form.fields[1].key.clone(),
                    Answer::List(vec!["Tests".into(), "Docs".into()]),
                ),
            ],
        )
        .await
        .expect("answer");
    wait_idle(&client, &session.id).await;
    assert!(client.forms(&session.id).await.expect("forms").is_empty());

    // Asked again, and dismissed this time.
    client
        .prompt(&session.id, &Prompt::new(ask("dismissed"), Delivery::Steer))
        .await
        .expect("prompt");
    let form = wait_for("another question", || async {
        client.forms(&session.id).await.ok()?.into_iter().next()
    })
    .await;
    client
        .cancel_form(&session.id, &form.id)
        .await
        .expect("dismiss");
    wait_idle(&client, &session.id).await;
    assert!(client.forms(&session.id).await.expect("forms").is_empty());
    let stale = client
        .cancel_form(&session.id, &form.id)
        .await
        .expect_err("already dismissed");
    assert!(stale.is_settled(), "{stale}");
}
