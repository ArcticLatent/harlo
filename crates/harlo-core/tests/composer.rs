//! The composer's `/` and `@` menus, and what sending it does.

// Rust guideline compliant 2026-10-09

use harlo_core::composer::{
    Attached, Choice, Draft, Menu, Outgoing, Runner, at_options, insertion, outgoing,
    slash_options, trigger,
};
use harlo_core::server::{
    Agent, Attachment, Catalog, Command, Delivery, FileMatch, McpResource, Skill,
};

fn catalog() -> Catalog {
    Catalog {
        agents: vec![
            Agent {
                id: "build".into(),
                name: "Build".into(),
                description: None,
                primary: true,
                subagent: false,
                color: None,
                read_only: false,
                model: None,
            },
            Agent {
                id: "explore".into(),
                name: "Explore".into(),
                description: Some("Looks around".into()),
                primary: false,
                subagent: true,
                color: None,
                read_only: false,
                model: None,
            },
        ],
        commands: vec![
            Command {
                name: "init".into(),
                description: Some("Write AGENTS.md".into()),
                agent: None,
                subagent: false,
            },
            Command {
                name: "review".into(),
                description: None,
                agent: Some("explore".into()),
                subagent: true,
            },
        ],
        skills: vec![Skill {
            id: "browser-skill".into(),
            name: "browser".into(),
            description: Some("Drives a browser".into()),
        }],
        resources: vec![McpResource {
            server: "docs".into(),
            name: "readme".into(),
            uri: "docs://readme".into(),
            description: None,
        }],
        ..Catalog::default()
    }
}

#[test]
fn menus_open_on_the_word_at_the_cursor() {
    let slash = trigger("/rev", 0, 4).expect("a slash word");
    assert_eq!(
        (slash.menu, slash.start, slash.query.as_str()),
        (Menu::Slash, 0, "rev")
    );

    let at = trigger("look at @src/ma please", 0, 15).expect("an at word");
    assert_eq!((at.menu, at.start, at.end), (Menu::At, 8, 15));
    assert_eq!(at.query, "src/ma");

    // Multi-byte text before the word keeps byte offsets right.
    let at = trigger("é @a", 1, 5).expect("an at word");
    assert_eq!((at.line, at.start, at.query.as_str()), (1, 3, "a"));

    assert_eq!(trigger("a /path", 0, 7), None, "a slash mid-text is a path");
    assert_eq!(
        trigger("/x", 1, 2),
        None,
        "only the first line starts a command"
    );
    assert_eq!(trigger("plain", 0, 5), None);
    assert_eq!(trigger("@a b", 0, 4), None, "the cursor left the word");
    assert_eq!(trigger("@a", 0, 9), None, "a cursor past the line");
}

#[test]
fn the_slash_menu_lists_commands_compact_and_skills() {
    let labels = |query| {
        slash_options(&catalog(), query)
            .into_iter()
            .map(|o| o.label)
            .collect::<Vec<_>>()
    };
    assert_eq!(labels(""), ["/init", "/review", "/compact", "/browser"]);
    assert_eq!(labels("r"), ["/review", "/browser"], "prefix matches first");
    assert_eq!(labels("COMP"), ["/compact"]);
    assert!(labels("zzz").is_empty());
}

#[test]
fn a_command_says_which_agent_runs_it() {
    let mut catalog = catalog();
    catalog.agents[1].color = Some("#4f9dff".into());
    catalog.commands.push(Command {
        name: "ask".into(),
        description: None,
        agent: Some("oracle".into()),
        subagent: false,
    });
    let runner = |name: &str| {
        slash_options(&catalog, name)
            .into_iter()
            .find(|o| o.label == format!("/{name}"))
            .expect("the command")
            .runner
    };
    assert_eq!(runner("init"), None, "it runs like a message");
    assert_eq!(
        runner("review"),
        Some(Runner {
            agent: Some("Explore".into()),
            color: Some("#4f9dff".into()),
            subagent: true,
        }),
        "the agent by name, and its colour"
    );
    assert_eq!(
        runner("ask").and_then(|r| r.agent),
        Some("oracle".into()),
        "an agent the folder doesn't list keeps its ID"
    );
}

