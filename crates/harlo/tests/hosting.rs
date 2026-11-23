//! Git hosting in Settings → Git: who is signed in, signing in to GitHub
//! with a one-time code, cancelling, signing out, and a Forgejo token.
//!
//! The tools are stand-in scripts in a scratch home with its own `PATH` and
//! git config, and Forgejo is a stand-in server on this computer, so the
//! real tools, keyring and git config are never touched.

// Rust guideline compliant 2026-10-09

mod support;

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;
use std::time::Duration;

use harlo::app::{Field, HostingEvent, Load, Message, SheetEvent};
use harlo_core::hosting::{Account, Hosting, Service};
use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// How long a step may take.
const WAIT: Duration = Duration::from_secs(20);

/// A stand-in `gh` whose sign-in waits until the test approves it.
const GH: &str = r#"#!/bin/sh
PATH=/usr/bin:/bin
echo "$*" >> "$STATE/gh.log"
case "$1 $2" in
"auth status")
    if [ -f "$STATE/gh-in" ]; then
        echo "  ✓ Logged in to github.com account octocat (keyring)" >&2; exit 0
    fi
    echo "You are not logged into any GitHub hosts." >&2; exit 1 ;;
"auth login")
    echo "! First copy your one-time code: 1A2B-3C4D"
    read answer
    while [ ! -f "$STATE/approve" ]; do sleep 0.05; done
    touch "$STATE/gh-in"
    exit 0 ;;
"auth setup-git") exit 0 ;;
"auth logout") rm -f "$STATE/gh-in"; exit 0 ;;
esac
exit 2
"#;

/// A scratch home with the stand-in `gh` and git on its `PATH`.
struct Scratch {
    dir: tempfile::TempDir,
}

impl Scratch {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a scratch folder");
        for sub in ["bin", "home", "state", "config"] {
            std::fs::create_dir_all(dir.path().join(sub)).expect("a folder");
        }
        let git = std::env::split_paths(&std::env::var_os("PATH").expect("a PATH"))
            .map(|folder| folder.join("git"))
            .find(|path| path.is_file())
            .expect("git");
        std::os::unix::fs::symlink(git, dir.path().join("bin/git")).expect("git");
        let gh = dir.path().join("bin/gh");
        std::fs::write(&gh, GH).expect("a stand-in");
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).expect("runnable");
        Self { dir }
    }

    fn path(&self, sub: &str) -> PathBuf {
        self.dir.path().join(sub)
    }

    fn hosting(&self) -> Hosting {
        let text = |sub: &str| self.path(sub).to_string_lossy().into_owned();
        Hosting::new()
            .env("PATH", text("bin"))
            .env("HOME", text("home"))
            .env("XDG_CONFIG_HOME", text("config"))
            .env("GIT_CONFIG_GLOBAL", text("home/.gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("STATE", text("state"))
            .credential_helper("store")
    }
}

/// Settings → Git, with the scratch tools.
fn settings(scratch: &Scratch, tasks: &mut Tasks) -> Ui {
    let mut app = demo::app();
    app.hosting = Some(scratch.hosting());
    let ui = ui(app);
    ui.press("Settings");
    ui.press("Git");
    assert!(
        tasks.until(&ui, WAIT, |ui| {
            let accounts = &ui.app().sheets.hosting.accounts;
            accounts.get(&Service::GitHub) == Some(&Load::Loaded(Account::SignedOut))
                && accounts.get(&Service::GitLab) == Some(&Load::Loaded(Account::Missing))
        }),
        "who is signed in"
    );
    assert!(ui.shows("Needs glab, GitLab's CLI"));
    ui
}

fn hosting(ui: &Ui, event: HostingEvent) {
    ui.send(Message::Sheet(SheetEvent::Hosting(event)));
}

#[test]
fn github_signs_in_with_a_code_then_out() {
    let scratch = Scratch::new();
    let mut tasks = Tasks::new();
    let ui = settings(&scratch, &mut tasks);

    ui.press("Sign in");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui
            .shows("Enter 1A2B-3C4D at github.com/login/device")),
        "the code"
    );
    assert!(ui.shows("Copy and open"));
    assert!(ui.shows("Waiting for you to finish on the site…"));

    std::fs::write(scratch.path("state/approve"), "").expect("approved");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui
            .shows("git is signed in to github.com as octocat.")),
        "signed in"
    );
    assert!(ui.shows("Signed in as octocat"));
    let log = std::fs::read_to_string(scratch.path("state/gh.log")).expect("the log");
    assert!(
        log.contains("auth setup-git --hostname github.com"),
        "{log}"
    );

    ui.press("Sign out");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Signed out.")
            && ui.shows("Not signed in")),
        "signed out"
    );
}

