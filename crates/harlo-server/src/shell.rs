//! Shell commands on the server: a person's own command in a session (`!`),
//! run with no model, and the commands agents send to the background, run
//! as jobs that report back when they end.

// Rust guideline compliant 2026-10-09

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use harlo_engine::jobs::{self, Adopted, Asked, Jobs};
use harlo_engine::sandbox::Confined;
use harlo_engine::tools::shell::{Ended, Ran, Running, capped, execute};
use harlo_engine::{Created, Message, ShellOutput, ShellRun, ShellTime, ascending_id, now};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::server::{Refusal, Server, lock};

/// The most of a person's own command's output kept and shown: its end.
const KEPT: usize = 64 * 1024;

/// How long a command an agent gave no time limit runs before it carries
/// on in the background: two minutes, the limit it used to be stopped at.
const PATIENCE: Duration = Duration::from_mins(2);

/// Names another [`PATIENCE`], in milliseconds, for tests: theirs can't
/// wait two minutes for a command to outrun it.
const PATIENCE_VAR: &str = "HARLO_SHELL_PATIENCE_MS";

/// How long a command that outran its time may carry on in the background
/// before it is stopped. An agent that sends a command to the background
/// itself means it to stay, as a server does. One moved there for being
/// slow was meant to end, and one that hangs shouldn't stay for good.
const MOVED_FOR: Duration = Duration::from_mins(30);

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
        confined: Option<Confined>,
    ) -> Result<String, String> {
        Ok(self
            .server
            .start_shell_job(&self.session, command, cwd, confined))
    }

    fn takes_over_after(&self) -> Option<Duration> {
        let named = std::env::var(PATIENCE_VAR)
            .ok()
            .and_then(|ms| ms.parse().ok())
            .map(Duration::from_millis);
        Some(named.unwrap_or(PATIENCE))
    }

    fn asked_to_move<'a>(&'a self, _message: &'a str, call: &'a str) -> Asked<'a> {
        // Listed now, not when the future is first polled: that is only
        // once the command has started, and by then its call has been
        // announced as running, so someone may already have asked. An
        // early ask isn't lost: `notify_one` keeps it for the wait below.
        let asked = Arc::new(tokio::sync::Notify::new());
        let key = (self.session.clone(), call.to_owned());
        lock(&self.server.live)
            .waited
            .insert(key.clone(), Arc::clone(&asked));
        // (Dropped with the future, when the command ends.)
        let listed = Waited {
            server: &self.server,
            key,
        };
        Box::pin(async move {
            let _listed = listed;
            asked.notified().await;
        })
    }

    fn adopt(&self, adopted: Adopted<'_>) -> String {
        // A person who sends a command to the background means it to
        // stay, as an agent that starts one there does.
        let limit = (!adopted.asked).then_some(MOVED_FOR);
        self.server.adopt_shell_job(
            &self.session,
            adopted.command,
            adopted.cwd,
            adopted.running,
            limit,
        )
    }
}

/// A command an agent is waiting for, listed while it runs.
struct Waited<'a> {
    server: &'a Server,
    key: (String, String),
}

