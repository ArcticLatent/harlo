//! Shell commands on the server: a person's own command in a session (`!`),
//! run with no model, and the commands agents send to the background, run
//! as jobs that report back when they end.

// Rust guideline compliant 2026-10-09

use std::path::Path;
use std::sync::Arc;

use harlo_engine::jobs::{self, Jobs};
use harlo_engine::tools::shell::{Ended, Ran, execute};
use harlo_engine::{Created, Message, ShellOutput, ShellRun, ShellTime, ascending_id, now};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::server::{Refusal, Server, lock};

/// The most of a command's output kept and shown to the model: its end.
const KEPT: usize = 64 * 1024;

/// Where a running session's background commands start.
#[derive(Debug)]
pub(crate) struct SessionJobs {
    /// The server.
    pub server: Arc<Server>,
    /// The session.
    pub session: String,
}

impl Jobs for SessionJobs {
    fn shell(
        &self,
        _message: &str,
        _call: &str,
        command: &str,
        cwd: &Path,
    ) -> Result<String, String> {
        Ok(self.server.start_shell_job(&self.session, command, cwd))
    }
}

/// The end of `text`, at most [`KEPT`] bytes, and whether some was left
/// out.
fn kept(text: &str) -> (String, bool) {
    if text.len() <= KEPT {
        return (text.to_owned(), false);
    }
    let mut from = text.len() - KEPT;
    while !text.is_char_boundary(from) {
        from += 1;
    }
    (text[from..].to_owned(), true)
}

impl Server {
    /// Starts `command` in `cwd` for session `session` as a background
    /// job; returns its ID.
    fn start_shell_job(self: &Arc<Self>, session: &str, command: &str, cwd: &Path) -> String {
        let id = ascending_id("sh");
        let stop = CancellationToken::new();
        let info = json!({
            "kind": "shell", "id": id, "sessionID": session, "status": "running",
            "title": command, "time": { "started": now() }, "shellID": id, "command": command,
        });
        let mut live = lock(&self.live);
        live.jobs.insert(id.clone(), info.clone());
        live.shells.insert(id.clone(), stop.clone());
        drop(live);
        let directory = cwd.to_string_lossy().into_owned();
        self.emit(
            "experimental.job.updated",
            json!({ "info": info }),
            Some(&directory),
            None,
        );
        let server = Arc::clone(self);
        let (session, job, command, cwd) = (
            session.to_owned(),
            id.clone(),
            command.to_owned(),
            cwd.to_path_buf(),
        );
        tokio::spawn(async move {
            let ran = execute(&command, &cwd, None, &stop).await;
            server.shell_job_ended(&session, &job, &command, &cwd, ran);
        });
        id
    }

    /// Reports background command `id` of session `session` as ended, and
    /// sends its output to the session, which runs on it.
    fn shell_job_ended(
        self: &Arc<Self>,
        session: &str,
        id: &str,
        command: &str,
        cwd: &Path,
        ran: Ran,
    ) {
        let (state, exit, output) = match ran.ended {
            Ended::Exited(code) => ("completed", code, ran.output),
            Ended::Stopped | Ended::TimedOut => ("cancelled", None, ran.output),
            Ended::NotStarted(why) => ("error", None, why),
        };
        let (output, _) = kept(&output);
        let info = {
            let mut live = lock(&self.live);
            live.shells.remove(id);
            live.jobs.get_mut(id).map(|info| {
                info["status"] = json!(state);
                info["time"]["completed"] = json!(now());
                info.clone()
            })
        };
        let directory = cwd.to_string_lossy().into_owned();
        if let Some(info) = info {
            self.emit(
                "experimental.job.updated",
                json!({ "info": info }),
                Some(&directory),
                None,
            );
        }
        let text = jobs::report(id, command, state, &output, exit);
        let metadata = json!({ "source": "shell", "shellID": id, "jobID": id, "state": state });
        let message = Message::Synthetic {
            id: ascending_id("msg"),
            time: Created { created: now() },
            text: text.clone(),
            description: Some(command.to_owned()),
            metadata: Some(metadata.clone()),
        };
        self.emit(
            "session.inbox.enqueued",
            json!({
                "inboxID": message.id(), "sessionID": session,
                "item": { "type": "synthetic", "delivery": "steer", "payload": {
                    "text": text, "description": command, "metadata": metadata,
                } },
            }),
            Some(&directory),
            Some(session),
        );
        self.enqueue(session, message, "steer");
    }

