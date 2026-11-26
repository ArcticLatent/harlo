//! The workspace reducer: snapshots, live events and the sidebar it builds.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use harlo_client::{
    Delivery, Effect, EventKind, Form, InboxItem, ModelRef, Permission, Project, Revert, Rule,
    Session, Tokens,
};
use harlo_core::workspace::{Fetch, Fetched, Snapshot, Workspace};

/// "Now" in the tests: 2026-11-21, in milliseconds.
const NOW_MS: f64 = 1_795_000_000_000.0;
const HOUR_MS: f64 = 3_600_000.0;

fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_795_000_000)
}

fn project(id: &str, directory: &str) -> Project {
    Project {
        id: id.into(),
        directory: directory.into(),
        name: None,
        git: true,
        color: None,
        icon: None,
        start: None,
    }
}

fn session(id: &str, project: &str, directory: &str, hours_ago: f64) -> Session {
    Session {
        id: id.into(),
        title: Some(format!("title {id}")),
        directory: directory.into(),
        project_id: project.into(),
        parent_id: None,
        agent: None,
        model: Some(ModelRef {
            provider_id: "anthropic".into(),
            id: "claude".into(),
            variant: None,
        }),
        created: NOW_MS - hours_ago * HOUR_MS,
        updated: NOW_MS - hours_ago * HOUR_MS,
        archived: None,
        idle: None,
        outcome: None,
        revert: None,
        permissions: Vec::new(),
        cost: 0.0,
        tokens: Tokens::default(),
    }
}

fn snapshot() -> Snapshot {
    Snapshot {
        projects: vec![
            project("p_app", "/code/app"),
            project("p_web", "/code/web"),
            project("p_idle", "/code/idle"),
        ],
        // As `server::load` returns: every git project's folders, main checkout included.
        worktrees: HashMap::from([
            (
                "p_app".to_owned(),
                vec!["/code/app".to_owned(), "/wt/app/feature".to_owned()],
            ),
            ("p_web".to_owned(), vec!["/code/web".to_owned()]),
            ("p_idle".to_owned(), vec!["/code/idle".to_owned()]),
        ]),
        sessions: vec![
            session("s1", "p_app", "/code/app", 5.0),
            session("s2", "p_app", "/code/app/sub", 1.0),
            session("s3", "p_app", "/wt/app/feature", 2.0),
            session("s4", "p_web", "/code/web", 0.5),
            Session {
                parent_id: Some("s1".into()),
                ..session("child", "p_app", "/code/app", 0.1)
            },
        ],
        active: vec!["s3".into()],
        permissions: Vec::new(),
        forms: Vec::new(),
    }
}

fn ids(list: &[harlo_core::sessions::SessionSummary]) -> Vec<&str> {
    list.iter().map(|s| s.id.as_str()).collect()
}

#[test]
fn a_snapshot_groups_sessions_by_project_and_worktree() {
    let mut ws = Workspace::new(vec!["/code/idle".into()]);
    ws.resync(snapshot());
    let sidebar = ws.sidebar(now());

    // Saved projects first, then projects with sessions, newest first.
    let names: Vec<&str> = sidebar.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["idle", "web", "app"]);
    assert_eq!(sidebar[0].total(), 0);

    let app = &sidebar[2];
    assert_eq!(
        ids(&app.sessions),
        ["s2", "s1"],
        "subfolders belong to the main folder; newest first"
    );
    assert_eq!(app.worktrees.len(), 1);
    assert_eq!(app.worktrees[0].name, "feature");
    assert_eq!(ids(&app.worktrees[0].sessions), ["s3"]);
    assert_eq!(
        (app.total(), app.working()),
        (3, 1),
        "the subagent session isn't counted as the project's own"
    );
    let s1 = &app.sessions[1];
    assert_eq!(ids(&s1.children), ["child"], "it sits under its parent");

    let s2 = &app.sessions[0];
    assert_eq!(s2.maker.as_deref(), Some("anthropic"));
    assert_eq!(s2.age, Duration::from_secs(3_600));
    assert!(app.worktrees[0].sessions[0].busy);
}