impl Drop for Waited<'_> {
    fn drop(&mut self) {
        lock(&self.server.live).waited.remove(&self.key);
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
    /// job, in the sandbox when it is `confined`; returns its ID.
    fn start_shell_job(
        self: &Arc<Self>,
        session: &str,
        command: &str,
        cwd: &Path,
        confined: Option<Confined>,
    ) -> String {
        let (id, stop) = self.list_shell_job(session, command, cwd);
        let server = Arc::clone(self);
        let (session, job, command, cwd) = (
            session.to_owned(),
            id.clone(),
            command.to_owned(),
            cwd.to_path_buf(),
        );
        tokio::spawn(async move {
            let ran = execute(&command, &cwd, confined.as_ref(), None, &stop).await;
            server.shell_job_ended(&session, &job, &command, &cwd, ran);
        });
        id
    }

    /// Lets `running`, a command of session `session`, carry on as a
    /// background job, for at most `limit` when there is one; returns the
    /// job's ID.
    fn adopt_shell_job(
        self: &Arc<Self>,
        session: &str,
        command: &str,
        cwd: &Path,
        mut running: Running,
        limit: Option<Duration>,
    ) -> String {
        let (id, stop) = self.list_shell_job(session, command, cwd);
        let server = Arc::clone(self);
        let (session, job, command, cwd) = (
            session.to_owned(),
            id.clone(),
            command.to_owned(),
            cwd.to_path_buf(),
        );
        tokio::spawn(async move {
            let ended = if let Some(ended) = running.wait(limit, &stop).await {
                ended
            } else {
                running.end().await;
                Ended::TimedOut
            };
            let ran = Ran {
                output: running.printed(),
                ended,
            };
            server.shell_job_ended(&session, &job, &command, &cwd, ran);
        });
        id
    }

    /// Lists `command` in `cwd` as a background job of session `session`
    /// that is running; returns its ID and what stops it.
    fn list_shell_job(
        self: &Arc<Self>,
        session: &str,
        command: &str,
        cwd: &Path,
    ) -> (String, CancellationToken) {
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
        (id, stop)
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
            Ended::Stopped => ("cancelled", None, ran.output),
            // (Only a command that was moved here has a limit to pass.)
            Ended::TimedOut => (
                "error",
                None,
                format!(
                    "{}\nStopped after another {} minutes in the background: it took too long.",
                    ran.output.trim_end(),
                    MOVED_FOR.as_secs() / 60
                ),
            ),
            Ended::NotStarted(why) => ("error", None, why),
        };
        // What the model reads of it is cut as any command's output is: a
        // long build's result would otherwise weigh on every later step.
        let output = capped(&output);
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

    /// Sends the command session `session`'s call `call` is running to
    /// the background: its turn stops waiting for it, and it carries on
    /// as a job.
    ///
    /// # Errors
    ///
    /// When no agent of the session is waiting for such a command.
    pub(crate) fn move_to_background(&self, session: &str, call: &str) -> Result<(), Refusal> {
        let key = (session.to_owned(), call.to_owned());
        let asked = lock(&self.live).waited.get(&key).cloned().ok_or_else(|| {
            Refusal::NotFound("That command isn't running, or has already ended.".to_owned())
        })?;
        asked.notify_one();
        Ok(())
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
        // (A person's own command isn't confined.)
        let ran = execute(
            command,
            Path::new(&directory),
            None,
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
            metadata: Some(json!({ "background": true })),
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

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    use harlo_engine::jobs::Jobs as _;

    use super::SessionJobs;
    use crate::{PhoneHost, Server, Store};

    /// A server with nothing in it, its files under `scratch`.
    fn server(scratch: &Path) -> Arc<Server> {
        let config = harlo_engine::Config::load(scratch).expect("no configuration is a valid one");
        Arc::new(Server::new(
            Store::in_memory().expect("a store"),
            config,
            scratch,
            None,
            PhoneHost::new(None, scratch.join("phone.json")),
            scratch.join("snapshot"),
            Vec::new(),
        ))
    }

    #[tokio::test]
    async fn a_command_can_be_moved_before_it_has_started() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let server = server(scratch.path());
        let jobs = SessionJobs {
            server: Arc::clone(&server),
            session: "ses_1".to_owned(),
        };

        // The call is announced as running before its command has started,
        // which is when the wait is first polled: an ask in between counts.
        let asked = jobs.asked_to_move("msg_1", "call_1");
        server
            .move_to_background("ses_1", "call_1")
            .expect("listed from its call");
        tokio::time::timeout(Duration::from_secs(5), asked)
            .await
            .expect("an early ask is kept");

        // The wait is over, so there is nothing left to move.
        let late = server.move_to_background("ses_1", "call_1");
        assert!(late.is_err(), "{late:?}");
    }
}
