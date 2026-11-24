//! The open session's menu: fork it, move it to another folder of its
//! project, export it to a file or import one, archive it, or delete it;
//! and forking from one of its prompts.
//!
//! OpenCode does each; Harlo asks for the file or the confirmation, and
//! opens what comes of it.

// Rust guideline compliant 2026-10-09

use std::path::PathBuf;

use harlo_core::server::Client;
use harlo_core::timeline::Entry;
use harlo_core::workspace::Session;

use super::{App, Message, home_folder};
use crate::model::Picker;
use crate::task::Task;

/// Something done from the open session's menu, or what came of it.
#[derive(Debug, Clone)]
pub enum SessionAction {
    /// Start a new session with a copy of the open one's history.
    Fork,
    /// Start a new session with the history before the prompt `key`, with
    /// that prompt in the composer to change and send again.
    ForkBefore(String),
    /// A fork or an import was made: open it, with the composer's text
    /// when there is one.
    Made(Result<Box<Session>, String>, Option<String>),
    /// Move the open session to another folder of its project.
    MoveTo(String),
    /// Ask where to save the open session's transcript.
    Export,
    /// Save the open session's transcript to a file.
    ExportTo(PathBuf),
    /// Ask for a transcript to import.
    Import,
    /// Import the transcript in a file into the open session's folder.
    ImportFrom(PathBuf),
    /// Archive the open session: it leaves the lists but stays on the
    /// server.
    Archive,
    /// Ask before deleting the open session.
    AskDelete,
    /// Delete the open session and its subagents' sessions for good.
    Delete,
    /// Copy the open session, made on OpenCode's service here, to Harlo's
    /// server here.
    CopyToHarlo,
    /// What copying it did.
    Copied(Result<harlo_core::import::Imported, String>),
}

impl App {
    /// Folders of the open session's project it could move to, with their
    /// names: the project's own folder and its worktrees, less its own.
    #[must_use]
    pub fn move_targets(&self) -> Vec<(String, String)> {
        let Some(folder) = self
            .active
            .as_deref()
            .and_then(|id| self.workspace.session(id))
            .map(|s| s.directory.as_str())
        else {
            return Vec::new();
        };
        let Some(project) = self
            .projects
            .iter()
            .find(|p| p.directory == folder || p.worktrees.iter().any(|w| w.directory == folder))
        else {
            return Vec::new();
        };
        std::iter::once((project.directory.clone(), project.name.clone()))
            .chain(
                project
                    .worktrees
                    .iter()
                    .map(|w| (w.directory.clone(), w.name.clone())),
            )
            .filter(|(directory, _)| directory != folder)
            .collect()
    }

    /// Handles something done from the session's menu.
    pub(super) fn session_menu(&mut self, action: SessionAction) -> Task<Message> {
        if let SessionAction::Made(made, text) = action {
            return self.made(made, text);
        }
        if let SessionAction::Copied(done) = action {
            return self.copied(done);
        }
        // Each choice closes the menu; deleting asks first, in its place.
        self.picker = matches!(action, SessionAction::AskDelete).then_some(Picker::DeleteSession);
        let (Some(client), Some(id)) = (self.client.clone(), self.active.clone()) else {
            return Task::none();
        };
        match action {
            SessionAction::Fork => fork(client, id, None, None),
            SessionAction::ForkBefore(key) => {
                let text =
                    self.timeline_entries(&id).into_iter().flatten().find_map(
                        |entry| match entry {
                            Entry::Prompt { key: k, text, .. } if k == key => Some(text),
                            _ => None,
                        },
                    );
                fork(client, id, Some(key), text)
            }
            SessionAction::MoveTo(folder) => {
                self.act(move |client, id| async move { client.move_session(&id, &folder).await })
            }
            SessionAction::Export => {
                let name = self
                    .workspace
                    .session(&id)
                    .and_then(|s| s.title.clone())
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or(id);
                ask_export(&name)
            }
            SessionAction::ExportTo(path) => export(client, id, path),
            SessionAction::Import => ask_import(),
            SessionAction::ImportFrom(path) => {
                let folder = self
                    .workspace
                    .session(&id)
                    .map_or_else(home_folder, |s| s.directory.clone());
                import(client, path, folder)
            }
            SessionAction::Archive => {
                self.act(|client, id| async move { client.set_archived(&id, true).await })
            }
            SessionAction::CopyToHarlo => {
                Task::perform(harlo_core::import::from_opencode(Some(vec![id])), |done| {
                    Message::SessionMenu(SessionAction::Copied(done))
                })
            }
            SessionAction::AskDelete | SessionAction::Made(..) | SessionAction::Copied(_) => {
                Task::none()
            }
            SessionAction::Delete => {
                self.act(|client, id| async move { client.delete_session(&id).await })
            }
        }
    }
}

