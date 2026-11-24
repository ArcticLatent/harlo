//! What a phone asks of a session beyond following it: the changes in its
//! folder and their diffs, its files and branches, what its agent did and
//! the whole conversation as Markdown, undoing or editing one of the user's
//! messages, its waiting messages and how much it may do without asking.
//!
//! Each answer is JSON as the phone app reads it, or `{ error }`.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;

use harlo_client::Delivery;
use serde_json::{Value, json};

use super::Server;
use crate::approvals::Shield;
use crate::git::{Branch, Repo};
use crate::timeline::{self, Entry};
use crate::workspace::ToolStatus;

/// Lines of a diff or a file a phone gets, at most.
const MAX_LINES: usize = 5000;

/// Files a phone's `@` search lists, at most.
const MAX_FILES: usize = 30;

/// The approval modes a phone offers, as the phone names them, with the
/// desktop's mode each stands for.
const MODES: [(&str, Shield); 3] = [
    ("always-ask", Shield::Careful),
    ("write", Shield::Agent),
    ("yolo", Shield::Trusted),
];

/// Answers request `op` about session `key`, with the request `m`;
/// `None` when it's none of these.
pub(super) async fn answer(op: &str, server: &Server, key: &str, m: &Value) -> Option<Value> {
    let text = |name: &str| m[name].as_str().unwrap_or_default();
    Some(match op {
        "changes" => changes(server, key).await,
        "diff" => diff(server, key, text("path")).await,
        "files" => files(server, key, text("query")).await,
        "file" => file(server, key, text("path")).await,
        "branches" => branches(server, key).await,
        "switchBranch" => switch_branch(server, key, m).await,
        "markdown" => markdown(server, key).await,
        "actions" => actions(server, key).await,
        "modes" => modes(server, key),
        "setModes" => set_modes(server, key, m).await,
        "unqueue" => unqueue(server, key, m).await,
        "promote" => promote(server, key, m).await,
        "undoTargets" => undo_targets(server, key).await,
        "undoPlan" | "undo" => undo(server, key, text("row"), op == "undo").await,
        "branch" => branch(server, key, text("row"), m["fork"].as_bool() == Some(true)).await,
        _ => return None,
    })
}

fn error(text: impl Into<String>) -> Value {
    json!({ "error": text.into() })
}

/// The folder session `key` works in.
fn folder(server: &Server, key: &str) -> Result<String, Value> {
    super::lock(&server.workspace)
        .session(key)
        .map(|s| s.directory.clone())
        .ok_or_else(|| error("That session is gone."))
}

/// The session's folder as a repository, when Harlo runs on the computer
/// that holds it.
fn repo(server: &Server, key: &str) -> Result<Repo, Value> {
    if !server.local {
        return Err(error(
            "Harlo's server runs on another computer; its folders are seen there.",
        ));
    }
    folder(server, key).map(Repo::new)
}

/// The changed files in the session's repository, with its branch.
async fn changes(server: &Server, key: &str) -> Value {
    let repo = match repo(server, key) {
        Ok(repo) => repo,
        Err(e) => return e,
    };
    match repo.status().await {
        Ok(status) => {
            let files: Vec<Value> = status
                .files
                .iter()
                .map(|f| {
                    json!({
                        "path": f.path, "from": f.from.clone().unwrap_or_default(),
                        "status": f.code.to_string(), "staged": f.staged,
                        "untracked": f.code == '?', "conflicted": f.code == 'U',
                    })
                })
                .collect();
            json!({
                "branch": status.branch.unwrap_or_default(),
                "ahead": status.ahead, "behind": status.behind, "files": files,
            })
        }
        Err(_) => error("This session's folder isn't a git repository."),
    }
}

/// One changed file's diff, `path` relative to the repository's root.
async fn diff(server: &Server, key: &str, file_path: &str) -> Value {
    let path = file_path;
    let repo = match repo(server, key) {
        Ok(repo) => repo,
        Err(e) => return e,
    };
    let Ok(status) = repo.status().await else {
        return error("This session's folder isn't a git repository.");
    };
    let Some(file) = status.files.iter().find(|f| f.path == path) else {
        return error("That file has no changes now.");
    };
    let unified = if file.code == '?' {
        // Untracked: all of it is new.
        let Ok(root) = repo.root().await else {
            return error("This session's folder isn't a git repository.");
        };
        match tokio::fs::read(root.join(path)).await {
            Ok(bytes) if bytes.contains(&0) => return binary(path),
            Ok(bytes) => crate::diff::written(path, &String::from_utf8_lossy(&bytes)).patch,
            Err(e) => return error(format!("Couldn't read {path}: {e}")),
        }
    } else {
        match repo.diff_file(path).await {
            Ok(text) => text,
            Err(e) => return error(e.to_string()),
        }
    };
    if unified.lines().any(|l| l.starts_with("Binary files ")) {
        return binary(path);
    }
    diff_json(path, &unified)
}

