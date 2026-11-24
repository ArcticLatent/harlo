//! Working out a session's instruction files the way the server does, in
//! scratch folders standing in for home, config and project.

// Rust guideline compliant 2026-10-09

use std::path::Path;

use harlo_core::instructions::{File, Places, Scope, display, display_in, files};

fn put(path: &Path) {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("folders");
    std::fs::write(path, "rules\n").expect("file");
}

fn file(path: &Path, scope: Scope) -> File {
    File {
        path: path.to_owned(),
        scope,
    }
}

#[test]
fn global_files_then_each_folder_up_to_home() {
    let root = tempfile::tempdir().expect("scratch");
    let home = root.path().join("home");
    let config = home.join(".config/opencode");
    let project = home.join("code/app");
    let folder = project.join("crates/core");
    std::fs::create_dir_all(&folder).expect("folders");
    put(&config.join("AGENTS.md"));
    put(&home.join(".claude/CLAUDE.md"));
    put(&folder.join("AGENTS.md"));
    // A folder with both gives only its AGENTS.md.
    put(&project.join("AGENTS.md"));
    put(&project.join("CLAUDE.md"));
    // A folder with only CLAUDE.md gives that, above the project too.
    put(&home.join("code/CLAUDE.md"));
    // Nothing above home counts.
    put(&root.path().join("AGENTS.md"));

    let found = files(&Places {
        folder: folder.clone(),
        project: project.clone(),
        home: home.clone(),
        config: config.clone(),
    });
    assert_eq!(
        found,
        vec![
            file(&config.join("AGENTS.md"), Scope::Global),
            file(&home.join(".claude/CLAUDE.md"), Scope::Global),
            file(&folder.join("AGENTS.md"), Scope::Project),
            file(&project.join("AGENTS.md"), Scope::Project),
            file(&home.join("code/CLAUDE.md"), Scope::Project),
        ]
    );
}

#[test]
fn outside_home_the_walk_stops_at_the_project() {
    let root = tempfile::tempdir().expect("scratch");
    let home = root.path().join("home");
    let project = root.path().join("srv/app");
    let folder = project.join("web");
    std::fs::create_dir_all(&folder).expect("folders");
    put(&project.join("AGENTS.md"));
    put(&root.path().join("srv/AGENTS.md"));
    let found = files(&Places {
        folder,
        project: project.clone(),
        home: home.clone(),
        config: home.join(".config/opencode"),
    });
    assert_eq!(
        found,
        vec![file(&project.join("AGENTS.md"), Scope::Project)]
    );
}

#[test]
fn a_folder_outside_its_project_gets_only_global_files() {
    let root = tempfile::tempdir().expect("scratch");
    let home = root.path().join("home");
    let folder = home.join("elsewhere");
    std::fs::create_dir_all(&folder).expect("folders");
    put(&folder.join("AGENTS.md"));
    let found = files(&Places {
        folder,
        project: home.join("code/app"),
        home: home.clone(),
        config: home.join(".config/opencode"),
    });
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn paths_show_home_as_a_tilde() {
    let home = Path::new("/home/me");
    assert_eq!(
        display(Path::new("/home/me/code/AGENTS.md"), home),
        "~/code/AGENTS.md"
    );
    assert_eq!(display(Path::new("/home/me"), home), "~");
    assert_eq!(display(Path::new("/srv/AGENTS.md"), home), "/srv/AGENTS.md");
    assert_eq!(
        display(Path::new("/srv/AGENTS.md"), Path::new("")),
        "/srv/AGENTS.md",
        "no home known"
    );
}

#[test]
fn paths_inside_the_project_show_from_its_root() {
    let (project, home) = (Path::new("/home/me/app"), Path::new("/home/me"));
    assert_eq!(
        display_in(Path::new("/home/me/app/sub/AGENTS.md"), project, home),
        "sub/AGENTS.md"
    );
    assert_eq!(
        display_in(Path::new("/home/me/.claude/CLAUDE.md"), project, home),
        "~/.claude/CLAUDE.md"
    );
    assert_eq!(
        display_in(Path::new("/srv/AGENTS.md"), Path::new(""), home),
        "/srv/AGENTS.md"
    );
}