/// Forks session `id`, whole or before message `before`; the fork opens
/// with `text` in its composer.
fn fork(client: Client, id: String, before: Option<String>, text: Option<String>) -> Task<Message> {
    Task::perform(
        async move {
            let made = client.fork_session(&id, before.as_deref()).await;
            SessionAction::Made(
                made.map(Box::new)
                    .map_err(|e| format!("Couldn't fork: {e}")),
                text,
            )
        },
        Message::SessionMenu,
    )
}

/// Asks where to save a session named `name`.
fn ask_export(name: &str) -> Task<Message> {
    let file = rfd::AsyncFileDialog::new()
        .set_title("Export session")
        .set_file_name(format!("{}.json", file_name(name)))
        .add_filter("Session", &["json"])
        .save_file();
    Task::future(async move {
        match file.await {
            Some(file) => Message::SessionMenu(SessionAction::ExportTo(file.path().to_path_buf())),
            None => Message::ClosePicker,
        }
    })
}

/// Saves session `id`'s transcript to `path`.
fn export(client: Client, id: String, path: PathBuf) -> Task<Message> {
    Task::perform(
        async move {
            let transcript = client
                .export_session(&id)
                .await
                .map_err(|e| e.to_string())?;
            tokio::fs::write(&path, transcript)
                .await
                .map_err(|e| e.to_string())
        },
        |done: Result<(), String>| {
            Message::Acted(done.map_err(|e| format!("Couldn't export: {e}")))
        },
    )
}

/// Asks for a transcript to import.
fn ask_import() -> Task<Message> {
    let file = rfd::AsyncFileDialog::new()
        .set_title("Import session")
        .add_filter("Session", &["json"])
        .pick_file();
    Task::future(async move {
        match file.await {
            Some(file) => {
                Message::SessionMenu(SessionAction::ImportFrom(file.path().to_path_buf()))
            }
            None => Message::ClosePicker,
        }
    })
}

/// Imports the transcript in `path` into `folder`; it opens once made.
fn import(client: Client, path: PathBuf, folder: String) -> Task<Message> {
    Task::perform(
        async move {
            let made = async {
                let transcript = tokio::fs::read_to_string(&path)
                    .await
                    .map_err(|e| e.to_string())?;
                client
                    .import_session(&transcript, &folder)
                    .await
                    .map_err(|e| e.to_string())
            }
            .await;
            SessionAction::Made(
                made.map(Box::new)
                    .map_err(|e| format!("Couldn't import: {e}")),
                None,
            )
        },
        Message::SessionMenu,
    )
}

impl App {
    /// Switches to Harlo's server, where the copied session now is; or
    /// says why it didn't come over.
    fn copied(&mut self, done: Result<harlo_core::import::Imported, String>) -> Task<Message> {
        match done {
            Ok(imported) if imported.failed.is_empty() => {
                let copied = self.active.clone();
                let switch = self.pick_location(harlo_core::server::Location::Engine);
                // It opens there once connected, with the tabs left open there.
                if let Some(id) = copied {
                    let tabs = self.restore.get_or_insert_with(Default::default);
                    if !tabs.open.contains(&id) {
                        tabs.open.push(id.clone());
                    }
                    tabs.active = Some(id);
                }
                switch
            }
            other => {
                self.send_error = Some(super::computers::imported_text(&other));
                Task::none()
            }
        }
    }

    /// Opens a fork or an import that was made, with `text` in the
    /// composer when there is some; or says why it wasn't.
    fn made(&mut self, made: Result<Box<Session>, String>, text: Option<String>) -> Task<Message> {
        match made {
            Ok(session) => {
                let id = session.id.clone();
                let fetches = self.workspace.upsert(*session);
                self.refresh();
                let open = self.open(id.clone());
                if let Some(text) = text {
                    self.composer.set(&text);
                }
                Task::batch([open, self.load(id), self.fetch(fetches)])
            }
            Err(reason) => {
                self.send_error = Some(reason);
                Task::none()
            }
        }
    }
}

/// `title` as a file name: the characters file systems refuse become `-`.
fn file_name(title: &str) -> String {
    title
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect()
}
