//! What a session has going beside its conversation, as the phone shows
//! it above the message box: the agent's todo list, the work it sent to
//! the background, its goal, and the command it waits for.
//!
//! The window shows the same in its rows above the composer. Each row here
//! says what it is and names the one thing a person can do to it, and the
//! phone sends that row's ID back to have it done ([`act`]).

// Rust guideline compliant 2026-10-09

use harlo_client::{Client, Delivery, Goal, GoalStatus, Job, JobKind, JobStatus};
use serde_json::{Value, json};

use crate::timeline::{self, Entry, Todo, TodoStatus};
use crate::workspace::ToolStatus;

/// The tool that runs a command.
const SHELL: &str = "shell";

/// A background job's row ID starts with this, then the job's ID.
const JOB: &str = "job:";

/// The goal's row ID starts with this, then the `/goal` word it sends.
const GOAL: &str = "goal:";

/// The background work and goal of the session a phone has open.
#[derive(Debug, Default)]
pub(super) struct Aside {
    /// Its background jobs, as the server last said.
    jobs: Vec<Job>,
    /// Its goal, when it has one.
    goal: Option<Goal>,
}

impl Aside {
    /// Reads session `key`'s jobs and goal from the server.
    ///
    /// A server that can't say has none to show.
    pub(super) async fn load(client: &Client, key: &str, directory: Option<&str>) -> Self {
        let jobs = client.jobs(key).await.unwrap_or_default();
        let goal = match directory {
            Some(directory) => client.goal(key, directory).await.ok().flatten(),
            None => None,
        };
        Self { jobs, goal }
    }

    /// Follows a job the server reports started, changed or ended.
    pub(super) fn job_changed(&mut self, job: &Job) {
        match self.jobs.iter_mut().find(|known| known.id == job.id) {
            Some(known) => known.clone_from(job),
            None => self.jobs.push(job.clone()),
        }
        // Only running jobs are shown.
        self.jobs.retain(|j| j.status == JobStatus::Running);
    }

    /// Follows the session's goal being set, changed or cleared.
    pub(super) fn goal_changed(&mut self, goal: Option<&Goal>) {
        self.goal = goal.cloned();
    }

    /// Returns the `jobs` event for session `key`: its rows, and the call
    /// of the command the agent waits for (`background`, empty when none).
    pub(super) fn event(&self, key: &str, entries: &[Entry], waits_on_user: bool) -> Value {
        let mut items: Vec<Value> = self
            .jobs
            .iter()
            .filter(|job| job.status == JobStatus::Running)
            .map(job_row)
            .collect();
        items.extend(self.goal.as_ref().map(goal_row));
        // An agent waiting for leave to run a command isn't running it.
        let background = if waits_on_user {
            None
        } else {
            running_command(entries)
        };
        json!({ "ev": "jobs", "key": key, "items": items,
                "background": background.unwrap_or_default() })
    }
}

/// A running job's row, with Stop.
fn job_row(job: &Job) -> Value {
    let (symbol, title, text) = match &job.kind {
        JobKind::Shell { command } => ("terminal", "Command".to_owned(), command.clone()),
        JobKind::Subagent {
            agent, description, ..
        } => ("person.2", capitalized(agent), description.clone()),
        JobKind::Script => (
            "chevron.left.forwardslash.chevron.right",
            "Script".to_owned(),
            job.title.clone().unwrap_or_default(),
        ),
        JobKind::Image { path } => ("photo", "Picture".to_owned(), path.clone()),
        JobKind::Video { path } => ("film", "Video".to_owned(), path.clone()),
    };
    json!({ "id": format!("{JOB}{}", job.id), "symbol": symbol, "title": title, "text": text,
            "note": "In the background", "action": "Stop", "working": true })
}

