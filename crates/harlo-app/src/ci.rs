//! A pull request's CI checks (gh / glab), and what failed, for the agent.
//! The Changes panel shows them; harlo watches a pull request's checks
//! while they run and notifies when they pass or fail (Workspace).

use std::process::Stdio;

use harlo_core::git_hosting::{self as hosting, Checks};

/// Runs a tool and keeps its output whatever its exit (gh exits non-zero
/// while checks fail or run).
fn output(mut c: std::process::Command) -> Option<String> {
    let out = c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The checks on a pull request (blocking); None when they can't be read.
pub fn read(service: &str, full: &str, url: &str, number: i64) -> Option<Checks> {
    // Forgejo: the statuses on the pull request's latest commit, by its API.
    if service == "forgejo" {
        let (host, repo) = full.split_once('/')?;
        let api = crate::setup::forgejo_api(host)?;
        let sha = api.pull_head(repo, number).ok()?;
        return api.checks(repo, &sha).ok();
    }
    let mut c = crate::setup::tool(service)?;
    match service {
        "github" => {
            c.args(["pr", "checks", url, "--json", "name,bucket,link"]);
            // (No checks at all: gh says so in words.)
            Some(hosting::gh_checks(&output(c)?).unwrap_or_default())
        }
        "gitlab" => {
            c.args(["mr", "view", &number.to_string(), "-R", full, "-F", "json"]);
            hosting::glab_checks(&output(c)?)
        }
        _ => None,
    }
}

/// What failed, as a message for the agent: each failed check, its page,
/// and (GitHub Actions) the end of its failed steps' log (blocking).
pub fn failure_report(service: &str, full: &str, word: &str, number: i64, url: &str, checks: &Checks) -> String {
    let names: Vec<&str> = checks.failing.iter().map(|(n, _)| n.as_str()).collect();
    let mut out = format!("The CI checks on {word} #{number} ({url}) failed: {}.\n", names.join(", "));
    let mut runs: Vec<String> = Vec::new();
    for (name, link) in &checks.failing {
        out.push_str(&format!("\n{name}: {link}\n"));
        let Some(run) = (service == "github").then(|| hosting::actions_run_id(link)).flatten() else { continue };
        if runs.contains(&run) || runs.len() >= 3 {
            continue;
        }
        runs.push(run.clone());
        let Some(mut c) = crate::setup::tool(service) else { continue };
        c.args(["run", "view", &run, "--log-failed", "-R", full]);
        if let Some(log) = output(c).filter(|l| !l.trim().is_empty()) {
            out.push_str(&format!("```\n{}\n```\n", hosting::log_tail(&log, 80)));
        }
    }
    out.push_str("\nFind out why they fail and fix it.");
    out
}

/// The branch's pull (merge) request on GitHub or GitLab, for the Settled
/// shelf (blocking): an open one, else the one merged or closed last.
/// Read from the branch's remote as git keeps it, so a branch deleted
/// there after merging still finds its request.
pub fn review(root: &std::path::Path, branch: &str) -> Option<harlo_core::shelf::Review> {
    use harlo_core::git;
    let remote = git::output_in(root, &["config", &format!("branch.{branch}.remote")]).trim().to_owned();
    let remote = if remote.is_empty() || remote == "." { "origin".to_owned() } else { remote };
    let merge = git::output_in(root, &["config", &format!("branch.{branch}.merge")]);
    let head = merge.trim().strip_prefix("refs/heads/").filter(|h| !h.is_empty()).unwrap_or(branch).to_owned();
    let url = git::output_in(root, &["remote", "get-url", &remote]).trim().to_owned();
    let (host, repo) = hosting::parse_remote(&url)?;
    let service = hosting::service_for(&host, &crate::setup::setting("gitlab_host"), &crate::setup::setting("forgejo_host"), &crate::setup::setting("git_host"));
    let full = format!("{host}/{repo}");
    let mut c = crate::setup::tool(&service)?;
    match service.as_str() {
        "github" => {
            c.args(["pr", "list", "--repo", &full, "--head", &head, "--state", "all", "--limit", "20", "--json", "state,mergedAt,closedAt"]);
            harlo_core::shelf::gh_review(&output(c)?)
        }
        "gitlab" => {
            c.args(["mr", "list", "--source-branch", &head, "--all", "-R", &full, "-F", "json"]);
            harlo_core::shelf::glab_review(&output(c)?)
        }
        _ => None,
    }
}