fn binary(path: &str) -> Value {
    json!({ "path": path, "binary": true, "added": 0, "removed": 0, "hunks": [] })
}

/// A unified diff as the phone reads it: hunks of numbered lines.
#[must_use]
pub fn diff_json(file: &str, unified: &str) -> Value {
    let mut hunks: Vec<Value> = Vec::new();
    let (mut added, mut removed, mut shown) = (0_u64, 0_u64, 0_usize);
    let (mut old, mut new) = (0_u64, 0_u64);
    let mut cut = false;
    for line in unified.lines() {
        if let Some(header) = line.strip_prefix("@@") {
            let (o, n) = hunk_start(header);
            (old, new) = (o, n);
            hunks.push(json!({ "header": line, "lines": [] }));
            continue;
        }
        let Some(hunk) = hunks.last_mut() else {
            continue; // The file headers.
        };
        let (kind, text) = line.split_at(line.len().min(1));
        let row = match kind {
            "+" => {
                added += 1;
                new += 1;
                json!({ "k": "+", "n": new - 1, "t": text })
            }
            "-" => {
                removed += 1;
                old += 1;
                json!({ "k": "-", "o": old - 1, "t": text })
            }
            "\\" => json!({ "k": "\\", "t": text.trim_start() }),
            _ => {
                old += 1;
                new += 1;
                json!({ "k": " ", "o": old - 1, "n": new - 1, "t": text })
            }
        };
        if shown < MAX_LINES {
            shown += 1;
            if let Some(lines) = hunk["lines"].as_array_mut() {
                lines.push(row);
            }
        } else {
            cut = true;
        }
    }
    json!({
        "path": file, "binary": false, "added": added, "removed": removed,
        "hunks": hunks, "cut": cut,
    })
}

/// The first old and new line numbers of a hunk header's ` -a,b +c,d @@`.
fn hunk_start(header: &str) -> (u64, u64) {
    let number = |sign: char| {
        header
            .split_whitespace()
            .find_map(|w| w.strip_prefix(sign))
            .and_then(|w| w.split(',').next())
            .and_then(|n| n.parse().ok())
            .unwrap_or(1)
    };
    (number('-'), number('+'))
}

/// The session folder's files matching `query`, best first.
async fn files(server: &Server, key: &str, query: &str) -> Value {
    let directory = match folder(server, key) {
        Ok(d) => d,
        Err(e) => return e,
    };
    match server.client.find_files(&directory, query, MAX_FILES).await {
        Ok(found) => {
            let paths: Vec<String> = found.into_iter().map(|f| f.path).collect();
            json!({ "files": paths })
        }
        Err(e) => error(e.to_string()),
    }
}

/// A file of the session's folder to read: its lines, as HTML text.
async fn file(server: &Server, key: &str, path: &str) -> Value {
    let directory = match folder(server, key) {
        Ok(d) => d,
        Err(e) => return e,
    };
    let bytes = match server.client.read_file(&directory, path).await {
        Ok(bytes) => bytes,
        Err(e) => return error(format!("Couldn't open {path}: {e}")),
    };
    if bytes.contains(&0) {
        return error(format!("{path} isn't text."));
    }
    let text = String::from_utf8_lossy(&bytes);
    let total = text.lines().count();
    let lines: Vec<String> = text.lines().take(MAX_LINES).map(escape).collect();
    json!({ "path": path, "lines": lines, "total": total, "cut": total > MAX_LINES })
}

/// `text` with `&`, `<` and `>` escaped, as the phone reads code lines.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
    out
}

/// The repository's branches, and how many files have changes.
async fn branches(server: &Server, key: &str) -> Value {
    let repo = match repo(server, key) {
        Ok(repo) => repo,
        Err(e) => return e,
    };
    let Ok(branches) = repo.branches().await else {
        return error("This session's folder isn't a git repository.");
    };
    let changed = repo.status().await.map_or(0, |s| s.files.len());
    let list = |list: &[Branch]| -> Vec<Value> {
        list.iter()
            .map(|b| json!({ "name": b.name, "upstream": b.upstream, "ahead": b.ahead, "date": b.date }))
            .collect()
    };
    json!({
        "current": branches.current.unwrap_or_default(),
        "local": list(&branches.local),
        "remote": list(&branches.remote),
        "changed": changed,
    })
}