/// The goal's row: where it stands, and Pause, Resume or Dismiss.
fn goal_row(goal: &Goal) -> Value {
    let (note, word, action) = match goal.status {
        GoalStatus::Active => ("Active", "pause", "Pause"),
        GoalStatus::Paused => ("Paused", "resume", "Resume"),
        GoalStatus::Complete => ("Reached", "clear", "Dismiss"),
    };
    let detail = match (&goal.reason, goal.status) {
        (Some(reason), _) => reason.clone(),
        (None, GoalStatus::Active) if goal.rounds > 0 => {
            format!("Round {} of {}", goal.rounds, goal.max_rounds)
        }
        (None, _) => String::new(),
    };
    json!({ "id": format!("{GOAL}{word}"), "symbol": "scope", "title": "Goal",
            "text": goal.objective, "detail": detail, "note": note, "action": action,
            "working": goal.status == GoalStatus::Active })
}

/// `explore` as `Explore`.
fn capitalized(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// Returns the call ID of the command the agent waits for in its current
/// run, which a person can send to the background.
///
/// Judged as the window does: the newest tool call still under way since
/// the last prompt, when that is a command already running.
fn running_command(entries: &[Entry]) -> Option<String> {
    entries
        .iter()
        .rev()
        .take_while(|entry| !matches!(entry, Entry::Prompt { .. }))
        .find_map(|entry| match entry {
            Entry::Tool {
                key, name, status, ..
            } if matches!(status, ToolStatus::Streaming | ToolStatus::Running) => {
                Some((name == SHELL && *status == ToolStatus::Running).then(|| call_of(key)))
            }
            _ => None,
        })
        .flatten()
}

/// The call ID in a tool row's key (`message/tool/call`).
pub(super) fn call_of(key: &str) -> String {
    key.rsplit('/').next().unwrap_or_default().to_owned()
}

/// Returns the `todos` event for session `key`: the agent's list as it
/// stands, in one phase, without the items it dropped.
pub(super) fn todos_event(key: &str, entries: &[Entry]) -> Value {
    let tasks: Vec<Value> = timeline::current_todos(entries)
        .unwrap_or_default()
        .iter()
        .filter_map(todo)
        .collect();
    let phases = if tasks.is_empty() {
        Vec::new()
    } else {
        vec![json!({ "name": "Todos", "tasks": tasks })]
    };
    json!({ "ev": "todos", "key": key, "phases": phases })
}

/// One item as the phone reads it; `None` for a dropped one.
fn todo(item: &Todo) -> Option<Value> {
    let status = match item.status {
        TodoStatus::Pending => "pending",
        TodoStatus::InProgress => "in_progress",
        TodoStatus::Completed => "completed",
        TodoStatus::Cancelled => return None,
    };
    Some(json!({ "content": item.content, "status": status }))
}

/// Does what the row `id` of session `key` offers: stops a job, or
/// pauses, resumes or clears the goal.
///
/// # Errors
///
/// Fails when `id` names no row's action, or the server refuses it or
/// can't be reached.
pub(super) async fn act(client: &Client, key: &str, id: &str) -> Result<(), String> {
    if let Some(job) = id.strip_prefix(JOB) {
        return client.cancel_job(job).await.map_err(|e| e.to_string());
    }
    match id.strip_prefix(GOAL) {
        Some(word @ ("pause" | "resume" | "clear")) => client
            .run_command(key, "goal", word, Delivery::Queue)
            .await
            .map_err(|e| e.to_string()),
        _ => Err("Harlo has nothing like that to do here.".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn tool(key: &str, name: &str, status: ToolStatus) -> Entry {
        Entry::Tool {
            key: key.to_owned(),
            name: name.to_owned(),
            label: String::new(),
            detail: String::new(),
            status,
            took: None,
            files: Vec::new(),
            todos: None,
            pictures: Vec::new(),
            error: None,
            script: None,
        }
    }

    fn prompt(key: &str) -> Entry {
        Entry::Prompt {
            key: key.to_owned(),
            text: String::new(),
            attachments: Vec::new(),
            pictures: Vec::new(),
            skills: Vec::new(),
            agent: None,
        }
    }

    fn shell_job(id: &str, status: JobStatus) -> Job {
        Job {
            id: id.to_owned(),
            session_id: "ses_1".to_owned(),
            kind: JobKind::Shell {
                command: "npm run dev".to_owned(),
            },
            status,
            title: None,
            started: 0.0,
            completed: None,
        }
    }

    #[test]
    fn only_a_command_under_way_in_this_run_can_go_to_the_background() {
        let running = [
            prompt("m1"),
            tool("m2/tool/call_a", "read", ToolStatus::Completed),
            tool("m2/tool/call_b", "shell", ToolStatus::Running),
        ];
        assert_eq!(running_command(&running).as_deref(), Some("call_b"));
        // Still being written, another tool, an earlier run's, or ended.
        let written = [tool("m2/tool/call_b", "shell", ToolStatus::Streaming)];
        assert_eq!(running_command(&written), None);
        let reading = [
            tool("m2/tool/call_b", "shell", ToolStatus::Running),
            tool("m2/tool/call_c", "read", ToolStatus::Running),
        ];
        assert_eq!(running_command(&reading), None);
        let earlier = [
            tool("m2/tool/call_b", "shell", ToolStatus::Running),
            prompt("m3"),
        ];
        assert_eq!(running_command(&earlier), None);
        let ended = [tool("m2/tool/call_b", "shell", ToolStatus::Completed)];
        assert_eq!(running_command(&ended), None);
    }

    #[test]
    fn running_jobs_and_the_goal_are_rows_with_one_action_each() {
        let mut aside = Aside::default();
        aside.job_changed(&shell_job("job_1", JobStatus::Running));
        aside.job_changed(&shell_job("job_2", JobStatus::Running));
        aside.job_changed(&shell_job("job_2", JobStatus::Completed));
        aside.goal_changed(Some(&Goal {
            id: "goal_1".to_owned(),
            objective: "All tests pass".to_owned(),
            status: GoalStatus::Active,
            reason: None,
            rounds: 2,
            max_rounds: 10,
        }));
        let running = [tool("m2/tool/call_b", "shell", ToolStatus::Running)];
        let event = aside.event("ses_1", &running, false);
        assert_eq!(event["background"], "call_b");
        assert_eq!(
            event["items"],
            json!([
                { "id": "job:job_1", "symbol": "terminal", "title": "Command",
                  "text": "npm run dev", "note": "In the background", "action": "Stop",
                  "working": true },
                { "id": "goal:pause", "symbol": "scope", "title": "Goal",
                  "text": "All tests pass", "detail": "Round 2 of 10", "note": "Active",
                  "action": "Pause", "working": true },
            ])
        );
        // Waiting for leave to run it, the command isn't under way yet.
        assert_eq!(aside.event("ses_1", &running, true)["background"], "");
        aside.goal_changed(None);
        assert_eq!(
            aside.event("ses_1", &[], false)["items"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn the_todo_list_is_the_last_one_written_without_what_was_dropped() {
        let item = |content: &str, status| Todo {
            content: content.to_owned(),
            status,
        };
        let mut first = tool("m1/tool/a", "todowrite", ToolStatus::Completed);
        let mut last = tool("m2/tool/b", "todowrite", ToolStatus::Completed);
        if let (Entry::Tool { todos: a, .. }, Entry::Tool { todos: b, .. }) =
            (&mut first, &mut last)
        {
            *a = Some(vec![item("Old", TodoStatus::Pending)]);
            *b = Some(vec![
                item("Read the code", TodoStatus::Completed),
                item("Fix it", TodoStatus::InProgress),
                item("Rewrite it all", TodoStatus::Cancelled),
            ]);
        }
        let event = todos_event("ses_1", &[first, last]);
        assert_eq!(
            event["phases"],
            json!([{ "name": "Todos", "tasks": [
                { "content": "Read the code", "status": "completed" },
                { "content": "Fix it", "status": "in_progress" },
            ] }])
        );
        assert_eq!(todos_event("ses_1", &[])["phases"], json!([]));
    }
}
