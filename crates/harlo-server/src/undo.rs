//! Undo: stage an undo back to before a prompt
//! (restoring the files the agent changed since), then keep it (the
//! messages go) or cancel it (the files come back).
//!
//! Each step's files are restored three-way from the snapshots around it
//! (see `harlo_engine::snapshot`), newest step first. What each step
//! changed is attributed: its own tools' writes, a shell command it ran,
//! other sessions' writes in the same folder at the same time. Files left
//! alone are listed in the staged undo's `report`, with why.

// Rust guideline compliant 2026-10-09

use harlo_engine::snapshot::{Attribution, FileUndo, Restored};
use harlo_engine::{Message, Part};
use serde_json::{Value, json};

use crate::git;
use crate::server::{Refusal, Server};
use crate::store::Session;

/// The window a step's tools ran in, and the files they wrote.
struct Window {
    from: i64,
    to: i64,
    files: Vec<String>,
}

impl Server {
    /// Stages an undo of session `id` back to before its prompt `message`.
    /// With `files`, the files the agent changed since are restored now.
    ///
    /// # Errors
    ///
    /// Fails when the session or prompt doesn't exist, the session is busy,
    /// or a snapshot can't be read.
    pub(crate) async fn stage_revert(
        &self,
        id: &str,
        message: &str,
        files: bool,
    ) -> Result<Value, Refusal> {
        let mut session = self.session(id)?;
        if self.busy().iter().any(|b| b == id) {
            return Err(Refusal::Bad(
                "The session is running; stop it before undoing.".to_owned(),
            ));
        }
        let messages = self.store.messages(id)?;
        if !messages
            .iter()
            .any(|m| matches!(m, Message::User { id, .. } if id == message))
        {
            return Err(Refusal::NotFound(format!("There's no prompt {message}.")));
        }
        // A staged undo moves: put the files back first.
        if session.revert.is_some() {
            self.put_back(&mut session).await?;
        }
        let directory = session.location.directory.clone();
        let shadow = self.shadow(&directory);
        let mut revert = json!({ "messageID": message, "files": [] });
        if files {
            let before = shadow.track().await.map_err(Refusal::Store)?;
            let others = self.other_windows(&session)?;
            let mut report: Vec<FileUndo> = Vec::new();
            for m in messages.iter().rev().filter(|m| m.id() >= message) {
                let Message::Assistant(step) = m else {
                    continue;
                };
                let Some(snap) = &step.snapshot else { continue };
                let (Some(start), Some(end)) = (&snap.start, &snap.end) else {
                    continue;
                };
                let from = step.time.created;
                let to = step.time.completed.unwrap_or(i64::MAX);
                let who = Attribution {
                    own: snap.files.clone(),
                    others: others
                        .iter()
                        .filter(|w| w.from <= to && w.to >= from)
                        .flat_map(|w| w.files.iter().cloned())
                        .collect(),
                    ran_command: step
                        .content
                        .iter()
                        .any(|p| matches!(p, Part::Tool { name, .. } if name == "shell")),
                };
                let done = shadow
                    .undo(start, end, &who)
                    .await
                    .map_err(Refusal::Store)?;
                report.extend(done);
            }
            let after = shadow.track().await.map_err(Refusal::Store)?;
            let patch = shadow
                .patch(&before, &after)
                .await
                .map_err(Refusal::Store)?;
            revert["files"] = Value::Array(git::split(&patch));
            revert["snapshot"] = json!(before);
            revert["report"] = Value::Array(
                report
                    .iter()
                    .filter(|f| f.outcome != Restored::Restored && f.outcome != Restored::Merged)
                    .map(|f| json!({ "file": f.path, "kept": kept_why(&f.outcome) }))
                    .collect(),
            );
            revert["restored"] = json!(after);
        }
        self.change(id, |s| s.revert = Some(revert.clone()))?;
        // The files may be as they were: what the session saw no longer holds.
        self.forget_files(id);
        let public = public(&revert);
        self.emit(
            "session.revert.staged",
            json!({ "sessionID": id, "revert": public }),
            Some(&directory),
            Some(id),
        );
        Ok(revert)
    }

    /// Keeps session `id`'s staged undo: the undone messages go for good.
    ///
    /// # Errors
    ///
    /// Fails when there's no such session or nothing is staged.
    pub(crate) fn commit_revert(&self, id: &str) -> Result<(), Refusal> {
        let mut session = self.session(id)?;
        let Some(revert) = session.revert.take() else {
            return Err(Refusal::Bad("No undo is staged.".to_owned()));
        };
        let from = revert["messageID"].as_str().unwrap_or_default();
        self.store.remove_messages_from(id, from)?;
        self.change(id, |s| s.revert = None)?;
        self.emit(
            "session.revert.committed",
            json!({ "sessionID": id }),
            Some(&session.location.directory),
            Some(id),
        );
        Ok(())
    }

    /// Cancels session `id`'s staged undo: the files it restored come back
    /// as they were, unless they were changed again since.
    ///
    /// # Errors
    ///
    /// Fails when there's no such session or a snapshot can't be read.
    pub(crate) async fn cancel_revert(&self, id: &str) -> Result<(), Refusal> {
        let mut session = self.session(id)?;
        if session.revert.is_none() {
            return Ok(());
        }
        self.put_back(&mut session).await?;
        self.change(id, |s| s.revert = None)?;
        self.emit(
            "session.revert.cleared",
            json!({ "sessionID": id }),
            Some(&session.location.directory),
            Some(id),
        );
        Ok(())
    }

    /// Puts back the files a staged undo restored, and clears it.
    async fn put_back(&self, session: &mut Session) -> Result<(), Refusal> {
        let Some(revert) = session.revert.take() else {
            return Ok(());
        };
        if let (Some(before), Some(after)) =
            (revert["snapshot"].as_str(), revert["restored"].as_str())
        {
            let shadow = self.shadow(&session.location.directory);
            let changed = shadow
                .changed(before, after)
                .await
                .map_err(Refusal::Store)?;
            let all = Attribution {
                own: changed,
                ..Attribution::default()
            };
            shadow
                .undo(before, after, &all)
                .await
                .map_err(Refusal::Store)?;
        }
        Ok(())
    }

    /// The windows other sessions in the same folder ran tools in.
    fn other_windows(&self, session: &Session) -> Result<Vec<Window>, Refusal> {
        let mut out = Vec::new();
        for other in self.store.sessions()? {
            if other.id == session.id || other.location.directory != session.location.directory {
                continue;
            }
            for m in self.store.messages(&other.id)? {
                if let Message::Assistant(step) = m
                    && let Some(snap) = step.snapshot
                    && !snap.files.is_empty()
                {
                    out.push(Window {
                        from: step.time.created,
                        to: step.time.completed.unwrap_or(i64::MAX),
                        files: snap.files,
                    });
                }
            }
        }
        Ok(out)
    }
}

/// The staged undo as clients read it.
fn public(revert: &Value) -> Value {
    json!({
        "messageID": revert["messageID"],
        "files": revert["files"],
        "snapshot": revert["snapshot"],
        "report": revert["report"],
    })
}

/// Why undo left a file as it is.
fn kept_why(outcome: &Restored) -> &'static str {
    match outcome {
        Restored::Conflict => "conflict",
        Restored::OtherSession => "another session",
        Restored::Ambiguous => "ambiguous",
        Restored::DuringCommand => "changed while a command ran",
        Restored::Restored | Restored::Merged => "",
    }
}