#[test]
fn events_keep_the_sidebar_current() {
    let mut ws = Workspace::new(Vec::new());
    ws.resync(snapshot());

    let fetch = ws.apply(
        &EventKind::Busy {
            session_id: "s1".into(),
        },
        NOW_MS,
    );
    assert!(fetch.is_empty());
    let app = ws
        .sidebar(now())
        .into_iter()
        .find(|p| p.name == "app")
        .expect("app");
    assert_eq!(
        ids(&app.sessions),
        ["s1", "s2"],
        "working moves a session to the top"
    );
    assert!(app.sessions[0].busy);

    let _ = ws.apply(
        &EventKind::Done {
            session_id: "s1".into(),
            outcome: "succeeded".into(),
        },
        NOW_MS,
    );
    let _ = ws.apply(
        &EventKind::SessionRenamed {
            session_id: "s1".into(),
            title: "Renamed".into(),
        },
        NOW_MS,
    );
    let _ = ws.apply(
        &EventKind::ModelSelected {
            session_id: "s1".into(),
            model: ModelRef {
                provider_id: "openai".into(),
                id: "gpt".into(),
                variant: None,
            },
        },
        NOW_MS,
    );
    let s1 = ws.session("s1").expect("s1");
    assert_eq!(s1.title.as_deref(), Some("Renamed"));
    assert_eq!(
        s1.model.as_ref().map(|m| m.provider_id.as_str()),
        Some("openai")
    );
    assert!(!ws.is_busy("s1"));

    let _ = ws.apply(
        &EventKind::SessionArchived {
            session_id: "s4".into(),
        },
        NOW_MS,
    );
    assert!(ws.session("s4").is_none());
}

#[test]
fn new_sessions_are_fetched_and_new_projects_adopted() {
    let mut ws = Workspace::new(Vec::new());
    ws.resync(snapshot());

    let fetch = ws.apply(
        &EventKind::SessionCreated {
            session_id: "s9".into(),
        },
        NOW_MS,
    );
    assert_eq!(fetch, [Fetch::Session("s9".into())]);

    // A session in a project the workspace hasn't seen asks for the project list.
    let fresh = session("s9", "p_new", "/code/new", 0.0);
    let fetch = ws.fetched(Fetched::Session("s9".into(), Some(Box::new(fresh))));
    assert_eq!(fetch, [Fetch::Projects]);

    let mut projects = snapshot().projects;
    projects.push(project("p_new", "/code/new"));
    let fetch = ws.fetched(Fetched::Projects(projects));
    assert_eq!(
        fetch,
        [Fetch::Worktrees("p_new".into())],
        "a git project's worktrees load next"
    );

    let order = ws.order();
    assert_eq!(
        order.last().map(String::as_str),
        Some("/code/new"),
        "adopted at the end"
    );
    assert_eq!(
        ws.sidebar(now())
            .last()
            .map(harlo_core::sessions::Project::total),
        Some(1)
    );

    // Archived just after it was made, its fetch can land after the archive
    // event: it stays out of the lists.
    let _ = ws.apply(
        &EventKind::SessionCreated {
            session_id: "s10".into(),
        },
        NOW_MS,
    );
    let _ = ws.apply(
        &EventKind::SessionArchived {
            session_id: "s10".into(),
        },
        NOW_MS,
    );
    let late = Session {
        archived: Some(NOW_MS),
        ..session("s10", "p_new", "/code/new", 0.0)
    };
    let _ = ws.fetched(Fetched::Session("s10".into(), Some(Box::new(late))));
    assert!(ws.session("s10").is_none(), "archived, it stays out");

    // A deleted session fetches as missing and goes away; replaying is harmless.
    let _ = ws.fetched(Fetched::Session("s9".into(), None));
    let _ = ws.fetched(Fetched::Session("s9".into(), None));
    assert!(ws.session("s9").is_none());
}

