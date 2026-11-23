//! Signing git in to the sites that host repositories, so pushes and pulls
//! over HTTPS work: GitHub and GitLab through their command-line tools,
//! Forgejo (and Gitea) with an access token.
//!
//! - GitHub: `gh auth login --web` shows a one-time code to enter on the
//!   site; then `gh auth setup-git` makes git ask gh.
//! - GitLab: `glab auth login --device`, the same way; then git's credential
//!   helper for that host is glab.
//! - Forgejo: the token is checked with the server's API, then kept by
//!   git's credential helper for that host only.
//!
//! The tools and git keep the sign-in; Harlo keeps no token, and never reads
//! the ones they keep. SSH remotes keep using SSH keys.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use futures_util::stream::{self, BoxStream, StreamExt as _};
use tokio::io::{AsyncBufReadExt as _, AsyncRead, AsyncWriteExt as _, BufReader};
use tokio::process::{Child, ChildStdin, Command};

/// How long a call to a Forgejo server may take.
const API_TIMEOUT: Duration = Duration::from_secs(20);

/// How long a tool may take to say who is signed in.
const STATUS_TIMEOUT: Duration = Duration::from_secs(15);

/// A site that hosts git repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Service {
    /// GitHub, through the GitHub CLI.
    GitHub,
    /// GitLab, through glab.
    GitLab,
    /// Forgejo or Gitea, with a token.
    Forgejo,
}

impl Service {
    /// Every service, in the order they're shown.
    pub const ALL: [Self; 3] = [Self::GitHub, Self::GitLab, Self::Forgejo];

    /// Its name.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::GitLab => "GitLab",
            Self::Forgejo => "Forgejo",
        }
    }

    /// Where it is unless another host is set: GitHub's and GitLab's own
    /// sites, and Codeberg, the largest Forgejo.
    #[must_use]
    pub fn default_host(self) -> &'static str {
        match self {
            Self::GitHub => "github.com",
            Self::GitLab => "gitlab.com",
            Self::Forgejo => "codeberg.org",
        }
    }

    /// The command-line tool that signs in to it; Forgejo has none.
    #[must_use]
    pub fn cli(self) -> Option<&'static str> {
        match self {
            Self::GitHub => Some("gh"),
            Self::GitLab => Some("glab"),
            Self::Forgejo => None,
        }
    }

    /// The page where its one-time code is entered, at `host`.
    fn device_page(self, host: &str) -> String {
        match self {
            Self::GitLab => format!("https://{host}/oauth/device"),
            Self::GitHub | Self::Forgejo => format!("https://{host}/login/device"),
        }
    }
}

/// A host as typed (`https://git.example.org/`, `git.example.org`) → its
/// name (`git.example.org`); empty when it isn't one. A server on the local
/// network may have no dots and a port: `forgejo:3000`.
#[must_use]
pub fn normalize_host(typed: &str) -> String {
    let typed = typed.trim();
    let rest = typed
        .strip_prefix("https://")
        .or_else(|| typed.strip_prefix("http://"))
        .unwrap_or(typed);
    let host = rest
        .split('/')
        .next()
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let ok = host
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'));
    if ok { host } else { String::new() }
}

/// A Forgejo server's web address as typed: `https://host[:port]`, or
/// `http://…` when it was typed so (a server on the local network without
/// TLS).
#[must_use]
pub fn forgejo_base(typed: &str) -> String {
    let host = normalize_host(typed);
    if typed.trim().to_lowercase().starts_with("http://") {
        format!("http://{host}")
    } else {
        format!("https://{host}")
    }
}

/// The account `gh auth status` or `glab auth status` says is signed in to
/// `host`: "Logged in to github.com account octocat (keyring)", "Logged in
/// to gitlab.com as tanuki (keyring)".
#[must_use]
pub fn account_in_status(text: &str, host: &str) -> Option<String> {
    const SAID: &str = "Logged in to ";
    text.lines().find_map(|line| {
        let rest = &line[line.find(SAID)? + SAID.len()..];
        let rest = rest.strip_prefix(host)?.trim_start();
        let rest = rest
            .strip_prefix("account ")
            .or_else(|| rest.strip_prefix("as "))?;
        let name: String = rest
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != '(')
            .collect();
        (!name.is_empty()).then_some(name)
    })
}