/// Switches the repository to branch `m.name`; what Git did, in words.
async fn switch_branch(server: &Server, key: &str, m: &Value) -> Value {
    let repo = match repo(server, key) {
        Ok(repo) => repo,
        Err(e) => return e,
    };
    let name = m["name"].as_str().unwrap_or_default();
    let remote = m["remote"].as_bool() == Some(true);
    let stash = m["stash"].as_bool() == Some(true);
    match repo.switch(name, remote, stash).await {
        Ok(said) => json!({ "ok": true, "message": said }),
        Err(e) => error(e.to_string()),
    }
}

/// The session's timeline, read from the server.
async fn entries(server: &Server, key: &str) -> Result<Vec<Entry>, Value> {
    server
        .client
        .messages(key)
        .await
        .map(|messages| timeline::entries(&messages))
        .map_err(|e| error(e.to_string()))
}

/// The whole conversation as Markdown.
async fn markdown(server: &Server, key: &str) -> Value {
    let entries = match entries(server, key).await {
        Ok(e) => e,
        Err(e) => return e,
    };
    let title = super::lock(&server.workspace)
        .session(key)
        .and_then(|s| s.title.clone())
        .unwrap_or_default();
    let mut out = String::new();
    if !title.is_empty() {
        let _ = writeln!(out, "# {title}\n");
    }
    for entry in &entries {
        match entry {
            Entry::Prompt { text, .. } => {
                let quoted: Vec<String> = text.lines().map(|l| format!("> {l}")).collect();
                let _ = writeln!(out, "**You**\n\n{}\n", quoted.join("\n"));
            }
            Entry::Answer { text, .. } => {
                let _ = writeln!(out, "{}\n", text.trim_end());
            }
            Entry::Tool { label, detail, .. } => {
                let _ = writeln!(out, "- *{label}* `{}`\n", detail.replace('`', "'"));
            }
            Entry::Subagent { agent, task, .. } => {
                let _ = writeln!(out, "- *Subagent {agent}*: {task}\n");
            }
            Entry::Shell { run, .. } => {
                let _ = writeln!(
                    out,
                    "```sh\n$ {}\n{}\n```\n",
                    run.command,
                    run.output.trim_end()
                );
            }
            _ => {}
        }
    }
    json!({ "markdown": out.trim_end() })
}

/// What kind of action the phone shows a tool call as.
fn action_kind(tool: &str) -> &'static str {
    match tool {
        "shell" | "execute" | "bash" => "command",
        "edit" | "apply_patch" => "edit",
        "write" => "write",
        "read" => "read",
        "glob" | "grep" | "search_sessions" => "search",
        "webfetch" | "websearch" => "web",
        "subagent" => "agent",
        _ => "other",
    }
}

/// What the session's agent did: each tool call, newest last, with counts.
async fn actions(server: &Server, key: &str) -> Value {
    let entries = match entries(server, key).await {
        Ok(e) => e,
        Err(e) => return e,
    };
    let mut actions = Vec::new();
    let (mut commands, mut failed) = (0, 0);
    let (mut changed, mut read): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    let note = |list: &mut Vec<String>, path: &str| {
        if !path.is_empty() && !list.iter().any(|p| p == path) {
            list.push(path.to_owned());
        }
    };
    for entry in &entries {
        let (kind, tool, target, status, result) = match entry {
            Entry::Tool {
                name,
                label,
                detail,
                status,
                files,
                error,
                ..
            } => {
                let kind = action_kind(name);
                match kind {
                    "edit" | "write" => {
                        for f in files {
                            note(&mut changed, &f.file);
                        }
                        if files.is_empty() {
                            note(&mut changed, detail);
                        }
                    }
                    "read" => note(&mut read, detail),
                    _ => {}
                }
                let result = error.clone().unwrap_or_else(|| {
                    let (a, r) = files
                        .iter()
                        .fold((0, 0), |(a, r), f| (a + f.additions, r + f.deletions));
                    if files.is_empty() {
                        String::new()
                    } else {
                        format!("+{a} −{r}")
                    }
                });
                (kind, label.clone(), detail.clone(), *status, result)
            }
            Entry::Subagent {
                agent,
                task,
                status,
                ..
            } => (
                "agent",
                format!("Subagent {agent}"),
                task.clone(),
                *status,
                String::new(),
            ),
            _ => continue,
        };
        let failing = status == ToolStatus::Failed;
        commands += usize::from(kind == "command");
        failed += usize::from(failing);
        actions.push(json!({
            "kind": kind, "tool": tool, "target": target, "result": result,
            "error": failing,
            "pending": matches!(status, ToolStatus::Running | ToolStatus::Streaming),
        }));
    }
    json!({
        "actions": actions, "total": actions.len(), "commands": commands, "failed": failed,
        "changed": changed, "read": read,
    })
}

