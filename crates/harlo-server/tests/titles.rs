//! A new session is named from its first message: at once with the
//! message's first line, then with the title the model gives.

// Rust guideline compliant 2026-10-09

mod support;

use std::time::Duration;

use harlo_engine::generate::title_prompt;
use opencode_client::{Delivery, Prompt};
use support::Sandbox;

#[tokio::test(flavor = "multi_thread")]
async fn the_model_names_a_new_session() {
    let sandbox = Sandbox::start().await;
    let first = "Fix the login bug in the auth module, please";
    let scripts = serde_json::json!({ title_prompt(first): "say \"Login bug in auth.\"" });
    std::fs::write(sandbox.dir.path().join("scripts.json"), scripts.to_string()).expect("scripts");
    let client = sandbox.client();
    let session = client
        .create_session(sandbox.project().to_str().expect("utf-8"))
        .await
        .expect("session");
    client
        .prompt(&session.id, &Prompt::new(first, Delivery::Queue))
        .await
        .expect("prompt");
    let mut title = None;
    for _ in 0..100 {
        title = client.session(&session.id).await.expect("session").title;
        if title.as_deref() == Some("Login bug in auth") {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("never named by the model: {title:?}");
}