/// The one-time code a device sign-in shows ("First copy your one-time
/// code: 1A2B-3C4D", "Enter the code 0A44L90H"): a word of capitals and
/// digits, a dash allowed, on a line that mentions a code.
#[must_use]
pub fn one_time_code(line: &str) -> Option<String> {
    if !line.to_lowercase().contains("code") {
        return None;
    }
    line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter(|w| (6..=12).contains(&w.len()))
        .find(|w| {
            w.chars()
                .all(|c| c == '-' || c.is_ascii_uppercase() || c.is_ascii_digit())
                && w.chars().any(|c| c.is_ascii_digit())
                && !w.starts_with('-')
                && !w.ends_with('-')
        })
        .map(str::to_owned)
}

/// The first web address in `line`.
#[must_use]
pub fn url_in(line: &str) -> Option<String> {
    let at = line.find("https://").or_else(|| line.find("http://"))?;
    let url: String = line[at..]
        .chars()
        .take_while(|c| !c.is_whitespace())
        .collect();
    Some(url.trim_end_matches(['.', ',', ')']).to_owned())
}

/// Who is signed in to a service, as its tool says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Account {
    /// Its tool isn't installed.
    Missing,
    /// No one.
    SignedOut,
    /// This account.
    SignedIn(String),
    /// Its tool didn't answer in time.
    NoAnswer,
}

/// How a device sign-in is going.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// Enter `code` at `url` to go on.
    Code {
        /// The one-time code.
        code: String,
        /// The page to enter it on.
        url: String,
    },
    /// It finished: the account signed in, or why it failed.
    Done(Result<String, String>),
}

/// The tools that sign git in, with the environment they run in.
///
/// # Examples
///
/// ```no_run
/// use harlo_core::hosting::{Account, Hosting, Service};
/// # async fn run() {
/// let hosting = Hosting::new();
/// if let Account::SignedIn(name) = hosting.account(Service::GitHub, "github.com").await {
///     println!("signed in as {name}");
/// }
/// # }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hosting {
    /// Variables set for every command, such as a scratch `HOME`.
    env: Vec<(String, String)>,
    /// The credential helper that keeps a Forgejo token, when not chosen
    /// from what git has.
    helper: Option<String>,
}

impl Hosting {
    /// The user's own tools and git config.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets an environment variable for every command run, such as `HOME`,
    /// `PATH` or `GIT_CONFIG_GLOBAL` for scratch ones.
    #[must_use]
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Keeps Forgejo tokens with git credential helper `helper`, such as
    /// `store`, instead of the desktop keyring when git has one.
    #[must_use]
    pub fn credential_helper(mut self, helper: impl Into<String>) -> Self {
        self.helper = Some(helper.into());
        self
    }

    /// Who is signed in to GitHub or GitLab at `host`. Forgejo has no tool
    /// to ask: it's always [`Account::SignedOut`] here.
    pub async fn account(&self, service: Service, host: &str) -> Account {
        let Some(mut command) = self.tool(service) else {
            return Account::Missing;
        };
        command.args(["auth", "status", "--hostname", host]);
        // Signed out, they exit with an error; what they say is all that
        // matters. One that hangs, on a keyring say, is stopped.
        match tokio::time::timeout(STATUS_TIMEOUT, self.run(command, None)).await {
            Ok(Ok(text) | Err(text)) => {
                account_in_status(&text, host).map_or(Account::SignedOut, Account::SignedIn)
            }
            Err(_) => Account::NoAnswer,
        }
    }

