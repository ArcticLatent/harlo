//! Git hosting services (GitHub, GitLab, Forgejo): who is signed in, read
//! from their command-line tools' words, and Forgejo's API for a token.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::Value;

/// The services harlo signs git in to, in the order it shows them.
pub const SERVICES: [&str; 3] = ["github", "gitlab", "forgejo"];

pub fn label(service: &str) -> &'static str {
    match service {
        "github" => "GitHub",
        "gitlab" => "GitLab",
        "forgejo" => "Forgejo",
        _ => "",
    }
}

/// Where a service is unless one is set: GitHub's, GitLab's, Codeberg (the
/// largest Forgejo).
pub fn default_host(service: &str) -> &'static str {
    match service {
        "github" => "github.com",
        "gitlab" => "gitlab.com",
        "forgejo" => "codeberg.org",
        _ => "",
    }
}

/// The command-line tool that signs in to it (Forgejo has none: a token).
pub fn cli(service: &str) -> Option<&'static str> {
    match service {
        "github" => Some("gh"),
        "gitlab" => Some("glab"),
        _ => None,
    }
}

/// A host as typed ("https://git.example.org/", "git.example.org") → its
/// name ("git.example.org"); "" when it isn't one.
pub fn normalize_host(typed: &str) -> String {
    let t = typed.trim();
    let t = t.strip_prefix("https://").or_else(|| t.strip_prefix("http://")).unwrap_or(t);
    let host = t.split('/').next().unwrap_or_default().trim().to_lowercase();
    // (A server on the local network may have no dots: "forgejo:3000".)
    let ok = !host.is_empty() && host.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) && host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'));
    if ok { host } else { String::new() }
}

