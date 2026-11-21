//! The workspace reducer: snapshots, live events and the sidebar it builds.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use harlo_core::workspace::{Fetch, Fetched, Snapshot, Workspace};
use opencode_client::{EventKind, ModelRef, Project, Session};

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
        }),
        created: NOW_MS - hours_ago * HOUR_MS,
        updated: NOW_MS - hours_ago * HOUR_MS,
        revert: None,
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
        "the subagent session is left out"
    );

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

    // A deleted session fetches as missing and goes away; replaying is harmless.
    let _ = ws.fetched(Fetched::Session("s9".into(), None));
    let _ = ws.fetched(Fetched::Session("s9".into(), None));
    assert!(ws.session("s9").is_none());
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