    /// Signs in to GitHub or GitLab at `host` with a one-time code, then
    /// sets git up to ask the tool. Dropping the stream stops the tool.
    #[must_use]
    pub fn sign_in(&self, service: Service, host: &str) -> BoxStream<'static, Progress> {
        let this = self.clone();
        let host = host.to_owned();
        let started = match (service, self.tool(service)) {
            (Service::Forgejo, _) | (_, None) => Err(format!(
                "{} isn't installed.",
                service.cli().unwrap_or("A tool")
            )),
            (Service::GitHub, Some(mut command)) => {
                command.args([
                    "auth",
                    "login",
                    "--hostname",
                    &host,
                    "--web",
                    "--git-protocol",
                    "https",
                    "--skip-ssh-key",
                ]);
                Ok(command)
            }
            (Service::GitLab, Some(mut command)) => {
                command.args([
                    "auth",
                    "login",
                    "--hostname",
                    &host,
                    "--device",
                    "--git-protocol",
                    "https",
                ]);
                Ok(command)
            }
        };
        let page = service.device_page(&host);
        // The tool starts when the stream is first polled, on the runtime
        // that runs it.
        stream::unfold(Some(Stage::Start(started)), move |stage| {
            let (this, host, page) = (this.clone(), host.clone(), page.clone());
            async move {
                let device = match stage? {
                    Stage::Start(Err(why)) => return Some((Progress::Done(Err(why)), None)),
                    Stage::Start(Ok(command)) => match Device::start(command) {
                        Ok(device) => device,
                        Err(why) => return Some((Progress::Done(Err(why)), None)),
                    },
                    Stage::Running(device) => device,
                };
                match device.next(&page).await {
                    (Step::Show(progress), device) => Some((progress, device.map(Stage::Running))),
                    (Step::Failed(why), _) => Some((Progress::Done(Err(why)), None)),
                    (Step::Finished, _) => Some((this.finish(service, &host).await, None)),
                }
            }
        })
        .boxed()
    }

    /// After the tool signed in: git set up to ask it, and who it says is
    /// signed in.
    async fn finish(&self, service: Service, host: &str) -> Progress {
        if let Err(why) = self.connect_git(service, host).await {
            return Progress::Done(Err(format!("Signed in, but setting up git failed: {why}")));
        }
        match self.account(service, host).await {
            Account::SignedIn(name) => Progress::Done(Ok(name)),
            Account::Missing | Account::SignedOut | Account::NoAnswer => {
                Progress::Done(Err("It finished, but no one is signed in.".to_owned()))
            }
        }
    }

    /// Signs out of GitHub or GitLab at `host`; `account` names which
    /// GitHub account, when more than one is signed in.
    ///
    /// # Errors
    ///
    /// Fails when the tool is missing or refuses.
    pub async fn sign_out(
        &self,
        service: Service,
        host: &str,
        account: &str,
    ) -> Result<(), String> {
        let mut command = self
            .tool(service)
            .ok_or_else(|| format!("{} isn't installed.", service.cli().unwrap_or("A tool")))?;
        command.args(["auth", "logout", "--hostname", host]);
        if service == Service::GitHub && !account.is_empty() {
            command.args(["--user", account]);
        }
        self.run(command, None).await.map(drop)
    }

    /// Checks `token` with the Forgejo server at `base`
    /// (`https://host[:port]`), then has git keep it for that server only.
    /// Returns the account it belongs to.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached or refuses the token, or git
    /// can't keep it.
    pub async fn forgejo_sign_in(&self, base: &str, token: &str) -> Result<String, String> {
        let token = token.trim();
        let login = forgejo_user(base, token).await?;
        let helper = match &self.helper {
            Some(helper) => helper.clone(),
            None if self.has_keyring().await => "libsecret".to_owned(),
            None => "store".to_owned(),
        };
        self.git(
            &[
                "config",
                "--global",
                "--replace-all",
                &format!("credential.{base}.helper"),
                &helper,
            ],
            None,
        )
        .await?;
        let (protocol, host) = base.split_once("://").unwrap_or(("https", base));
        self.git(
            &["credential", "approve"],
            Some(&format!(
                "protocol={protocol}\nhost={host}\nusername={login}\npassword={token}\n\n"
            )),
        )
        .await?;
        Ok(login)
    }

    /// Has git forget the token it keeps for `account` on the Forgejo
    /// server at `base`.
    ///
    /// # Errors
    ///
    /// Fails when git can't run.
    pub async fn forgejo_sign_out(&self, base: &str, account: &str) -> Result<(), String> {
        let (protocol, host) = base.split_once("://").unwrap_or(("https", base));
        self.git(
            &["credential", "reject"],
            Some(&format!(
                "protocol={protocol}\nhost={host}\nusername={account}\n\n"
            )),
        )
        .await
        .map(drop)
    }

    /// After a sign-in, git asks the service's tool for that host.
    async fn connect_git(&self, service: Service, host: &str) -> Result<(), String> {
        match service {
            Service::GitHub => {
                let mut command = self
                    .tool(service)
                    .ok_or_else(|| "The GitHub CLI isn't installed.".to_owned())?;
                command.args(["auth", "setup-git", "--hostname", host]);
                self.run(command, None).await.map(drop)
            }
            Service::GitLab => {
                let glab = self
                    .program("glab")
                    .ok_or_else(|| "glab isn't installed.".to_owned())?;
                let helper = format!("!{} auth git-credential", glab.display());
                self.git(
                    &[
                        "config",
                        "--global",
                        "--replace-all",
                        &format!("credential.https://{host}.helper"),
                        &helper,
                    ],
                    None,
                )
                .await
                .map(drop)
            }
            Service::Forgejo => Ok(()),
        }
    }

    /// Whether git has the desktop keyring's credential helper.
    async fn has_keyring(&self) -> bool {
        let Ok(exec) = self.git(&["--exec-path"], None).await else {
            return false;
        };
        Path::new(exec.trim())
            .join("git-credential-libsecret")
            .is_file()
    }

    /// A service's tool, quiet: no colours, no update notices, and no
    /// browser of its own (Harlo opens the page).
    fn tool(&self, service: Service) -> Option<Command> {
        let mut command = self.command(&self.program(service.cli()?)?);
        command
            .env("NO_COLOR", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_BROWSER", "true")
            .env("BROWSER", "true")
            .env("GLAB_CHECK_UPDATE", "false");
        Some(command)
    }

    /// A command with this environment, which never asks on a terminal.
    fn command(&self, program: &Path) -> Command {
        let mut command = Command::new(program);
        command
            .env("GIT_TERMINAL_PROMPT", "0")
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .kill_on_drop(true);
        command
    }

    /// Runs `git` with `args`, `input` on its standard input.
    async fn git(&self, args: &[&str], input: Option<&str>) -> Result<String, String> {
        let mut command = self.command(Path::new("git"));
        command
            .args(args)
            .current_dir(std::env::temp_dir())
            .env("GIT_ASKPASS", "true")
            .env("SSH_ASKPASS", "true");
        self.run(command, input).await
    }

    /// Runs `command`, `input` on its standard input: what it printed, or
    /// its last line when it fails.
    async fn run(&self, mut command: Command, input: Option<&str>) -> Result<String, String> {
        if input.is_some() {
            command.stdin(Stdio::piped());
        }
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Couldn't run it: {e}"))?;
        if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
            // A tool that stops reading early still says why below.
            let _ = stdin.write_all(text.as_bytes()).await;
        }
        let output = child
            .wait_with_output()
            .await
            .map_err(|e| format!("Couldn't run it: {e}"))?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if output.status.success() {
            Ok(text)
        } else {
            Err(last_line(&text).unwrap_or("It didn't finish.").to_owned())
        }
    }

    /// A program on the `PATH` (this one's, when set), or in the folders a
    /// desktop's `PATH` often leaves out.
    fn program(&self, name: &str) -> Option<PathBuf> {
        let var = |key: &str| {
            self.env
                .iter()
                .rev()
                .find(|(k, _)| k == key)
                .map(|(_, v)| std::ffi::OsString::from(v))
                .or_else(|| std::env::var_os(key))
        };
        let mut folders: Vec<PathBuf> = var("PATH")
            .map(|path| std::env::split_paths(&path).collect())
            .unwrap_or_default();
        if let Some(home) = var("HOME").map(PathBuf::from) {
            folders.extend([home.join(".local/bin"), home.join("bin")]);
        }
        folders
            .into_iter()
            .map(|folder| folder.join(name))
            .find(|path| path.is_file())
    }
}