#[test]
fn a_rename_right_after_creation_is_kept() {
    let mut ws = Workspace::new(Vec::new());
    ws.resync(snapshot());
    let created = ws.apply(
        &EventKind::SessionCreated {
            session_id: "s9".into(),
        },
        NOW_MS,
    );
    assert_eq!(created, [Fetch::Session("s9".into())]);
    // Renamed before that fetch came back: fetched again.
    let renamed = ws.apply(
        &EventKind::SessionRenamed {
            session_id: "s9".into(),
            title: "Named".into(),
        },
        NOW_MS,
    );
    assert_eq!(renamed, [Fetch::Session("s9".into())]);
    // The second fetch answers first; the first, older one lands after it.
    let fresh = Session {
        title: Some("Named".into()),
        ..session("s9", "p_app", "/code/app", 0.0)
    };
    let stale = Session {
        title: None,
        updated: fresh.updated - 5.0,
        ..fresh.clone()
    };
    let _ = ws.fetched(Fetched::Session("s9".into(), Some(Box::new(fresh))));
    let _ = ws.fetched(Fetched::Session("s9".into(), Some(Box::new(stale))));
    assert_eq!(
        ws.session("s9").and_then(|s| s.title.as_deref()),
        Some("Named")
    );
}

#[test]
fn a_resync_replaces_everything_but_keeps_the_order() {
    let mut ws = Workspace::new(Vec::new());
    ws.resync(snapshot());
    ws.add_project("/code/added".into());
    ws.remove_project("/code/web");
    let before = ws.order().to_vec();

    let mut fresh = snapshot();
    fresh.sessions.retain(|s| s.id != "s1");
    fresh.active.clear();
    ws.resync(fresh);

    assert!(ws.session("s1").is_none(), "missed deletions disappear");
    assert!(!ws.is_busy("s3"), "missed completions clear");
    // The removed project comes back because it still has a session.
    assert_eq!(ws.order()[..before.len()], before[..]);
    assert_eq!(ws.order().last().map(String::as_str), Some("/code/web"));
    let added = ws
        .sidebar(now())
        .into_iter()
        .find(|p| p.directory == "/code/added");
    assert_eq!(
        added.map(|p| (p.total(), p.name)),
        Some((0, "added".into()))
    );
}

#[test]
fn worktrees_inside_the_project_folder_are_still_worktrees() {
    let mut ws = Workspace::new(Vec::new());
    let mut snap = snapshot();
    snap.worktrees.insert(
        "p_web".into(),
        vec!["/code/web".into(), "/code/web/.worktrees/fix".into()],
    );
    snap.sessions
        .push(session("s5", "p_web", "/code/web/.worktrees/fix", 0.2));
    ws.resync(snap);
    let web = ws
        .sidebar(now())
        .into_iter()
        .find(|p| p.name == "web")
        .expect("web");
    assert_eq!(ids(&web.sessions), ["s4"]);
    assert_eq!(ids(&web.worktrees[0].sessions), ["s5"]);
}

fn queued(id: &str) -> InboxItem {
    InboxItem {
        id: id.into(),
        kind: "user".into(),
        text: Some(format!("prompt {id}")),
        attachments: Vec::new(),
        pictures: Vec::new(),
        skills: Vec::new(),
        delivery: Delivery::Queue,
    }
}

