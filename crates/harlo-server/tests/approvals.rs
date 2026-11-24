//! Approval replies are decided once. The same reply sent twice at the same
//! moment (a phone retrying, or a phone and the desktop both answering)
//! succeeds both times and decides once; a different reply afterwards is
//! refused as settled. The edit runs once and replay holds one ask and one
//! decision, in order.

// Rust guideline compliant 2026-10-09

mod support;

use harlo_client::{Delivery, Effect, EventKind, PermissionReply, Prompt, Rule};
use support::{Sandbox, replay, until};

const EDIT: &str =
    r#"tool edit {"path":"a.txt","oldString":"alpha","newString":"alpha beta"} ;; say Done."#;

#[tokio::test]
async fn the_same_reply_twice_decides_once() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let mut events = client.events().await.expect("events");
    let session = client
        .create_session(project.to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .set_permissions(&session.id, &[Rule::any("edit", Effect::Ask)])
        .await
        .expect("careful");
    client
        .prompt(&session.id, &Prompt::new(EDIT, Delivery::Queue))
        .await
        .expect("prompt");
    let asked = until(&mut events, |kind| match kind {
        EventKind::PermissionAsked(p) => Some(*p),
        _ => None,
    })
    .await;

    let (first, second) = tokio::join!(
        client.reply_permission(&session.id, &asked.id, PermissionReply::Once, None),
        client.reply_permission(&session.id, &asked.id, PermissionReply::Once, None),
    );
    first.expect("first reply");
    second.expect("the same reply again");
    let conflicting = client
        .reply_permission(&session.id, &asked.id, PermissionReply::Reject, None)
        .await
        .expect_err("already decided");
    assert!(conflicting.is_settled(), "{conflicting}");
    let unknown = client
        .reply_permission(&session.id, "per_nothing", PermissionReply::Once, None)
        .await
        .expect_err("no such request");
    assert!(unknown.is_settled(), "{unknown}");

    until(&mut events, |kind| match kind {
        EventKind::Done { .. } => Some(()),
        _ => None,
    })
    .await;
    assert_eq!(
        std::fs::read_to_string(project.join("a.txt")).expect("read"),
        "alpha beta\n",
        "edited once"
    );

    let kinds: Vec<String> = replay(&sandbox, 0)
        .await
        .into_iter()
        .map(|(_, k)| k)
        .collect();
    let count = |k: &str| kinds.iter().filter(|x| *x == k).count();
    assert_eq!(count("permission.asked"), 1, "{kinds:?}");
    assert_eq!(count("permission.replied"), 1, "{kinds:?}");
    assert_eq!(count("session.tool.success"), 1, "{kinds:?}");
    let at = |k: &str| kinds.iter().position(|x| x == k).expect(k);
    assert!(at("permission.asked") < at("permission.replied"));
    assert!(at("permission.replied") < at("session.tool.success"));
}
