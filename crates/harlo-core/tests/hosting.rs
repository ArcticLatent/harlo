//! Signing git in to hosting sites: reading the tools' words, and the
//! sign-ins themselves with stand-in `gh` and `glab` scripts and a stand-in
//! Forgejo server.
//!
//! Every command runs with a scratch `HOME`, `PATH` and git config, so the
//! real tools, keyring and git config are never touched.

// Rust guideline compliant 2026-10-09

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

use futures_util::StreamExt as _;
use harlo_core::hosting::{
    Account, Hosting, Progress, Service, account_in_status, forgejo_base, normalize_host,
    one_time_code, url_in,
};

#[test]
fn hosts_as_typed() {
    assert_eq!(normalize_host("https://Codeberg.org/"), "codeberg.org");
    assert_eq!(
        normalize_host("git.example.org:3000/user"),
        "git.example.org:3000"
    );
    assert_eq!(normalize_host(" gitlab.com "), "gitlab.com");
    assert_eq!(normalize_host("forgejo:3000"), "forgejo:3000");
    assert_eq!(normalize_host("not a host"), "");
    assert_eq!(normalize_host(""), "");
    assert_eq!(forgejo_base("http://forgejo:3000/"), "http://forgejo:3000");
    assert_eq!(
        forgejo_base("git.company.lan:3000"),
        "https://git.company.lan:3000"
    );
}

#[test]
fn who_the_tools_say_is_signed_in() {
    let gh = "github.com\n  ✓ Logged in to github.com account octocat (keyring)\n  - Active account: true\n";
    assert_eq!(
        account_in_status(gh, "github.com").as_deref(),
        Some("octocat")
    );
    assert_eq!(
        account_in_status(
            "✓ Logged in to github.com as hubot (oauth_token)",
            "github.com"
        )
        .as_deref(),
        Some("hubot")
    );
    let glab = "gitlab.com\n  ✓ Logged in to gitlab.com as tanuki (keyring)\n  ✓ Git operations for gitlab.com configured to use https protocol.";
    assert_eq!(
        account_in_status(glab, "gitlab.com").as_deref(),
        Some("tanuki")
    );
    assert_eq!(account_in_status(glab, "gitlab.example.org"), None);
    assert_eq!(
        account_in_status(
            "You are not logged into any GitHub hosts. To log in, run: gh auth login",
            "github.com"
        ),
        None
    );
}

#[test]
fn one_time_codes_and_pages() {
    assert_eq!(
        one_time_code("! First copy your one-time code: 1A2B-3C4D").as_deref(),
        Some("1A2B-3C4D")
    );
    assert_eq!(
        one_time_code("Enter the code 0A44L90H at https://gitlab.com/oauth/device").as_deref(),
        Some("0A44L90H")
    );
    assert_eq!(
        one_time_code("Open this URL to continue: https://github.com/login/device"),
        None
    );
    assert_eq!(one_time_code("Authentication complete."), None);
    assert_eq!(one_time_code("code: lowercase-word"), None);
    assert_eq!(
        url_in("Enter the code 0A44L90H at https://gitlab.com/oauth/device.").as_deref(),
        Some("https://gitlab.com/oauth/device")
    );
    assert_eq!(url_in("no page here"), None);
}

/// A scratch home with stand-in tools on its own `PATH`.
struct Scratch {
    dir: tempfile::TempDir,
}

impl Scratch {
    /// A scratch home, with `tools` (name and script) in its `bin`, and
    /// git beside them.
    fn new(tools: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().expect("a scratch folder");
        let bin = dir.path().join("bin");
        for sub in ["bin", "home", "state", "config"] {
            std::fs::create_dir_all(dir.path().join(sub)).expect("a folder");
        }
        let git = which("git");
        std::os::unix::fs::symlink(git, bin.join("git")).expect("git");
        for (name, script) in tools {
            let path = bin.join(name);
            std::fs::write(&path, script).expect("a stand-in");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("runnable");
        }
        Self { dir }
    }

    fn path(&self, sub: &str) -> PathBuf {
        self.dir.path().join(sub)
    }

    /// The tools, run only inside this scratch home.
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

    /// A key in the scratch git config.
    fn git_config(&self, key: &str) -> Option<String> {
        let out = std::process::Command::new(which("git"))
            .args(["config", "--file"])
            .arg(self.path("home/.gitconfig"))
            .args(["--get", key])
            .output()
            .expect("git");
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }
}

/// Where `name` is on this computer's `PATH`.
fn which(name: &str) -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").expect("a PATH"))
        .map(|folder| folder.join(name))
        .find(|path| path.is_file())
        .unwrap_or_else(|| panic!("{name} isn't installed"))
}

/// A stand-in `gh`: signed in once `auth login` got its Enter.
const GH: &str = r#"#!/bin/sh
PATH=/usr/bin:/bin
echo "$*" >> "$STATE/gh.log"
case "$1 $2" in
"auth status")
    if [ -f "$STATE/gh-in" ]; then
        echo "github.com" >&2
        echo "  ✓ Logged in to github.com account octocat (keyring)" >&2
        exit 0
    fi
    echo "You are not logged into any GitHub hosts. To log in, run: gh auth login" >&2
    exit 1 ;;
"auth login")
    echo "! First copy your one-time code: 1A2B-3C4D"
    printf 'Press Enter to open https://github.com/login/device in your browser... '
    read answer
    touch "$STATE/gh-in"
    echo
    echo "✓ Authentication complete." >&2
    exit 0 ;;
"auth setup-git") exit 0 ;;
"auth logout") rm -f "$STATE/gh-in"; exit 0 ;;
esac
exit 2
"#;