#[test]
fn a_loaded_inbox_follows_events_and_agents_change() {
    let mut workspace = Workspace::new(Vec::new());
    workspace.resync(snapshot());
    let added = |id: &str| EventKind::InboxAdded {
        session_id: "s1".into(),
        item: queued(id),
    };

    // Not loaded yet: events don't build a partial inbox.
    workspace.apply(&added("m0"), 0.0);
    assert!(workspace.inbox("s1").is_empty());

    workspace.fetched(Fetched::Inbox("s1".into(), vec![queued("m1")]));
    workspace.apply(&added("m2"), 0.0);
    workspace.apply(&added("m3"), 0.0);
    workspace.apply(
        &EventKind::InboxDeliveryChanged {
            session_id: "s1".into(),
            item_id: "m2".into(),
            delivery: Delivery::Steer,
        },
        0.0,
    );
    workspace.apply(
        &EventKind::InboxRemoved {
            session_id: "s1".into(),
            item_id: "m1".into(),
            delivered: true,
        },
        0.0,
    );
    let inbox: Vec<(&str, Delivery)> = workspace
        .inbox("s1")
        .iter()
        .map(|i| (i.id.as_str(), i.delivery))
        .collect();
    assert_eq!(inbox, [("m2", Delivery::Steer), ("m3", Delivery::Queue)]);

    workspace.apply(
        &EventKind::AgentSelected {
            session_id: "s1".into(),
            agent: "plan".into(),
        },
        0.0,
    );
    assert_eq!(
        workspace.session("s1").and_then(|s| s.agent.as_deref()),
        Some("plan")
    );

    // A resync forgets inboxes; they're loaded again when shown.
    workspace.resync(snapshot());
    assert!(workspace.inbox("s1").is_empty());
}

fn asked(id: &str) -> Permission {
    Permission {
        id: id.into(),
        session_id: "s1".into(),
        action: "shell".into(),
        resources: vec!["cargo test".into()],
        save: vec!["cargo *".into()],
        message: None,
        files: Vec::new(),
        call: None,
    }
}

fn question(id: &str, session: &str) -> Form {
    Form {
        id: id.into(),
        session_id: session.into(),
        title: "Questions".into(),
        fields: Vec::new(),
    }
}

#[test]
fn requests_waiting_for_approval_follow_events_and_end_with_the_run() {
    let mut ws = Workspace::new(Vec::new());
    ws.resync(snapshot());
    // One was asked before Harlo attached; loading the session finds it.
    let _ = ws.fetched(Fetched::Permissions("s1".into(), vec![asked("per_1")]));
    let _ = ws.apply(
        &EventKind::PermissionAsked(Box::new(asked("per_2"))),
        NOW_MS,
    );
    let _ = ws.apply(
        &EventKind::PermissionAsked(Box::new(asked("per_2"))),
        NOW_MS,
    );
    let ids = |ws: &Workspace| -> Vec<String> {
        ws.permissions("s1").iter().map(|p| p.id.clone()).collect()
    };
    assert_eq!(
        ids(&ws),
        ["per_1", "per_2"],
        "asked once each, oldest first"
    );

    let _ = ws.apply(
        &EventKind::PermissionReplied {
            session_id: "s1".into(),
            request_id: "per_1".into(),
        },
        NOW_MS,
    );
    assert_eq!(ids(&ws), ["per_2"]);

    let _ = ws.apply(
        &EventKind::Done {
            session_id: "s1".into(),
            outcome: "interrupted".into(),
        },
        NOW_MS,
    );
    assert!(ws.permissions("s1").is_empty());
}

#[test]
fn questions_follow_events_and_end_with_the_run() {
    let mut ws = Workspace::new(Vec::new());
    ws.resync(snapshot());
    let _ = ws.fetched(Fetched::Forms("s1".into(), vec![question("frm_1", "s1")]));
    let _ = ws.apply(&EventKind::FormCreated(question("frm_2", "s1")), NOW_MS);
    let _ = ws.apply(&EventKind::FormCreated(question("frm_2", "s1")), NOW_MS);
    let ids =
        |ws: &Workspace| -> Vec<String> { ws.forms("s1").iter().map(|f| f.id.clone()).collect() };
    assert_eq!(
        ids(&ws),
        ["frm_1", "frm_2"],
        "asked once each, oldest first"
    );
    assert!(ws.needs_user("s1"));

    let _ = ws.apply(
        &EventKind::FormSettled {
            session_id: "s1".into(),
            form_id: "frm_1".into(),
        },
        NOW_MS,
    );
    assert_eq!(ids(&ws), ["frm_2"]);
    // Answered here: gone at once, before the server says so.
    ws.settle_form("s1", "frm_2");
    assert!(!ws.needs_user("s1"));

    let _ = ws.apply(&EventKind::FormCreated(question("frm_3", "s1")), NOW_MS);
    let _ = ws.apply(
        &EventKind::Done {
            session_id: "s1".into(),
            outcome: "interrupted".into(),
        },
        NOW_MS,
    );
    assert!(ws.forms("s1").is_empty());
}