#[test]
fn the_at_menu_lists_subagents_resources_then_files() {
    let files = [FileMatch {
        path: "src/main.rs".into(),
        directory: false,
    }];
    let options = at_options(&catalog(), &files, "");
    let labels: Vec<&str> = options.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(labels, ["@explore", "@readme", "@src/main.rs"]);
    assert_eq!(
        insertion(&options[2].choice, &catalog()),
        "@src/main.rs ",
        "a picked file is named in the text"
    );
    let filtered = at_options(&catalog(), &[], "exp");
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].choice, Choice::Agent("explore".into()));
}

#[test]
fn files_named_like_the_query_come_before_agents_that_only_contain_it() {
    let files = [
        FileMatch {
            path: "docs/release.md".into(),
            directory: false,
        },
        FileMatch {
            path: "src/explorer.rs".into(),
            directory: false,
        },
    ];
    let labels = |query: &str| -> Vec<String> {
        at_options(&catalog(), &files, query)
            .into_iter()
            .map(|o| o.label)
            .collect()
    };
    // "re" starts the README resource and a file's own name; "explore"
    // only contains it, so the server's file list order follows.
    assert_eq!(
        labels("re"),
        [
            "@readme",
            "@docs/release.md",
            "@explore",
            "@src/explorer.rs"
        ]
    );
}

#[test]
fn a_leading_slash_runs_a_command_compacts_or_loads_a_skill() {
    let draft = Draft::default();
    let send = |text: &str| outgoing(text, &draft, &catalog(), "/p", Delivery::Queue);
    assert_eq!(
        send("/review the last commit"),
        Outgoing::Command {
            name: "review".into(),
            arguments: "the last commit".into()
        }
    );
    assert_eq!(send("/compact"), Outgoing::Compact);
    let Outgoing::Prompt(prompt) = send("/browser open the docs") else {
        panic!("a skill is a prompt");
    };
    assert_eq!(prompt.skills, ["browser-skill"]);
    assert_eq!(prompt.text, "open the docs");
    let Outgoing::Prompt(prompt) = send("/etc/hosts is broken") else {
        panic!("an unknown slash word is plain text");
    };
    assert_eq!(prompt.text, "/etc/hosts is broken");
    assert!(prompt.skills.is_empty());
}

#[test]
fn mentions_and_attachments_go_with_the_prompt() {
    let mut draft = Draft::default();
    draft.mention(
        "@src/main.rs".into(),
        Choice::File {
            path: "src/main.rs".into(),
            directory: false,
        },
    );
    draft.mention("@explore".into(), Choice::Agent("explore".into()));
    draft.mention(
        "@gone.rs".into(),
        Choice::File {
            path: "gone.rs".into(),
            directory: false,
        },
    );
    draft.mention(
        "@readme".into(),
        Choice::Resource {
            uri: "docs://readme".into(),
            name: "readme".into(),
        },
    );
    draft.attached.push(Attached {
        name: "shot.png".into(),
        attachment: Attachment::inline("image/png", b"png", "shot.png"),
    });

    let Outgoing::Prompt(prompt) = outgoing(
        "  ask @explore about é @src/main.rs and @readme ",
        &draft,
        &catalog(),
        "/home/me/project/",
        Delivery::Steer,
    ) else {
        panic!("a prompt");
    };
    assert_eq!(prompt.text, "ask @explore about é @src/main.rs and @readme");
    assert_eq!(prompt.delivery, Delivery::Steer);
    assert_eq!(prompt.agents, ["explore"]);
    let files: Vec<(&str, Option<&str>)> = prompt
        .files
        .iter()
        .map(|f| (f.uri.as_str(), f.name.as_deref()))
        .collect();
    assert_eq!(
        files,
        [
            ("file:///home/me/project/src/main.rs", Some("src/main.rs")),
            ("docs://readme", Some("readme")),
            ("data:image/png;base64,cG5n", Some("shot.png")),
        ],
        "mentions edited out of the text are dropped"
    );
    let mention = prompt.files[0].mention.as_ref().expect("tied to its text");
    // "ask @explore about é " is 21 UTF-16 units.
    assert_eq!((mention.start, mention.end), (21, 33));
}