/// The account `gh auth status` / `glab auth status` says is signed in to
/// `host`: "Logged in to github.com account octocat (keyring)", "Logged in
/// to gitlab.com as octocat (keyring)".
pub fn account_in_status(text: &str, host: &str) -> Option<String> {
    for line in text.lines() {
        let Some(at) = line.find("Logged in to ") else { continue };
        let rest = &line[at + "Logged in to ".len()..];
        let Some(rest) = rest.strip_prefix(host) else { continue };
        let rest = rest.trim_start();
        let rest = rest.strip_prefix("account ").or_else(|| rest.strip_prefix("as "))?;
        let name: String = rest.chars().take_while(|c| !c.is_whitespace() && *c != '(').collect();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

/// The one-time code a device sign-in shows ("First copy your one-time
/// code: 1A2B-3C4D", "Enter the code 0A44L90H"): a word of capitals and
/// digits (a dash allowed) on a line that mentions a code.
pub fn one_time_code(line: &str) -> Option<String> {
    if !line.to_lowercase().contains("code") {
        return None;
    }
    line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter(|w| (6..=12).contains(&w.len()))
        .find(|w| {
            w.chars().all(|c| c == '-' || c.is_ascii_uppercase() || c.is_ascii_digit())
                && w.chars().any(|c| c.is_ascii_digit())
                && !w.starts_with('-')
                && !w.ends_with('-')
        })
        .map(str::to_owned)
}

/// A Forgejo (or Gitea) server's web address as set in Settings → Git:
/// `https://host[:port]`, or `http://…` when it was typed so (a server on
/// the local network without TLS).
pub fn forgejo_base(typed: &str) -> String {
    let host = normalize_host(typed);
    if typed.trim().to_lowercase().starts_with("http://") { format!("http://{host}") } else { format!("https://{host}") }
}

/// A Forgejo (or Gitea) server's API with a token: `base` is
/// `https://host[:port]` (or `http://`). TLS is checked against the system's
/// certificates, so a company's own certificate authority, once installed
/// on the computer, is trusted.
pub struct Forgejo {
    pub base: String,
    token: String,
}

impl Forgejo {
    pub fn new(base: &str, token: &str) -> Self {
        Self { base: base.trim_end_matches('/').to_owned(), token: token.trim().to_owned() }
    }

    fn agent() -> ureq::Agent {
        let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
        ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(20))).http_status_as_error(false).tls_config(tls).build().into()
    }

    fn host(&self) -> &str {
        self.base.split("://").nth(1).unwrap_or(&self.base)
    }

    /// A call to `/api/v1{path}`: its status and JSON.
    fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<(u16, Value)> {
        let url = format!("{}/api/v1{path}", self.base);
        let agent = Self::agent();
        let auth = format!("token {}", self.token);
        let mut resp = match (method, body) {
            ("POST", Some(b)) => agent.post(&url).header("Authorization", &auth).header("Accept", "application/json").header("Content-Type", "application/json").send(b.to_string()),
            _ => agent.get(&url).header("Authorization", &auth).header("Accept", "application/json").call(),
        }
        .with_context(|| format!("Couldn't reach {}", self.host()))?;
        let status = resp.status().as_u16();
        let text = resp.body_mut().read_to_string().unwrap_or_default();
        Ok((status, serde_json::from_str(&text).unwrap_or(Value::Null)))
    }

    /// The account the token belongs to (it checks the token too).
    pub fn user(&self) -> Result<String> {
        let (status, v) = self.call("GET", "/user", None)?;
        match status {
            200 => {}
            401 | 403 => bail!("{} didn't accept the token. It needs the read:user scope (and repository access to push).", self.host()),
            404 => bail!("{} doesn't look like a Forgejo or Gitea server (no /api/v1/user).", self.host()),
            s => bail!("{} answered {s}.", self.host()),
        }
        v["login"].as_str().filter(|l| !l.is_empty()).map(str::to_owned).context("Its answer has no account name")
    }

    /// The open pull request from branch `head` of `repo` ("owner/name"):
    /// its page and number.
    pub fn open_pull(&self, repo: &str, head: &str) -> Result<Option<(String, i64)>> {
        let (status, v) = self.call("GET", &format!("/repos/{repo}/pulls?state=open&limit=50"), None)?;
        if status != 200 {
            bail!("{} answered {status} for {repo}'s pull requests.", self.host());
        }
        Ok(v.as_array().into_iter().flatten()
            .find(|p| p["head"]["ref"].as_str() == Some(head) && p["head"]["repo"]["full_name"].as_str().is_none_or(|f| f == repo))
            .map(|p| (p["html_url"].as_str().unwrap_or_default().to_owned(), p["number"].as_i64().unwrap_or(0))))
    }

    /// A pull request's latest commit.
    pub fn pull_head(&self, repo: &str, number: i64) -> Result<String> {
        let (status, v) = self.call("GET", &format!("/repos/{repo}/pulls/{number}"), None)?;
        if status != 200 {
            bail!("{} answered {status} for pull request #{number}.", self.host());
        }
        v["head"]["sha"].as_str().map(str::to_owned).context("Its answer has no commit")
    }

    /// Opens a pull request (a draft: "WIP:" before its title, as Forgejo
    /// marks drafts): its page.
    pub fn create_pull(&self, repo: &str, head: &str, base: &str, title: &str, body: &str, draft: bool) -> Result<String> {
        let title = if draft { format!("WIP: {title}") } else { title.to_owned() };
        let (status, v) = self.call("POST", &format!("/repos/{repo}/pulls"), Some(&serde_json::json!({ "head": head, "base": base, "title": title, "body": body })))?;
        match status {
            200 | 201 => v["html_url"].as_str().map(str::to_owned).context("Its answer has no address"),
            409 => bail!("There's already a pull request from {head}."),
            401 | 403 => bail!("{} didn't let this account open a pull request in {repo}.", self.host()),
            _ => bail!("{}", v["message"].as_str().map_or_else(|| format!("{} answered {status}.", self.host()), str::to_owned)),
        }
    }

    /// The checks on a branch or commit: its commit statuses (Forgejo
    /// Actions, Woodpecker, Drone…, as each reports its result).
    pub fn checks(&self, repo: &str, reference: &str) -> Result<Checks> {
        let (status, v) = self.call("GET", &format!("/repos/{repo}/commits/{reference}/status"), None)?;
        if status != 200 {
            bail!("{} answered {status} for the checks.", self.host());
        }
        Ok(forgejo_checks(&v))
    }
}