#[test]
fn a_sign_in_can_be_cancelled() {
    let scratch = Scratch::new();
    let mut tasks = Tasks::new();
    let ui = settings(&scratch, &mut tasks);
    ui.press("Sign in");
    assert!(tasks.until(&ui, WAIT, |ui| ui.shows("Copy and open")));
    ui.press("Cancel");
    assert!(ui.app().sheets.hosting.signing_in.is_none());
    assert!(ui.shows("Not signed in"));
    // The stopped tool's last word is dropped.
    hosting(
        &ui,
        HostingEvent::Progress(
            Service::GitHub,
            harlo_core::hosting::Progress::Done(Ok("late".into())),
        ),
    );
    assert!(!ui.shows("Signed in as late"));
    assert_eq!(
        ui.app().sheets.hosting.accounts.get(&Service::GitHub),
        Some(&Load::Loaded(Account::SignedOut))
    );
}

/// A stand-in Forgejo server answering `/api/v1/user` for the token
/// `good`; returns its address.
fn forgejo() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let base = format!("http://{}", listener.local_addr().expect("an address"));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(&stream);
            let mut good = false;
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                good |= line
                    .trim()
                    .eq_ignore_ascii_case("authorization: token good");
                line.clear();
            }
            let (status, body) = if good {
                ("200 OK", r#"{"login":"forge"}"#)
            } else {
                ("401 Unauthorized", "{}")
            };
            let mut stream = &stream;
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    base
}

#[test]
fn a_forgejo_token_signs_in_and_out() {
    let scratch = Scratch::new();
    let mut tasks = Tasks::new();
    let ui = settings(&scratch, &mut tasks);
    let base = forgejo();
    let host = base.trim_start_matches("http://").to_owned();

    ui.send(Message::Sheet(SheetEvent::Text(
        Field::ForgejoHost,
        base.clone(),
    )));
    ui.send(Message::Sheet(SheetEvent::Text(
        Field::ForgejoToken,
        "bad".into(),
    )));
    assert!(
        tasks.until(&ui, WAIT, |ui| ui
            .shows(&format!("{host} didn't accept the token. It needs the read:user scope, and repository access to push."))),
        "refused"
    );
    ui.send(Message::Sheet(SheetEvent::Text(
        Field::ForgejoToken,
        "good".into(),
    )));
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Signed in as forge")),
        "signed in"
    );
    assert_eq!(
        ui.app().preferences.forgejo_account.as_deref(),
        Some(format!("{host} forge").as_str())
    );
    assert!(!ui.shows("Sign in Token"), "no token field once signed in");

    hosting(&ui, HostingEvent::SignOut(Service::Forgejo));
    assert!(tasks.until(&ui, WAIT, |ui| ui.shows("Signed out.")));
    assert_eq!(ui.app().preferences.forgejo_account, None);
    let kept = std::fs::read_to_string(scratch.path("home/.git-credentials")).expect("read");
    assert!(kept.trim().is_empty(), "{kept}");
}

#[test]
fn without_the_tools_there_are_no_hosting_rows() {
    let ui = ui(demo::app());
    ui.press("Settings");
    ui.press("Git");
    assert!(!ui.shows("Git hosting"));
}