/// The approval mode the session is in, and the ones it can be put in.
fn modes(server: &Server, key: &str) -> Value {
    let rules = super::lock(&server.workspace)
        .session(key)
        .map(|s| s.permissions.clone())
        .unwrap_or_default();
    let shield = Shield::of(&rules);
    let approval = MODES
        .iter()
        .find(|(_, s)| *s == shield)
        .map_or("", |(name, _)| name);
    json!({ "approval": approval, "approvals": MODES.map(|(name, _)| name) })
}

/// Puts the session in approval mode `m.approval`.
async fn set_modes(server: &Server, key: &str, m: &Value) -> Value {
    if let Some(wanted) = m["approval"].as_str() {
        if let Err(e) = put_in_mode(server, key, wanted).await {
            return error(e);
        }
        // The server's event updates the session a moment later.
        return json!({ "approval": wanted, "approvals": MODES.map(|(name, _)| name) });
    }
    modes(server, key)
}

/// Puts session `key` in the approval mode named `wanted`.
///
/// # Errors
///
/// Returns why when there's no such mode or the server refuses it.
pub(super) async fn put_in_mode(server: &Server, key: &str, wanted: &str) -> Result<(), String> {
    let rules = MODES
        .iter()
        .find(|(name, _)| *name == wanted)
        .and_then(|(_, shield)| shield.rules())
        .ok_or_else(|| "Harlo has no such approval mode.".to_owned())?;
    server
        .client
        .set_permissions(key, &rules)
        .await
        .map_err(|e| e.to_string())
}

/// The waiting message of `queue` (`steering` or `followUp`) with `text`.
async fn waiting_item(
    server: &Server,
    key: &str,
    queue: Option<&str>,
    text: &str,
) -> Result<String, Value> {
    let inbox = server
        .client
        .inbox(key)
        .await
        .map_err(|e| error(e.to_string()))?;
    let wanted = queue.map(|q| {
        if q == "followUp" {
            Delivery::Queue
        } else {
            Delivery::Steer
        }
    });
    inbox
        .into_iter()
        .find(|item| {
            item.text.as_deref() == Some(text) && wanted.is_none_or(|d| item.delivery == d)
        })
        .map(|item| item.id)
        .ok_or_else(|| error("That message was sent already."))
}

/// Takes a waiting message back.
async fn unqueue(server: &Server, key: &str, m: &Value) -> Value {
    let text = m["text"].as_str().unwrap_or_default();
    match waiting_item(server, key, m["queue"].as_str(), text).await {
        Ok(id) => match server.client.cancel_inbox(key, &id).await {
            Ok(()) => json!({ "ok": true }),
            Err(e) => error(e.to_string()),
        },
        Err(e) => e,
    }
}

/// Sends a waiting follow-up now, into the running turn.
async fn promote(server: &Server, key: &str, m: &Value) -> Value {
    let text = m["text"].as_str().unwrap_or_default();
    match waiting_item(server, key, Some("followUp"), text).await {
        Ok(id) => match server.client.set_delivery(key, &id, Delivery::Steer).await {
            Ok(()) => json!({ "ok": true }),
            Err(e) => error(e.to_string()),
        },
        Err(e) => e,
    }
}

/// The user's messages, by row key, in order.
fn prompts(entries: &[Entry]) -> Vec<(&str, &str)> {
    entries
        .iter()
        .filter_map(|e| match e {
            Entry::Prompt { key, text, .. } => Some((key.as_str(), text.as_str())),
            _ => None,
        })
        .collect()
}