/// From a Forgejo combined status (`statuses`: context, status, target_url).
pub fn forgejo_checks(v: &Value) -> Checks {
    let mut c = Checks::default();
    for s in v["statuses"].as_array().into_iter().flatten() {
        match s["status"].as_str().or_else(|| s["state"].as_str()).unwrap_or_default() {
            "success" | "warning" => c.passed += 1,
            "failure" | "error" => {
                c.failed += 1;
                c.failing.push((s["context"].as_str().unwrap_or("check").to_owned(), s["target_url"].as_str().unwrap_or_default().to_owned()));
            }
            "pending" => c.pending += 1,
            _ => {}
        }
    }
    c
}

/// The Forgejo account a token belongs to on `base` (see `Forgejo::user`).
pub fn forgejo_user(base: &str, token: &str) -> Result<String> {
    Forgejo::new(base, token).user()
}

/// A remote's address → its host and repository path: "git@github.com:
/// octo/app.git", "ssh://git@host:2222/group/sub/app.git",
/// "https://user@host/octo/app" → ("host", "group/sub/app").
pub fn parse_remote(url: &str) -> Option<(String, String)> {
    let u = url.trim();
    let (host, path) = if let Some(rest) = u.strip_prefix("https://").or_else(|| u.strip_prefix("http://")).or_else(|| u.strip_prefix("ssh://")).or_else(|| u.strip_prefix("git://")) {
        let rest = rest.split_once('@').filter(|(user, _)| !user.contains('/')).map_or(rest, |(_, r)| r);
        let (host, path) = rest.split_once('/')?;
        // (An SSH port isn't the web's.)
        let host = if u.starts_with("ssh://") { host.split(':').next().unwrap_or(host) } else { host };
        (host.to_owned(), path.to_owned())
    } else {
        // scp-like: [user@]host:path
        let (left, path) = u.split_once(':')?;
        let host = left.rsplit('@').next().unwrap_or(left);
        (host.to_owned(), path.to_owned())
    };
    let path = path.trim_matches('/').trim_end_matches(".git").trim_matches('/').to_owned();
    (!host.is_empty() && path.contains('/')).then(|| (host.to_lowercase(), path))
}

/// The service a host is: GitHub's and GitLab's own, the GitLab and Forgejo
/// servers set in Settings → Git, Codeberg; else `default` (Settings → Git's
/// default service, for a self-hosted one).
pub fn service_for(host: &str, gitlab_host: &str, forgejo_host: &str, default: &str) -> String {
    let h = host.to_lowercase();
    let same = |set: &str| !set.is_empty() && normalize_host(set).split(':').next() == h.split(':').next();
    if h == "github.com" || h.ends_with(".github.com") {
        "github".into()
    } else if h == "gitlab.com" || same(gitlab_host) {
        "gitlab".into()
    } else if h == "codeberg.org" || same(forgejo_host) {
        "forgejo".into()
    } else if SERVICES.contains(&default) {
        default.into()
    } else {
        "github".into()
    }
}

/// The site's page for a new pull request (merge request on GitLab) from
/// `head` into `base`.
pub fn new_request_url(service: &str, host: &str, repo: &str, base: &str, head: &str) -> String {
    let enc = |s: &str| s.replace('%', "%25").replace(' ', "%20").replace('#', "%23").replace('&', "%26").replace('?', "%3F");
    match service {
        "gitlab" => format!("https://{host}/{repo}/-/merge_requests/new?merge_request%5Bsource_branch%5D={}&merge_request%5Btarget_branch%5D={}", enc(head), enc(base)),
        "forgejo" => format!("https://{host}/{repo}/compare/{}...{}", enc(base), enc(head)),
        _ => format!("https://{host}/{repo}/compare/{}...{}?expand=1", enc(base), enc(head)),
    }
}