#[test]
fn a_reconnect_replaces_what_waits_for_the_user() {
    let mut ws = Workspace::new(Vec::new());
    ws.resync(snapshot());
    // Asked before the connection dropped, then answered elsewhere.
    let _ = ws.apply(
        &EventKind::PermissionAsked(Box::new(asked("per_old"))),
        NOW_MS,
    );
    let _ = ws.apply(&EventKind::FormCreated(question("frm_old", "s1")), NOW_MS);
    // While away, s3 asked for approval and s4 asked a question.
    let away = Snapshot {
        permissions: vec![Permission {
            session_id: "s3".into(),
            ..asked("per_new")
        }],
        forms: vec![question("frm_new", "s4")],
        ..snapshot()
    };
    ws.resync(away);
    assert!(!ws.needs_user("s1"), "answered elsewhere, so gone");
    assert_eq!(ws.permissions("s3")[0].id, "per_new");
    assert_eq!(ws.forms("s4")[0].id, "frm_new");
}

#[test]
fn a_sessions_rules_follow_events() {
    let mut ws = Workspace::new(Vec::new());
    ws.resync(snapshot());
    let rules = vec![Rule::any("*", Effect::Allow)];
    let _ = ws.apply(
        &EventKind::SessionPermissions {
            session_id: "s1".into(),
            rules: rules.clone(),
        },
        NOW_MS,
    );
    assert_eq!(ws.session("s1").map(|s| s.permissions.clone()), Some(rules));
}

#[test]
fn a_staged_undo_follows_events() {
    let mut ws = Workspace::new(Vec::new());
    ws.resync(snapshot());
    let revert = Revert {
        message_id: "msg_1".into(),
        files: Vec::new(),
    };
    let staged = |ws: &Workspace| ws.session("s1").and_then(|s| s.revert.clone());
    let _ = ws.apply(
        &EventKind::RevertStaged {
            session_id: "s1".into(),
            revert: revert.clone(),
        },
        NOW_MS,
    );
    assert_eq!(staged(&ws), Some(revert));
    let _ = ws.apply(
        &EventKind::RevertCommitted {
            session_id: "s1".into(),
        },
        NOW_MS,
    );
    assert_eq!(staged(&ws), None, "kept elsewhere, the card goes");
}

#[test]
fn children_load_with_their_parent_and_flag_it_when_they_ask() {
    let mut ws = Workspace::new(Vec::new());
    ws.resync(snapshot());
    let later = Session {
        parent_id: Some("s1".into()),
        agent: Some("explore".into()),
        ..session("later", "p_app", "/code/app", 0.05)
    };
    let more = ws.fetched(Fetched::Children("s1".into(), vec![later]));
    assert!(more.is_empty());
    let children: Vec<&str> = ws.children("s1").iter().map(|c| c.id.as_str()).collect();
    assert_eq!(children, ["child", "later"], "in the order they started");

    // A child asking for approval makes its parent's row ask too.
    let mut waiting = asked("per_child");
    waiting.session_id = "later".into();
    let _ = ws.apply(&EventKind::PermissionAsked(Box::new(waiting)), NOW_MS);
    let app = ws
        .sidebar(now())
        .into_iter()
        .find(|p| p.name == "app")
        .expect("app");
    let s1 = app.sessions.iter().find(|s| s.id == "s1").expect("s1");
    assert!(s1.waiting, "the parent asks for its child");
    let later = s1.children.iter().find(|c| c.id == "later").expect("later");
    assert!(later.waiting);
    assert_eq!(later.agent.as_deref(), Some("explore"));
    assert!(!ws.needs_user("s1"), "the parent itself asks for nothing");
}