/// The account a Forgejo `token` belongs to on the server at `base`.
async fn forgejo_user(base: &str, token: &str) -> Result<String, String> {
    let host = base.split("://").nth(1).unwrap_or(base);
    let client = reqwest::Client::builder()
        .timeout(API_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(format!("{}/api/v1/user", base.trim_end_matches('/')))
        .header("Authorization", format!("token {token}"))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Couldn't reach {host}: {e}"))?;
    match response.status().as_u16() {
        200 => {}
        401 | 403 => {
            return Err(format!(
                "{host} didn't accept the token. It needs the read:user scope, and \
                 repository access to push."
            ));
        }
        404 => {
            return Err(format!(
                "{host} doesn't look like a Forgejo or Gitea server."
            ));
        }
        status => return Err(format!("{host} answered {status}.")),
    }
    let user: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("{host} answered something else: {e}"))?;
    user["login"]
        .as_str()
        .filter(|login| !login.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("{host}'s answer has no account name."))
}

/// The last line of `text` with something on it.
fn last_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).rfind(|line| !line.is_empty())
}

/// Where a device sign-in is.
enum Stage {
    /// Its tool, to start, or why it can't.
    Start(Result<Command, String>),
    /// Its tool, running.
    Running(Device),
}

/// What a device sign-in's tool did next.
enum Step {
    /// Show this.
    Show(Progress),
    /// It failed, for this reason.
    Failed(String),
    /// It finished signing in.
    Finished,
}