/// What a pull request is called there: "pull request", "merge request".
pub fn request_word(service: &str) -> &'static str {
    if service == "gitlab" { "merge request" } else { "pull request" }
}

/// A pull request's CI checks, in all: none, still running, all passed,
/// or some failed (their names and pages).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Checks {
    pub passed: usize,
    pub failed: usize,
    pub pending: usize,
    /// Failed checks: name and page.
    pub failing: Vec<(String, String)>,
}

impl Checks {
    /// "none", "pending", "failing" (some failed; others may still run), "passing".
    pub fn state(&self) -> &'static str {
        if self.failed > 0 {
            "failing"
        } else if self.pending > 0 {
            "pending"
        } else if self.passed > 0 {
            "passing"
        } else {
            "none"
        }
    }

    /// Finished: nothing left running.
    pub fn settled(&self) -> bool {
        self.pending == 0 && self.passed + self.failed > 0
    }

    /// "3 of 5 checks passed, 1 running", "All 5 checks passed", "2 failed: build, lint".
    pub fn summary(&self) -> String {
        let total = self.passed + self.failed + self.pending;
        let checks = |n: usize| if n == 1 { "check" } else { "checks" };
        match self.state() {
            "failing" => {
                let names: Vec<&str> = self.failing.iter().map(|(n, _)| n.as_str()).take(3).collect();
                let more = if self.failing.len() > 3 { format!(" and {} more", self.failing.len() - 3) } else { String::new() };
                format!("{} of {total} {} failed: {}{more}", self.failed, checks(total), names.join(", "))
            }
            "pending" => format!("{} of {total} {} passed, {} running", self.passed, checks(total), self.pending),
            "passing" if total == 1 => "The check passed".into(),
            "passing" => format!("All {total} checks passed"),
            _ => "No checks".into(),
        }
    }

    pub fn to_json(&self) -> Value {
        serde_json::json!({ "state": self.state(), "summary": self.summary(), "passed": self.passed, "failed": self.failed, "pending": self.pending,
                            "failing": self.failing.iter().map(|(n, l)| serde_json::json!({ "name": n, "link": l })).collect::<Vec<_>>() })
    }
}

/// From `gh pr checks --json name,bucket,link` (bucket: pass, fail,
/// pending, skipping, cancel).
pub fn gh_checks(json: &str) -> Option<Checks> {
    let list: Vec<Value> = serde_json::from_str(json.trim()).ok()?;
    let mut c = Checks::default();
    for check in &list {
        match check["bucket"].as_str().unwrap_or_default() {
            "pass" => c.passed += 1,
            "fail" | "cancel" => {
                c.failed += 1;
                c.failing.push((check["name"].as_str().unwrap_or("check").to_owned(), check["link"].as_str().unwrap_or_default().to_owned()));
            }
            "pending" => c.pending += 1,
            _ => {}
        }
    }
    Some(c)
}

/// From `glab mr view -F json`: its latest pipeline, as one check.
pub fn glab_checks(json: &str) -> Option<Checks> {
    let v: Value = serde_json::from_str(json.trim()).ok()?;
    let p = v.get("head_pipeline").filter(|p| p.is_object()).or_else(|| v.get("pipeline").filter(|p| p.is_object()));
    let mut c = Checks::default();
    let Some(p) = p else { return Some(c) };
    let link = p["web_url"].as_str().unwrap_or_default().to_owned();
    match p["status"].as_str().unwrap_or_default() {
        "success" => c.passed = 1,
        "failed" | "canceled" => {
            c.failed = 1;
            c.failing.push(("pipeline".into(), link));
        }
        "skipped" | "manual" | "" => {}
        _ => c.pending = 1,
    }
    Some(c)
}

