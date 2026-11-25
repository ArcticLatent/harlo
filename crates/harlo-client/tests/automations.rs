//! The server's automations against a private server: refused while
//! `automations` is off, then created, listed, paused, resumed, run and
//! deleted.
//!
//! Each test runs its own `harlo-server` with scratch `HOME`/`XDG_*`
//! folders and the scripted model, never the shared service.

// Rust guideline compliant 2026-10-09

mod support;

use harlo_client::{
    AutomationAction, AutomationStatus, AutomationTrigger, Effect, NewAction, NewAutomation, Rule,
};
use support::{Sandbox, wait_for};

/// Turns automations on or off in the sandbox's global config; the server
/// reads the change without restarting.
fn allow_automations(sandbox: &Sandbox, on: bool) {
    let config = sandbox.root().join("config/harlo/harlo.json");
    let mut settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config).expect("config")).expect("JSON");
    settings["automations"] = serde_json::json!(on);
    std::fs::write(&config, settings.to_string()).expect("config");
}

#[tokio::test(flavor = "multi_thread")]
async fn automations_are_created_paused_resumed_run_and_deleted() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("a session");
    let new = |trigger| NewAutomation {
        name: "Weekday check".into(),
        session_id: Some(session.id.clone()),
        directory: None,
        trigger,
        action: NewAction::Prompt("Check the build.".into()),
    };

    // On unless the config turns them off.
    allow_automations(&sandbox, false);
    let refused = wait_for("automations off", || async {
        client
            .create_automation(&new(AutomationTrigger::Cron("0 9 * * 1-5".into())))
            .await
            .err()
    })
    .await;
    assert!(
        refused.to_string().contains("Automations are disabled"),
        "{refused}"
    );
    assert!(client.automations().await.expect("the list").is_empty());

    allow_automations(&sandbox, true);
    let weekday = wait_for("automations on", || async {
        client
            .create_automation(&new(AutomationTrigger::Cron("0 9 * * 1-5".into())))
            .await
            .ok()
    })
    .await;
    assert_eq!(weekday.status, AutomationStatus::Active);
    assert_eq!(weekday.directory, sandbox.project_str());
    assert!(weekday.next_due_at.is_some());
    assert_eq!(
        weekday.action,
        AutomationAction::Prompt {
            prompt: "Check the build.".into(),
            session: Some(session.id.clone()),
            subagent: None,
        }
    );

    // The server says what's wrong with a time it can't read.
    for (trigger, said) in [
        (AutomationTrigger::Cron("nope".into()), "cron"),
        (
            AutomationTrigger::Every("10 seconds".into()),
            "at least 1 minute",
        ),
        (AutomationTrigger::At("tomorrow-ish".into()), "Invalid time"),
    ] {
        let why = client
            .create_automation(&new(trigger))
            .await
            .expect_err("refused");
        assert!(why.to_string().contains(said), "{why}");
    }

    let paused = client.pause_automation(&weekday.id).await.expect("paused");
    assert_eq!(paused.status, AutomationStatus::Paused);
    let resumed = client
        .resume_automation(&weekday.id)
        .await
        .expect("resumed");
    assert_eq!(resumed.status, AutomationStatus::Active);

    // A one-off a few seconds away sends its prompt to the session.
    let soon = client
        .create_automation(&new(AutomationTrigger::At("in 2 seconds".into())))
        .await
        .expect("created");
    let ran = wait_for("the one-off to run", || async {
        client
            .automations()
            .await
            .ok()?
            .into_iter()
            // (It is marked done as it starts, so it can't start twice; how
            // it went is kept when it ends.)
            .find(|a| {
                a.id == soon.id && a.status == AutomationStatus::Done && a.last_outcome.is_some()
            })
    })
    .await;
    assert_eq!(ran.run_count, 1);
    assert_eq!(ran.last_outcome.as_deref(), Some("fired"));
    // The run is done once the prompt is sent; the model gets it next.
    wait_for("the prompt to reach the model", || async {
        (sandbox.model_requests() >= 1).then_some(())
    })
    .await;

    client
        .delete_automation(&weekday.id)
        .await
        .expect("deleted");
    let left: Vec<_> = client
        .automations()
        .await
        .expect("the list")
        .into_iter()
        .map(|a| a.id)
        .collect();
    assert_eq!(left, [soon.id]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shell_command_and_code_run_with_no_model() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    // The server checks their permissions in the session they belong to.
    let session = client
        .create_session(&sandbox.project_str())
        .await
        .expect("a session");
    let soon = |name: &str, action| NewAutomation {
        name: name.into(),
        session_id: Some(session.id.clone()),
        directory: None,
        trigger: AutomationTrigger::At("in 2 seconds".into()),
        action,
    };
    // Created through the API, nobody is there to approve the command
    // or the code: the session they belong to allows them beforehand.
    let refused = client
        .create_automation(&soon("Where", NewAction::Shell("pwd".into())))
        .await
        .expect_err("not allowed yet");
    assert!(refused.to_string().contains("shell"), "{refused}");
    client
        .set_permissions(
            &session.id,
            &[
                Rule {
                    action: "shell".into(),
                    resource: "pwd".into(),
                    effect: Effect::Allow,
                },
                Rule::any("execute", Effect::Allow),
            ],
        )
        .await
        .expect("allowed");
    let shell = client
        .create_automation(&soon("Where", NewAction::Shell("pwd".into())))
        .await
        .expect("created");
    assert_eq!(shell.session_id.as_deref(), Some(session.id.as_str()));
    assert_eq!(shell.directory, sandbox.project_str());
    assert_eq!(
        shell.action,
        AutomationAction::Shell {
            command: "pwd".into()
        }
    );
    let code = client
        .create_automation(&soon("Count", NewAction::Code("return;".into())))
        .await
        .expect("created");
    assert_eq!(
        code.action,
        AutomationAction::Code {
            code: "return;".into()
        }
    );

    let done = |id: String| {
        let client = client.clone();
        async move {
            client
                .automations()
                .await
                .ok()?
                .into_iter()
                // Its outcome is written just after it's marked done.
                .find(|a| {
                    a.id == id && a.status == AutomationStatus::Done && a.last_outcome.is_some()
                })
        }
    };
    let ran = wait_for("the command to run", || done(shell.id.clone())).await;
    assert_eq!(ran.last_outcome.as_deref(), Some("fired"));
    let output = ran.last_output.unwrap_or_default();
    assert!(output.contains("/project"), "{output}");
    let ran = wait_for("the code to run", || done(code.id.clone())).await;
    assert_eq!(ran.last_outcome.as_deref(), Some("fired"));
    // Neither asked the model anything.
    assert_eq!(sandbox.model_requests(), 0);
}