    /// Stops background job `id`: a command, or a subagent's run.
    ///
    /// # Errors
    ///
    /// When there's no such job.
    pub(crate) fn cancel_job(&self, id: &str) -> Result<(), Refusal> {
        let live = lock(&self.live);
        if let Some(stop) = live.shells.get(id) {
            stop.cancel();
            return Ok(());
        }
        let job = live
            .jobs
            .get(id)
            .cloned()
            .ok_or_else(|| Refusal::NotFound(format!("There's no job {id}.")))?;
        drop(live);
        if let Some(child) = job["childSessionID"].as_str() {
            self.interrupt(child);
        }
        Ok(())
    }

    /// The background jobs of session `session`, or of every session,
    /// going or ended since the server started.
    pub(crate) fn jobs_of(&self, session: Option<&str>) -> Vec<Value> {
        lock(&self.live)
            .jobs
            .values()
            .filter(|j| session.is_none_or(|s| j["sessionID"] == s))
            .cloned()
            .collect()
    }

    /// Runs a person's `command` in session `id`'s folder with no model,
    /// and returns once it has ended. Its output waits in the inbox for
    /// the agent's next turn; it starts none itself.
    ///
    /// # Errors
    ///
    /// When there's no such session.
    pub(crate) async fn run_user_shell(&self, id: &str, command: &str) -> Result<(), Refusal> {
        let session = self.session(id)?;
        let directory = session.location.directory.clone();
        let shell_id = ascending_id("sh");
        let started = now();
        let mut info = json!({
            "id": shell_id, "status": "running", "command": command, "cwd": directory,
            "time": { "started": started }, "metadata": { "sessionID": id },
        });
        self.emit(
            "session.shell.started",
            json!({ "sessionID": id, "shell": info }),
            Some(&directory),
            Some(id),
        );
        let ran = execute(
            command,
            Path::new(&directory),
            None,
            &CancellationToken::new(),
        )
        .await;
        let (status, exit, printed) = match ran.ended {
            Ended::Exited(code) => ("exited", code, ran.output),
            Ended::TimedOut => ("timeout", None, ran.output),
            Ended::Stopped => ("killed", None, ran.output),
            Ended::NotStarted(why) => ("killed", None, why),
        };
        let (printed, truncated) = kept(&printed);
        let completed = now();
        info["status"] = json!(status);
        info["exit"] = json!(exit);
        info["time"]["completed"] = json!(completed);
        let output = ShellOutput {
            cursor: printed.len(),
            size: printed.len(),
            output: printed,
            truncated,
        };
        self.emit(
            "session.shell.ended",
            json!({ "sessionID": id, "shell": info, "output": output }),
            Some(&directory),
            Some(id),
        );
        let run = ShellRun {
            id: ascending_id("msg"),
            time: ShellTime {
                created: started,
                completed: Some(completed),
            },
            shell_id: shell_id.clone(),
            command: command.to_owned(),
            status: status.to_owned(),
            exit,
            output: output.clone(),
        };
        self.store.put_message(id, &Message::Shell(Box::new(run)))?;
        let text = format!(
            "The following shell command was executed by the user:\n{}",
            jobs::report(&shell_id, command, "completed", &output.output, exit)
        );
        let metadata = json!({
            "source": "shell", "shellID": shell_id, "state": "completed",
            "truncated": truncated, "exit": exit,
        });
        let message = Message::Synthetic {
            id: ascending_id("msg"),
            time: Created { created: now() },
            text: text.clone(),
            description: None,
            metadata: Some(metadata.clone()),
        };
        self.emit(
            "session.inbox.enqueued",
            json!({
                "inboxID": message.id(), "sessionID": id,
                "item": { "type": "synthetic", "delivery": "steer", "payload": {
                    "text": text, "metadata": metadata,
                } },
            }),
            Some(&directory),
            Some(id),
        );
        lock(&self.live)
            .inbox
            .entry(id.to_owned())
            .or_default()
            .push((message, "steer".to_owned()));
        Ok(())
    }
}