/// A GitHub Actions run's id, from a check's page
/// (`…/actions/runs/<id>/job/<job>`).
pub fn actions_run_id(link: &str) -> Option<String> {
    let rest = link.split("/actions/runs/").nth(1)?;
    let id: String = rest.chars().take_while(char::is_ascii_digit).collect();
    (!id.is_empty()).then_some(id)
}

/// The last `max_lines` lines of a failed run's log, without the
/// timestamps gh puts on each.
pub fn log_tail(log: &str, max_lines: usize) -> String {
    let lines: Vec<String> = log
        .lines()
        .map(|l| {
            // `job\tstep\t2026-10-16T09:00:00.0000000Z message`
            let msg = l.rsplit('\t').next().unwrap_or(l);
            match msg.split_once(' ') {
                Some((ts, rest)) if ts.len() > 20 && ts.as_bytes().get(4) == Some(&b'-') && ts.ends_with('Z') => rest.to_owned(),
                _ => msg.to_owned(),
            }
        })
        .collect();
    lines[lines.len().saturating_sub(max_lines)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks() {
        let c = gh_checks(r#"[{"name":"build","bucket":"pass","link":"l1"},{"name":"lint","bucket":"fail","link":"https://github.com/o/r/actions/runs/123/job/9"},
                               {"name":"test","bucket":"pending"},{"name":"docs","bucket":"skipping"}]"#).unwrap();
        assert_eq!((c.passed, c.failed, c.pending, c.state()), (1, 1, 1, "failing"));
        assert_eq!(c.summary(), "1 of 3 checks failed: lint");
        assert!(!c.settled());
        assert_eq!(actions_run_id(&c.failing[0].1).as_deref(), Some("123"));
        assert_eq!(actions_run_id("https://ci.example/x"), None);
        let running = gh_checks(r#"[{"name":"a","bucket":"pass"},{"name":"b","bucket":"pending"}]"#).unwrap();
        assert_eq!((running.state(), running.summary().as_str()), ("pending", "1 of 2 checks passed, 1 running"));
        let ok = gh_checks(r#"[{"name":"a","bucket":"pass"},{"name":"b","bucket":"pass"}]"#).unwrap();
        assert_eq!((ok.state(), ok.summary().as_str(), ok.settled()), ("passing", "All 2 checks passed", true));
        assert_eq!(gh_checks("[]").unwrap().state(), "none");
        assert!(gh_checks("no checks reported on the 'x' branch").is_none());
        let gl = glab_checks(r#"{"iid":3,"head_pipeline":{"status":"failed","web_url":"https://gitlab.com/o/r/-/pipelines/5"}}"#).unwrap();
        assert_eq!((gl.state(), gl.failing[0].1.as_str()), ("failing", "https://gitlab.com/o/r/-/pipelines/5"));
        assert_eq!(glab_checks(r#"{"head_pipeline":{"status":"running"}}"#).unwrap().state(), "pending");
        assert_eq!(glab_checks(r#"{"head_pipeline":null}"#).unwrap().state(), "none");
        assert_eq!(log_tail("build\tRun tests\t2026-10-16T09:00:00.1234567Z error: nope\nplain\nlast", 2), "plain\nlast");
        assert_eq!(log_tail("build\tRun tests\t2026-10-16T09:00:00.1234567Z error: nope", 5), "error: nope");
        let fj = forgejo_checks(&serde_json::json!({ "state": "failure", "statuses": [
            { "context": "ci / build (push)", "status": "success", "target_url": "https://git.lan/o/r/actions/runs/3" },
            { "context": "ci / lint (push)", "status": "failure", "target_url": "https://git.lan/o/r/actions/runs/3/jobs/1" },
            { "context": "woodpecker", "status": "pending" } ] }));
        assert_eq!((fj.passed, fj.failed, fj.pending, fj.state()), (1, 1, 1, "failing"));
        assert_eq!(fj.failing[0], ("ci / lint (push)".to_owned(), "https://git.lan/o/r/actions/runs/3/jobs/1".to_owned()));
        assert_eq!(forgejo_checks(&serde_json::json!({ "statuses": [] })).state(), "none");
        assert_eq!(forgejo_base("http://forgejo:3000/"), "http://forgejo:3000");
        assert_eq!(forgejo_base("git.company.lan:3000"), "https://git.company.lan:3000");
        assert_eq!(normalize_host("forgejo:3000"), "forgejo:3000", "a LAN name without dots");
    }

    #[test]
    fn hosts() {
        assert_eq!(normalize_host("https://Codeberg.org/"), "codeberg.org");
        assert_eq!(normalize_host("git.example.org:3000/user"), "git.example.org:3000");
        assert_eq!(normalize_host(" gitlab.com "), "gitlab.com");
        assert_eq!(normalize_host("not a host"), "");
        assert_eq!(normalize_host(""), "");
    }

    #[test]
    fn who_is_signed_in() {
        let gh = "github.com\n  ✓ Logged in to github.com account octocat (keyring)\n  - Active account: true\n";
        assert_eq!(account_in_status(gh, "github.com").as_deref(), Some("octocat"));
        assert_eq!(account_in_status("✓ Logged in to github.com as hubot (oauth_token)", "github.com").as_deref(), Some("hubot"));
        let glab = "gitlab.com\n  ✓ Logged in to gitlab.com as tanuki (keyring)\n  ✓ Git operations for gitlab.com configured to use https protocol.";
        assert_eq!(account_in_status(glab, "gitlab.com").as_deref(), Some("tanuki"));
        assert_eq!(account_in_status(glab, "gitlab.example.org"), None);
        assert_eq!(account_in_status("You are not logged into any GitHub hosts. To log in, run: gh auth login", "github.com"), None);
    }

    #[test]
    fn remotes() {
        assert_eq!(parse_remote("git@github.com:octo/app.git"), Some(("github.com".into(), "octo/app".into())));
        assert_eq!(parse_remote("https://github.com/octo/app"), Some(("github.com".into(), "octo/app".into())));
        assert_eq!(parse_remote("https://user@gitlab.com/group/sub/app.git"), Some(("gitlab.com".into(), "group/sub/app".into())));
        assert_eq!(parse_remote("ssh://git@codeberg.org:2222/me/thing.git"), Some(("codeberg.org".into(), "me/thing".into())));
        assert_eq!(parse_remote("/srv/git/app.git"), None);
        assert_eq!(parse_remote("../app"), None);
        assert_eq!(service_for("github.com", "", "", "forgejo"), "github");
        assert_eq!(service_for("git.example.org", "", "https://git.example.org/", "github"), "forgejo");
        assert_eq!(service_for("code.corp", "code.corp", "", "github"), "gitlab");
        assert_eq!(service_for("own.server", "", "", "forgejo"), "forgejo");
        assert_eq!(new_request_url("github", "github.com", "octo/app", "main", "fix/a b"), "https://github.com/octo/app/compare/main...fix/a%20b?expand=1");
        assert!(new_request_url("gitlab", "gitlab.com", "g/app", "main", "x").contains("/-/merge_requests/new?merge_request%5Bsource_branch%5D=x"));
    }

    #[test]
    fn codes() {
        assert_eq!(one_time_code("! First copy your one-time code: 1A2B-3C4D").as_deref(), Some("1A2B-3C4D"));
        assert_eq!(one_time_code("Enter the code 0A44L90H at https://gitlab.com/oauth/device").as_deref(), Some("0A44L90H"));
        assert_eq!(one_time_code("Open this URL to continue in your web browser: https://github.com/login/device"), None);
        assert_eq!(one_time_code("Authentication complete."), None);
        assert_eq!(one_time_code("code: lowercase-word"), None);
    }
}
