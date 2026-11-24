//! Signing in to a catalog provider with a key, answering the form a
//! provider needs, and removing the credential: the provider is usable
//! while it's signed in.

// Rust guideline compliant 2026-10-09

mod support;

use opencode_client::{Answer, FieldKind, SignInMethod};
use support::Sandbox;

#[tokio::test(flavor = "multi_thread")]
async fn a_provider_signs_in_with_a_key_and_is_removed() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let project = project.to_str().expect("utf-8");
    let all = client.integrations().await.expect("integrations");
    assert!(all.len() > 100, "the catalog's providers are listed");
    let ai302 = all.iter().find(|i| i.id == "302ai").expect("302.AI");
    assert_eq!(ai302.name, "302.AI");
    assert!(matches!(ai302.methods[0], SignInMethod::Key { .. }));
    assert!(
        ai302
            .methods
            .iter()
            .any(|m| matches!(m, SignInMethod::Env { names } if names == &["302AI_API_KEY"]))
    );
    let azure = all.iter().find(|i| i.id == "azure").expect("Azure");
    let form = azure.methods[0].form();
    assert_eq!(form[0].key, "resourceName");
    assert!(form[0].required);
    assert!(matches!(form[0].kind, FieldKind::Text { .. }));
    let before = client.providers(project).await.expect("providers");
    assert!(!before.iter().any(|p| p.id == "302ai"));

    client
        .connect_key("302ai", "sk-scratch", &[])
        .await
        .expect("signed in");
    let signed = client
        .integration("302ai")
        .await
        .expect("an answer")
        .expect("302.AI");
    let connection = &signed.connections[0];
    assert_eq!(connection.method.as_deref(), Some("key"));
    let providers = client.providers(project).await.expect("providers");
    assert!(providers.iter().any(|p| p.id == "302ai"), "{providers:?}");

    let refused = client
        .connect_key("azure", "k", &[])
        .await
        .expect_err("needs its resource name");
    assert_eq!(
        refused.server_message(),
        Some("Missing required form field: resourceName")
    );
    client
        .connect_key(
            "azure",
            "k",
            &[("resourceName".into(), Answer::Text("scratch".into()))],
        )
        .await
        .expect("signed in");

    let credential = connection.credential.clone().expect("a credential");
    client
        .remove_credential(&credential)
        .await
        .expect("removed");
    let removed = client
        .integration("302ai")
        .await
        .expect("an answer")
        .expect("302.AI");
    assert!(removed.connections.is_empty());
    let after = client.providers(project).await.expect("providers");
    assert!(!after.iter().any(|p| p.id == "302ai"));
    assert_eq!(
        client
            .integration("no-such-service")
            .await
            .expect("an answer"),
        None
    );
}
