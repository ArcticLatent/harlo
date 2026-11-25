//! Background jobs: work a session started that runs on without blocking
//! it, such as a command or a subagent sent to the background, a Code Mode
//! script, or a picture being made.
//!
//! The server owns them. Harlo reads a session's jobs when it opens,
//! follows them as they start and end, and stops one on asking.

// Rust guideline compliant 2026-10-09

use harlo_core::server::{Job, JobKind, JobStatus};
use harlo_core::workspace::EventKind;

use super::{App, Message};
use crate::task::Task;

/// Something that happened to a session's background jobs.
#[derive(Debug, Clone)]
pub enum JobEvent {
    /// A session's jobs were read from the server.
    Loaded {
        /// The session.
        session: String,
        /// Its jobs, or why they couldn't be read.
        result: Result<Vec<Job>, String>,
    },
    /// Stop a running job, named by its ID or, for a subagent, by its
    /// child session's.
    Stop(String),
    /// Send the command the open session's agent waits for, named by its
    /// call's ID, to the background.
    Move(String),
}

impl App {
    /// A session's background jobs still running, oldest first.
    pub fn jobs(&self, session: &str) -> impl Iterator<Item = &Job> {
        self.per_session
            .jobs
            .get(session)
            .into_iter()
            .flatten()
            .filter(|job| job.status == JobStatus::Running)
    }

    /// The running background job behind subagent session `child`.
    #[must_use]
    pub fn subagent_job(&self, session: &str, child: &str) -> Option<&Job> {
        self.jobs(session)
            .find(|job| matches!(&job.kind, JobKind::Subagent { child: c, .. } if c == child))
    }

    /// Reads a session's background jobs from the server.
    pub(super) fn load_jobs(&self, id: String) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        Task::perform(
            async move {
                let result = client.jobs(&id).await.map_err(|e| e.to_string());
                JobEvent::Loaded {
                    session: id,
                    result,
                }
            },
            Message::Job,
        )
    }

    /// Handles something that happened to a background job.
    pub(super) fn job_event(&mut self, event: JobEvent) -> Task<Message> {
        match event {
            JobEvent::Loaded { session, result } => {
                // A server without background jobs has none to show.
                if let Ok(jobs) = result {
                    self.per_session.jobs.insert(session, jobs);
                }
                Task::none()
            }
            JobEvent::Stop(named) => {
                let Some(open) = self.active.clone() else {
                    return Task::none();
                };
                let id = self
                    .subagent_job(&open, &named)
                    .map_or(named, |job| job.id.clone());
                self.act(move |client, _| async move { client.cancel_job(&id).await })
            }
            JobEvent::Move(call) => {
                let Some(open) = self.active.clone() else {
                    return Task::none();
                };
                self.act(
                    move |client, _| async move { client.move_to_background(&open, &call).await },
                )
            }
        }
    }

    /// Follows a job the server reports started or ended.
    pub(super) fn job_changed(&mut self, kind: &EventKind) {
        let EventKind::JobUpdated(job) = kind else {
            return;
        };
        let jobs = self
            .per_session
            .jobs
            .entry(job.session_id.clone())
            .or_default();
        match jobs.iter_mut().find(|known| known.id == job.id) {
            Some(known) => known.clone_from(job),
            None => jobs.push(job.clone()),
        }
        // Ended jobs aren't shown; only the latest few are kept.
        let ended = jobs
            .iter()
            .filter(|j| j.status != JobStatus::Running)
            .count();
        if ended > ENDED_KEPT {
            let mut drop = ended - ENDED_KEPT;
            jobs.retain(|j| {
                let gone = drop > 0 && j.status != JobStatus::Running;
                drop -= usize::from(gone);
                !gone
            });
        }
    }
}

/// How many ended jobs a session keeps.
const ENDED_KEPT: usize = 8;