/// A stand-in `glab` whose device sign-in the server refuses.
const GLAB_REFUSED: &str = r#"#!/bin/sh
case "$1 $2" in
"auth status") echo "  x gitlab.com: API call failed: 401" >&2; exit 1 ;;
"auth login")
    echo "Enter the code 0A44L90H at https://gitlab.com/oauth/device"
    echo "ERROR: the device flow isn't enabled on this server" >&2
    exit 1 ;;
esac
exit 2
"#;

/// A stand-in `glab` that signs in.
const GLAB: &str = r#"#!/bin/sh
PATH=/usr/bin:/bin
case "$1 $2" in
"auth status")
    if [ -f "$STATE/glab-in" ]; then
        echo "  ✓ Logged in to gitlab.com as tanuki (keyring)" >&2; exit 0
    fi
    exit 1 ;;
"auth login")
    echo "Enter the code 0A44L90H at https://gitlab.com/oauth/device"
    touch "$STATE/glab-in"
    exit 0 ;;
esac
exit 2
"#;

#[tokio::test]
async fn github_signs_in_with_a_code_and_sets_git_up() {
    let scratch = Scratch::new(&[("gh", GH)]);
    let hosting = scratch.hosting();
    assert_eq!(
        hosting.account(Service::GitHub, "github.com").await,
        Account::SignedOut
    );

    let steps: Vec<Progress> = hosting
        .sign_in(Service::GitHub, "github.com")
        .collect()
        .await;
    assert_eq!(
        steps,
        [
            Progress::Code {
                code: "1A2B-3C4D".into(),
                url: "https://github.com/login/device".into(),
            },
            Progress::Done(Ok("octocat".into())),
        ]
    );
    let log = std::fs::read_to_string(scratch.path("state/gh.log")).expect("the log");
    assert!(
        log.contains("auth login --hostname github.com --web --git-protocol https"),
        "{log}"
    );
    assert!(
        log.contains("auth setup-git --hostname github.com"),
        "{log}"
    );

    hosting
        .sign_out(Service::GitHub, "github.com", "octocat")
        .await
        .expect("signed out");
    assert!(log_has(
        &scratch,
        "auth logout --hostname github.com --user octocat"
    ));
    assert_eq!(
        hosting.account(Service::GitHub, "github.com").await,
        Account::SignedOut
    );
}

fn log_has(scratch: &Scratch, line: &str) -> bool {
    std::fs::read_to_string(scratch.path("state/gh.log")).is_ok_and(|log| log.contains(line))
}

#[tokio::test]
async fn gitlab_signs_in_and_git_asks_glab_for_that_host() {
    let scratch = Scratch::new(&[("glab", GLAB)]);
    let hosting = scratch.hosting();
    let steps: Vec<Progress> = hosting
        .sign_in(Service::GitLab, "gitlab.com")
        .collect()
        .await;
    assert_eq!(
        steps.last(),
        Some(&Progress::Done(Ok("tanuki".into()))),
        "{steps:?}"
    );
    let helper = scratch
        .git_config("credential.https://gitlab.com.helper")
        .expect("a helper");
    assert!(
        helper.starts_with('!') && helper.ends_with("glab auth git-credential"),
        "{helper}"
    );
}

#[tokio::test]
async fn a_refused_sign_in_says_why_and_sets_nothing_up() {
    let scratch = Scratch::new(&[("glab", GLAB_REFUSED)]);
    let steps: Vec<Progress> = scratch
        .hosting()
        .sign_in(Service::GitLab, "gitlab.com")
        .collect()
        .await;
    assert_eq!(
        steps.last(),
        Some(&Progress::Done(Err(
            "ERROR: the device flow isn't enabled on this server".into()
        )))
    );
    assert_eq!(
        scratch.git_config("credential.https://gitlab.com.helper"),
        None
    );
}

#[tokio::test]
async fn a_missing_tool_is_said_so() {
    let scratch = Scratch::new(&[]);
    let hosting = scratch.hosting();
    assert_eq!(
        hosting.account(Service::GitHub, "github.com").await,
        Account::Missing
    );
    let steps: Vec<Progress> = hosting
        .sign_in(Service::GitHub, "github.com")
        .collect()
        .await;
    assert_eq!(steps, [Progress::Done(Err("gh isn't installed.".into()))]);
}

/// A stand-in Forgejo server on this computer answering `/api/v1/user`
/// for the token `good`; returns its base address.
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
                ("401 Unauthorized", r#"{"message":"token is required"}"#)
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

#[tokio::test]
async fn a_forgejo_token_is_checked_then_kept_by_git_for_that_server() {
    let scratch = Scratch::new(&[]);
    let hosting = scratch.hosting();
    let base = forgejo();
    let host = base.trim_start_matches("http://");

    let refused = hosting
        .forgejo_sign_in(&base, "bad")
        .await
        .expect_err("a bad token");
    assert!(refused.contains("didn't accept the token"), "{refused}");
    assert!(!scratch.path("home/.git-credentials").exists());

    let login = hosting
        .forgejo_sign_in(&base, " good ")
        .await
        .expect("signed in");
    assert_eq!(login, "forge");
    assert_eq!(
        scratch
            .git_config(&format!("credential.{base}.helper"))
            .as_deref(),
        Some("store")
    );
    let kept = std::fs::read_to_string(scratch.path("home/.git-credentials")).expect("kept");
    // git's store writes the port's colon as %3a.
    let port = host.rsplit(':').next().expect("a port");
    assert_eq!(kept.trim(), format!("http://forge:good@127.0.0.1%3a{port}"));

    hosting
        .forgejo_sign_out(&base, "forge")
        .await
        .expect("signed out");
    let kept = std::fs::read_to_string(scratch.path("home/.git-credentials")).expect("read");
    assert!(kept.trim().is_empty(), "{kept}");
}