/// The user's messages a turn can be undone to (row key → message), and
/// whether the folder is a repository.
async fn undo_targets(server: &Server, key: &str) -> Value {
    let entries = match entries(server, key).await {
        Ok(e) => e,
        Err(e) => return e,
    };
    let targets: serde_json::Map<String, Value> = prompts(&entries)
        .into_iter()
        .map(|(row, _)| (row.to_owned(), json!(row)))
        .collect();
    let git = match repo(server, key) {
        Ok(repo) => repo.status().await.is_ok(),
        Err(_) => false,
    };
    json!({ "targets": targets, "git": git })
}

/// What undoing from the user's message `row` on does to files, and how
/// many of the user's messages follow it; `apply` does it.
async fn undo(server: &Server, key: &str, row: &str, apply: bool) -> Value {
    let entries = match entries(server, key).await {
        Ok(e) => e,
        Err(e) => return e,
    };
    let prompts = prompts(&entries);
    let Some(at) = prompts.iter().position(|(k, _)| *k == row) else {
        return error("That message isn't in the session any more.");
    };
    let later = prompts.len() - at - 1;
    let client = &server.client;
    let files = if apply {
        let staged = match client.stage_revert(key, row, true).await {
            Ok(revert) => revert,
            Err(e) => return error(e.to_string()),
        };
        if let Err(e) = client.commit_revert(key).await {
            let _ = client.cancel_revert(key).await;
            return error(e.to_string());
        }
        staged.files
    } else {
        let last = prompts.last().map(|(k, _)| *k).filter(|k| *k != row);
        match client.diff_since(key, row, last).await {
            Ok(files) => files,
            Err(e) => return error(e.to_string()),
        }
    };
    // An undo deletes what the turns added, and puts back the rest.
    let gone = |f: &harlo_client::FileDiff| {
        if apply {
            f.status == "deleted"
        } else {
            f.status == "added"
        }
    };
    let remove: Vec<&str> = files
        .iter()
        .filter(|f| gone(f))
        .map(|f| f.file.as_str())
        .collect();
    let restore: Vec<&str> = files
        .iter()
        .filter(|f| !gone(f))
        .map(|f| f.file.as_str())
        .collect();
    json!({ "restore": restore, "remove": remove, "headMoved": false, "later": later, "done": apply })
}

/// Edit and resend: goes back to before the user's message `row` and
/// returns its text to edit. With `fork`, a new session holds the
/// conversation up to it instead, and its key comes back too.
async fn branch(server: &Server, key: &str, row: &str, fork: bool) -> Value {
    let entries = match entries(server, key).await {
        Ok(e) => e,
        Err(e) => return e,
    };
    let Some(text) = prompts(&entries)
        .into_iter()
        .find(|(k, _)| *k == row)
        .map(|(_, text)| text.to_owned())
    else {
        return error("That message isn't in the session any more.");
    };
    let client = &server.client;
    if fork {
        return match client.fork_session(key, Some(row)).await {
            Ok(session) => json!({ "text": text, "key": session.id }),
            Err(e) => error(e.to_string()),
        };
    }
    let undone = undo(server, key, row, true).await;
    if undone.get("error").is_some() {
        return undone;
    }
    json!({ "text": text })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_diff_reads_as_numbered_hunks() {
        let patch = "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -3,2 +3,3 @@ fn x\n keep\n-old\n+new\n+more\n\\ No newline at end of file\n";
        let d = diff_json("a", patch);
        assert_eq!(
            (d["added"].as_u64(), d["removed"].as_u64()),
            (Some(2), Some(1))
        );
        let lines = d["hunks"][0]["lines"].as_array().expect("lines");
        assert_eq!(lines[0], json!({ "k": " ", "o": 3, "n": 3, "t": "keep" }));
        assert_eq!(lines[1], json!({ "k": "-", "o": 4, "t": "old" }));
        assert_eq!(lines[2], json!({ "k": "+", "n": 4, "t": "new" }));
        assert_eq!(lines[3], json!({ "k": "+", "n": 5, "t": "more" }));
        assert_eq!(lines[4]["k"], "\\");
        assert_eq!(d["cut"], false);
    }

    #[test]
    fn tools_become_the_phones_kinds_of_action() {
        assert_eq!(action_kind("shell"), "command");
        assert_eq!(action_kind("edit"), "edit");
        assert_eq!(action_kind("grep"), "search");
        assert_eq!(action_kind("websearch"), "web");
        assert_eq!(action_kind("todowrite"), "other");
        assert_eq!(escape("<a & b>"), "&lt;a &amp; b&gt;");
    }
}