/// A device sign-in's tool, running.
struct Device {
    child: Child,
    /// Kept open: the tool may wait for Enter before going on.
    stdin: Option<ChildStdin>,
    lines: BoxStream<'static, String>,
    /// Whether the code was shown.
    shown: bool,
    /// What it said, for its error.
    said: Vec<String>,
}

impl Device {
    /// Starts `command`, answering its "Press Enter" ahead.
    fn start(mut command: Command) -> Result<Self, String> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Couldn't run it: {e}"))?;
        let out = child.stdout.take().map(lines);
        let err = child.stderr.take().map(lines);
        let lines = stream::select(
            out.unwrap_or_else(|| stream::empty().boxed()),
            err.unwrap_or_else(|| stream::empty().boxed()),
        )
        .boxed();
        Ok(Self {
            stdin: child.stdin.take(),
            child,
            lines,
            shown: false,
            said: Vec::new(),
        })
    }

    /// Reads on until there's something to show, or the tool ends;
    /// `page` is where the code goes when the tool doesn't say.
    async fn next(mut self, page: &str) -> (Step, Option<Self>) {
        if let Some(stdin) = &mut self.stdin {
            // Its "Press Enter to open the page" goes on at once: Harlo
            // shows the page itself.
            let _ = stdin.write_all(b"\n").await;
        }
        while let Some(line) = self.lines.next().await {
            if line.is_empty() {
                continue;
            }
            let code = one_time_code(&line).filter(|_| !self.shown);
            self.said.push(line.clone());
            if let Some(code) = code {
                self.shown = true;
                let url = url_in(&line).unwrap_or_else(|| page.to_owned());
                return (Step::Show(Progress::Code { code, url }), Some(self));
            }
        }
        let ended = self.child.wait().await;
        let step = match ended {
            Ok(status) if status.success() => Step::Finished,
            _ => Step::Failed(self.why()),
        };
        (step, None)
    }

    /// Why it failed: its last line about an error, or its last line.
    fn why(&self) -> String {
        self.said
            .iter()
            .rev()
            .find(|line| {
                let line = line.to_lowercase();
                line.contains("error") || line.contains("fail")
            })
            .or_else(|| self.said.last())
            .cloned()
            .unwrap_or_else(|| "It didn't finish.".to_owned())
    }
}

/// The lines `pipe` prints, split at newlines and carriage returns.
fn lines(pipe: impl AsyncRead + Unpin + Send + 'static) -> BoxStream<'static, String> {
    stream::unfold(BufReader::new(pipe), |mut reader| async move {
        let mut bytes = Vec::new();
        match reader.read_until(b'\n', &mut bytes).await {
            Ok(0) | Err(_) => None,
            Ok(_) => {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                let parts: Vec<String> = text
                    .split(['\n', '\r'])
                    .map(|part| part.trim().to_owned())
                    .collect();
                Some((stream::iter(parts), reader))
            }
        }
    })
    .flatten()
    .boxed()
}
