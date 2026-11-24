//! Settings → Providers' providers against a private `harlo-server` with
//! scratch `HOME`/`XDG_*` folders: signing a provider in with a key,
//! removing it, a form a sign-in needs answered first, and choosing among
//! the ways to sign in. Never the shared service, and no sign-in reaches a
//! real provider: keys aren't checked when stored, and no browser sign-in
//! is started.

// Rust guideline compliant 2026-10-09

#[path = "../../harlo-client/tests/support/mod.rs"]
mod server;
mod support;

use std::sync::Arc;

use i_slint_backend_testing::AccessibleRole;

use harlo::app::{Load, Message, ProviderEvent, SheetEvent};
use harlo::model::Picker;
use harlo_core::server::{
    AttemptStatus, Connected, Integration, OAuthAttempt, SignInMethod, Update,
};
use server::{Sandbox, WAIT};
use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// Settings → Providers, connected to `sandbox` once it lists its
/// integrations (it loads them after it starts).
fn models(sandbox: &Sandbox, tasks: &mut Tasks) -> Ui {
    let client = sandbox.client();
    let listed = tasks.block_on(async {
        tokio::time::timeout(WAIT, async {
            // The catalog's come first, then those from the config.
            while !client
                .integrations()
                .await
                .is_ok_and(|l| l.len() > 100 && l.iter().any(|i| i.id == "fake"))
            {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        })
        .await
    });
    assert!(listed.is_ok(), "the server lists its integrations");
    let mut app = demo::new_app();
    let _ = app.update(Message::Tick(demo::now()));
    let _ = app.update(Message::Server(Update::Connected(Arc::new(Connected {
        client: sandbox.client(),
        server: "Harlo".into(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        snapshot: demo::snapshot(),
    }))));
    app.new_folder = Some(sandbox.project_str());
    let ui = ui(app);
    ui.press("Settings");
    ui.press("Providers");
    assert!(
        tasks.until(&ui, WAIT, |ui| {
            matches!(&ui.app().sheets.providers.list, Load::Loaded(l) if l.len() > 100)
                && ui
                    .find("Fake")
                    .iter()
                    .any(|e| e.accessible_role() == Some(AccessibleRole::Button))
        }),
        "the providers"
    );
    ui
}

/// Opens the sign-in page of the provider named `name` from the menu.
fn add(ui: &Ui, filter: &str, name: &str) {
    // At the end of the page, after the popular ones.
    if ui.find("Show more providers").is_empty() {
        ui.wheel("Popular", -10);
    }
    ui.press("Show more providers");
    assert_eq!(ui.app().picker, Some(Picker::Provider));
    ui.fill("Filter providers", filter);
    ui.press(name);
    assert_eq!(ui.app().picker, None);
    // The page starts at its top as the next frame draws.
    ui.wait(50);
}

#[test]
fn a_provider_signs_in_with_a_key_and_is_removed() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let ui = models(&sandbox, &mut tasks);
    assert_eq!(
        ui.button("Fake").accessible_description().as_deref(),
        Some("No sign-in needed"),
        "the scripted model's provider"
    );

    // The best-known ones not yet connected, to connect in one press.
    assert!(ui.shows("Popular"));
    assert!(ui.shows("Recommended"), "OpenCode Go");
    for name in ["OpenAI", "Anthropic", "OpenCode Go", "OpenRouter", "Google"] {
        assert!(
            !ui.find(&format!("Connect: {name}")).is_empty(),
            "{name} is popular"
        );
    }
    assert!(
        ui.find("Connect: GitHub Copilot").is_empty(),
        "no sign-in for it"
    );
    ui.press("Connect: Anthropic");
    assert_eq!(
        ui.app().sheets.providers.shown.as_deref(),
        Some("anthropic")
    );
    // Its own pages: where keys are made, and its prices.
    assert!(
        ui.shows("Claude models, with an API key. A Claude plan connects through Claude Code.")
    );
    assert!(ui.shows("platform.claude.com"));
    assert!(!ui.find("Get a key ↗").is_empty());
    assert!(!ui.find("Prices ↗").is_empty());
    ui.press("All providers");

    add(&ui, "302", "302.AI");
    assert!(ui.shows("Sign in with a key"));
    assert!(ui.shows("Or set 302AI_API_KEY where Harlo's server runs."));
    ui.press("Sign in");
    assert!(ui.shows("Paste the key first."));
    ui.fill("API key", "sk-scratch");
    ui.press("Sign in");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Signed in with a key")),
        "signed in: {:?}",
        ui.app().sheets.providers.error
    );
    assert!(ui.shows("Signed in. Its models are in the model menus."));
    assert!(ui.app().sheets.providers.key.is_empty(), "the key is gone");

    ui.press("All providers");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("302.AI")),
        "in the list"
    );
    ui.press("302.AI");
    ui.press("Remove");
    assert!(ui.shows("Remove it from the server?"));
    ui.press("Remove for good");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Removed.")
            && !ui.shows("Signed in with a key")),
        "removed: {:?}",
        ui.app().sheets.providers.error
    );
    let left = tasks
        .block_on(sandbox.client().integration("302ai"))
        .expect("an answer")
        .expect("302.AI");
    assert!(left.connections.is_empty());
}