#[test]
fn a_hidden_project_stays_hidden_until_added_again() {
    let mut ws = Workspace::new(Vec::new());
    ws.resync(snapshot());
    ws.hide_project("/code/web");
    assert!(!ws.order().iter().any(|d| d == "/code/web"));
    assert_eq!(ws.hidden(), ["/code/web".to_owned()]);

    // Its sessions changing, or a fresh snapshot, don't bring it back.
    ws.resync(snapshot());
    assert!(!ws.order().iter().any(|d| d == "/code/web"));
    assert!(
        !ws.sidebar(now()).iter().any(|p| p.directory == "/code/web"),
        "not in the sidebar"
    );

    // As saved and loaded again.
    let mut loaded = Workspace::new(ws.order().to_vec());
    loaded.set_hidden(ws.hidden().to_vec());
    loaded.resync(snapshot());
    assert!(!loaded.order().iter().any(|d| d == "/code/web"));

    // Adding it again does.
    ws.add_project("/code/web".into());
    assert!(ws.hidden().is_empty());
    assert!(ws.order().iter().any(|d| d == "/code/web"));
}

#[test]
fn sessions_started_without_a_project_are_listed_last_as_other_sessions() {
    let mut data = snapshot();
    data.projects.push(Project {
        git: false,
        ..project("p_home", "/home/me")
    });
    data.sessions
        .push(session("loose", "p_home", "/home/me", 0.2));
    data.sessions
        .push(session("older", "p_home", "/home/me", 3.0));
    // A layout from before lists and hides the home folder as a project.
    let mut ws = Workspace::new(vec!["/home/me".into(), "/code/idle".into()]);
    ws.set_hidden(vec!["/home/me".into()]);
    ws.resync(data);
    ws.set_other_folder(Some("/home/me".into()));
    let sidebar = ws.sidebar(now());

    let names: Vec<&str> = sidebar.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["idle", "web", "app", "Other sessions"]);
    let other = sidebar.last().expect("the group");
    assert!(other.other && !sidebar[..3].iter().any(|p| p.other));
    assert_eq!(other.directory, "/home/me");
    assert_eq!(ids(&other.sessions), ["loose", "older"], "newest first");

    // Without such sessions there's no group.
    let mut ws = Workspace::new(Vec::new());
    ws.resync(snapshot());
    ws.set_other_folder(Some("/home/me".into()));
    assert!(ws.sidebar(now()).iter().all(|p| !p.other));
}

#[test]
fn sessions_that_answered_since_a_time_are_told_apart() {
    let mut snapshot = snapshot();
    let ended = |s: &mut Session, at: f64, outcome: &str| {
        s.idle = Some(at);
        s.outcome = Some(outcome.to_owned());
    };
    let mut answered = session("s_answered", "p1", "/code/app", 1.0);
    ended(&mut answered, 2_000.0, "succeeded");
    let mut failed = session("s_failed", "p1", "/code/app", 1.0);
    ended(&mut failed, 2_500.4, "failed");
    let mut stopped = session("s_stopped", "p1", "/code/app", 1.0);
    ended(&mut stopped, 3_000.0, "interrupted");
    let mut earlier = session("s_earlier", "p1", "/code/app", 1.0);
    ended(&mut earlier, 1_000.0, "succeeded");
    let mut working = session("s_working", "p1", "/code/app", 1.0);
    ended(&mut working, 2_000.0, "succeeded");
    let mut helper = session("s_helper", "p1", "/code/app", 1.0);
    ended(&mut helper, 2_000.0, "succeeded");
    helper.parent_id = Some("s_answered".to_owned());
    snapshot
        .sessions
        .extend([answered, failed, stopped, earlier, working, helper]);
    snapshot.active = vec!["s_working".to_owned()];
    let mut ws = Workspace::default();
    ws.resync(snapshot);

    let mut since: Vec<&str> = ws.answered_since(1_000).collect();
    since.sort_unstable();
    assert_eq!(since, ["s_answered", "s_failed"]);
    assert_eq!(ws.answered_since(2_500).count(), 0, "nothing later");
    assert_eq!(ws.last_ended(), 3_000);
    assert_eq!(Workspace::default().last_ended(), 0);
}
