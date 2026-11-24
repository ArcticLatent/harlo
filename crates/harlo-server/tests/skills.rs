//! Commands and skills: the folder's commands and skills are listed, a
//! command becomes a prompt with its arguments filled in, a skill attached
//! to a prompt reaches the model, and the model can load one with the
//! skill tool.

// Rust guideline compliant 2026-10-09

mod support;

use harlo_client::{Delivery, EventKind, Message, Part, Prompt};
use support::{Sandbox, until};

async fn done(events: &mut harlo_client::Events) {
    until(events, |kind| match kind {
        EventKind::Done { .. } => Some(()),
        _ => None,
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn commands_and_skills_reach_the_model() {
    let sandbox = Sandbox::start().await;
    let client = sandbox.client();
    let project = sandbox.project();
    let folder = project.to_str().expect("utf-8").to_owned();
    let skill_dir = project.join(".opencode/skills/release-notes");
    std::fs::create_dir_all(&skill_dir).expect("skill folder");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: release-notes\ndescription: Writes release notes.\n---\n\nKeep them short.\n",
    )
    .expect("skill file");
    let commands_dir = project.join(".opencode/commands");
    std::fs::create_dir_all(&commands_dir).expect("commands folder");
    std::fs::write(
        commands_dir.join("greet.md"),
        "---\ndescription: Says a greeting\n---\nsay Hello $1 from !`echo the shell`\n",
    )
    .expect("command file");

    let commands = client.commands(&folder).await.expect("commands");
    let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["greet", "init", "review", "goal"]);
    assert!(commands.iter().all(|c| c.description.is_some()));
    let skills = client.skills(&folder).await.expect("skills");
    assert_eq!(
        skills.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["release-notes"]
    );

    let mut events = client.events().await.expect("events");
    let session = client.create_session(&folder).await.expect("session");
    client
        .run_command(&session.id, "greet", "Bob", Delivery::Queue)
        .await
        .expect("the command");
    done(&mut events).await;

    let mut prompt = Prompt::new("say noted", Delivery::Queue);
    prompt.skills = vec!["release-notes".to_owned()];
    client.prompt(&session.id, &prompt).await.expect("sent");
    done(&mut events).await;

    client
        .prompt(
            &session.id,
            &Prompt::new(
                r#"tool skill {"id":"release-notes"} ;; say loaded"#,
                Delivery::Queue,
            ),
        )
        .await
        .expect("sent");
    done(&mut events).await;

    let messages = client.messages(&session.id).await.expect("history");
    let texts: Vec<&str> = messages
        .iter()
        .filter_map(|m| match m {
            Message::Assistant { parts, .. } => Some(parts),
            _ => None,
        })
        .flatten()
        .filter_map(|p| match p {
            Part::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["Hello Bob from the shell", "ok.", "loaded"]);
    let attached = messages.iter().find_map(|m| match m {
        Message::User { skills, .. } if !skills.is_empty() => Some(skills.clone()),
        _ => None,
    });
    assert_eq!(attached, Some(vec!["release-notes".to_owned()]));

    let requests = support::turn_requests(sandbox.dir.path());
    let attached_request = requests[1]["messages"].to_string();
    assert!(
        attached_request.contains("Keep them short."),
        "{attached_request}"
    );
    let loaded = requests.last().expect("a request");
    let tool = loaded["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .find(|m| m["role"] == "tool")
        .expect("the tool's result");
    assert!(
        tool["content"].to_string().contains("Keep them short."),
        "{tool}"
    );
    let offered = loaded["tools"].to_string();
    assert!(
        offered.contains("release-notes: Writes release notes."),
        "{offered}"
    );
}