#[test]
fn a_form_is_answered_before_signing_in() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let ui = models(&sandbox, &mut tasks);

    add(&ui, "azure", "Azure");
    ui.fill("API key", "k");
    ui.press("Sign in");
    assert!(
        ui.shows("Enter Azure Resource Name: Needs an answer"),
        "{:?}",
        ui.app().sheets.providers.error
    );
    ui.fill("Enter Azure Resource Name", "scratch");
    ui.press("Sign in");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Signed in with a key")),
        "signed in: {:?}",
        ui.app().sheets.providers.error
    );
}

#[test]
fn openrouter_signs_in_in_the_browser_or_with_a_key() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let ui = models(&sandbox, &mut tasks);
    ui.press("Connect: OpenRouter");
    assert!(ui.shows("Ways to sign in"));
    assert!(ui.shows("Sign in with OpenRouter"));
    assert!(!ui.find("API key").is_empty(), "a key first");
    assert!(ui.shows("openrouter.ai"), "its key page");
}

#[test]
fn opencode_go_says_where_its_key_comes_from() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let ui = models(&sandbox, &mut tasks);
    ui.press("Connect: OpenCode Go");
    assert!(ui.shows(
        "A key from an OpenCode Go subscription, made in the OpenCode Console. Each model \
         has its own limits, by the 5 hours, the week and the month."
    ));
}

#[test]
fn a_provider_with_several_ways_lets_one_be_chosen() {
    let mut tasks = Tasks::new();
    let sandbox = tasks.block_on(Sandbox::start());
    let ui = models(&sandbox, &mut tasks);

    add(&ui, "openai", "OpenAI");
    assert!(ui.shows("Ways to sign in"));
    assert!(ui.shows("Sign in with ChatGPT"));
    assert!(!ui.find("API key").is_empty(), "a key first");
    ui.press("Use this");
    let page = &ui.app().sheets.providers;
    assert!(matches!(
        page.chosen(),
        Some(SignInMethod::OAuth { label, .. }) if label == "Sign in with ChatGPT"
    ));
    assert!(
        ui.find("API key")
            .iter()
            .all(|e| e.accessible_role() != Some(AccessibleRole::TextInput)),
        "no key field"
    );
    assert!(ui.shows("Open the sign-in page"));
}

/// `event` as the app's message.
fn provider(event: ProviderEvent) -> Message {
    Message::Sheet(SheetEvent::Provider(event))
}

#[test]
fn a_browser_sign_in_shows_its_page_takes_a_code_and_ends() {
    // No server: the sign-in's steps are fed in, and no page opens.
    let ui = ui(demo::new_app());
    ui.press("Settings");
    ui.press("Providers");
    let service = Integration {
        id: "scratch".into(),
        name: "Scratch AI".into(),
        methods: vec![
            SignInMethod::OAuth {
                id: "browser".into(),
                label: "Log in with Scratch".into(),
                form: Vec::new(),
            },
            SignInMethod::Command {
                id: "cli".into(),
                label: "Scratch CLI".into(),
                command: vec!["scratch".into(), "login".into()],
            },
        ],
        connections: Vec::new(),
    };
    ui.send(provider(ProviderEvent::Loaded(Ok(vec![service]))));
    ui.send(provider(ProviderEvent::Open("scratch".into())));
    assert!(ui.shows("Ways to sign in"));
    assert!(ui.shows("Open the sign-in page"));

    ui.send(provider(ProviderEvent::OAuthStarted(Ok(OAuthAttempt {
        id: "att_1".into(),
        url: "https://scratch.invalid/login".into(),
        instructions: "Copy the code the page shows.".into(),
        needs_code: true,
        paste: false,
        expires: None,
    }))));
    assert!(ui.shows("Copy the code the page shows."));
    assert!(ui.shows("https://scratch.invalid/login"));
    assert!(ui.shows("Waiting for the sign-in…"));
    ui.press("Finish");
    assert!(ui.shows("Paste what the page gave you first."));
    ui.fill("Code", "1234");
    assert_eq!(ui.app().sheets.providers.code, "1234");

    ui.send(provider(ProviderEvent::Status(AttemptStatus::Failed(
        "The code expired.".into(),
    ))));
    assert!(ui.shows("The code expired."));
    assert!(ui.app().sheets.providers.attempt.is_none());
    assert!(ui.shows("Open the sign-in page"), "it can start again");

    // A program's words show as it runs; Cancel stops it.
    ui.press("Use this");
    ui.send(provider(ProviderEvent::CommandStarted(Ok("att_2".into()))));
    ui.send(provider(ProviderEvent::Status(AttemptStatus::Pending(
        Some("Enter ABCD-1234 at scratch.invalid/device".into()),
    ))));
    assert!(ui.shows("Enter ABCD-1234 at scratch.invalid/device"));
    ui.press("Cancel");
    assert!(ui.app().sheets.providers.attempt.is_none());
    assert!(ui.shows("Run it"));

    ui.send(provider(ProviderEvent::Status(AttemptStatus::Complete)));
    assert!(ui.shows("Signed in. Its models are in the model menus."));
}
