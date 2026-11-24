//! Settings → Schedules: The server's automations that send a prompt to a
//! session, or run a shell command or code, at a time, every so often or
//! on a cron schedule. The server
//! keeps and runs them, so they run with Harlo closed; Harlo lists them,
//! makes new ones, and pauses, resumes and deletes them.
//!
//! The server runs automations unless `automations` is `false` in its
//! config. The switch here changes that in the global config, only when
//! the user asks, and the page says it changes every client's server.

// Rust guideline compliant 2026-10-09

use std::path::Path;

use harlo_core::config;
use harlo_core::schedules::{Does, When};
use harlo_core::server::{
    Automation, AutomationAction, Effect as RuleEffect, NewAction, NewAutomation, Rule,
};

use crate::task::Task;

use super::sheets::{Event as SheetEvent, Field, Load};
use super::{App, Message};

/// Something that happened in Settings → Schedules.
#[derive(Debug, Clone, PartialEq)]
pub enum ScheduleEvent {
    /// Whether the server's config allows automations, as read.
    Allowed(Result<bool, String>),
    /// Allow them, or stop allowing them.
    ToggleAllowed,
    /// They were allowed or not, or why the config couldn't change.
    AllowedSet(Result<bool, String>),
    /// Load the list again.
    Refresh,
    /// The list, as the server has it.
    Loaded(Result<Vec<Automation>, String>),
    /// Show a schedule's page, by ID.
    Open(String),
    /// Back to the list.
    Back,
    /// Start a new schedule.
    New,
    /// The project a new schedule runs in.
    Folder(String),
    /// The session a new schedule's prompt goes to; `None` for a new one.
    Session(Option<String>),
    /// Create the new schedule.
    Create,
    /// It was created, or why not.
    Created(Result<Automation, String>),
    /// Pause a schedule, by ID.
    Pause(String),
    /// Resume a schedule, by ID.
    Resume(String),
    /// A schedule as it is after pausing or resuming, or why that failed.
    Changed(Result<Automation, String>),
    /// Delete a schedule, by ID: the first time asks again.
    Delete(String),
    /// A schedule was deleted, or why not.
    Deleted(String, Result<(), String>),
    /// Close Settings and open a schedule's session, by ID.
    OpenSession(String),
}

/// Which page Settings → Schedules shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Page {
    /// Every schedule.
    #[default]
    List,
    /// One schedule, by ID.
    Shown(String),
    /// The form for a new one.
    New,
}

/// A new schedule, as the form holds it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Draft {
    /// Its name.
    pub name: String,
    /// How its time is given.
    pub when: When,
    /// The time, as typed.
    pub time: String,
    /// What it does each time.
    pub does: Does,
    /// The project folder it runs in.
    pub folder: Option<String>,
    /// The session its prompt goes to; `None` for a new one named after
    /// the schedule.
    pub session: Option<String>,
    /// The prompt, command or code, by what it does.
    pub prompt: String,
}

/// The session a schedule belongs to: the one its prompt goes to, or the
/// one that made it, such as an agent's.
#[must_use]
pub fn session_of(automation: &Automation) -> Option<&str> {
    match &automation.action {
        AutomationAction::Prompt {
            session: Some(id), ..
        } => Some(id),
        _ => automation.session_id.as_deref(),
    }
}

/// Settings → Schedules' state.
#[derive(Debug, Default)]
pub struct Schedules {
    /// Whether the server's global config allows automations.
    pub allowed: Load<bool>,
    /// Why allowing them couldn't change.
    pub allow_error: Option<String>,
    /// The schedules.
    pub list: Load<Vec<Automation>>,
    /// The page shown.
    pub page: Page,
    /// The new schedule being written.
    pub draft: Draft,
    /// Whether a change is on its way to the server.
    pub busy: bool,
    /// Why the last change failed.
    pub error: Option<String>,
    /// The schedule whose Delete was pressed once, by ID.
    pub deleting: Option<String>,
}

impl Schedules {
    /// The schedule with `id`, if it's loaded.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Automation> {
        match &self.list {
            Load::Loaded(list) => list.iter().find(|a| a.id == id),
            _ => None,
        }
    }

    /// Puts `automation` in the list in place of the one with its ID, or
    /// at its end.
    fn put(&mut self, automation: Automation) {
        if let Load::Loaded(list) = &mut self.list {
            match list.iter_mut().find(|a| a.id == automation.id) {
                Some(old) => *old = automation,
                None => list.push(automation),
            }
        }
    }
}

impl App {
    /// Applies something that happened in Settings → Schedules.
    pub(super) fn schedule_event(&mut self, event: ScheduleEvent) -> Task<Message> {
        if matches!(event, ScheduleEvent::Folder(_) | ScheduleEvent::Session(_)) {
            // Chosen from a menu, which closes.
            self.picker = None;
        }
        let page = &mut self.sheets.schedules;
        match event {
            ScheduleEvent::Allowed(read) => {
                page.allowed = read.map_or_else(Load::Failed, Load::Loaded);
            }
            ScheduleEvent::ToggleAllowed => return self.toggle_allowed(),
            ScheduleEvent::AllowedSet(Ok(on)) => page.allowed = Load::Loaded(on),
            ScheduleEvent::AllowedSet(Err(why)) => page.allow_error = Some(why),
            ScheduleEvent::Refresh => return self.load_schedules(),
            ScheduleEvent::Loaded(list) => page.list = list.map_or_else(Load::Failed, Load::Loaded),
            ScheduleEvent::Open(id) => {
                page.page = Page::Shown(id);
                page.error = None;
                page.deleting = None;
            }
            ScheduleEvent::Back => {
                page.page = Page::List;
                page.error = None;
                page.deleting = None;
            }
            ScheduleEvent::New => {
                let folder = self.schedule_folder();
                let page = &mut self.sheets.schedules;
                page.draft = Draft {
                    folder,
                    ..Draft::default()
                };
                page.page = Page::New;
                page.error = None;
            }
            ScheduleEvent::Folder(folder) => {
                if page.draft.folder.as_deref() != Some(folder.as_str()) {
                    page.draft.folder = Some(folder);
                    // A session belongs to one folder.
                    page.draft.session = None;
                }
            }
            ScheduleEvent::Session(session) => page.draft.session = session,
            ScheduleEvent::Create => return self.create_schedule(),
            ScheduleEvent::Created(Ok(automation)) => {
                page.busy = false;
                page.page = Page::Shown(automation.id.clone());
                page.put(automation);
            }
            ScheduleEvent::Created(Err(why)) | ScheduleEvent::Changed(Err(why)) => {
                page.busy = false;
                page.error = Some(why);
            }
            ScheduleEvent::Pause(id) => {
                return self.change_schedule(id, |client, id| async move {
                    client.pause_automation(&id).await
                });
            }
            ScheduleEvent::Resume(id) => {
                return self.change_schedule(id, |client, id| async move {
                    client.resume_automation(&id).await
                });
            }
            ScheduleEvent::Changed(Ok(automation)) => {
                page.busy = false;
                page.put(automation);
            }
            ScheduleEvent::Delete(id) => {
                if page.deleting.as_deref() != Some(id.as_str()) {
                    page.deleting = Some(id);
                    return Task::none();
                }
                return self.delete_schedule(id);
            }
            ScheduleEvent::Deleted(id, result) => {
                page.busy = false;
                page.deleting = None;
                match result {
                    Ok(()) => {
                        if let Load::Loaded(list) = &mut page.list {
                            list.retain(|a| a.id != id);
                        }
                        page.page = Page::List;
                    }
                    Err(why) => page.error = Some(why),
                }
            }
            ScheduleEvent::OpenSession(id) => {
                return Task::batch([
                    Task::done(Message::Sheet(SheetEvent::Close)),
                    Task::done(Message::OpenSession(id)),
                ]);
            }
        }
        Task::none()
    }

    /// Takes the text of a new schedule's field, as it's typed.
    pub(super) fn schedule_text(&mut self, field: Field, value: &str) -> Task<Message> {
        let draft = &mut self.sheets.schedules.draft;
        match field {
            Field::ScheduleName => value.clone_into(&mut draft.name),
            Field::ScheduleTime => value.clone_into(&mut draft.time),
            Field::SchedulePrompt => value.clone_into(&mut draft.prompt),
            _ => {}
        }
        Task::none()
    }

    /// Reads whether the server allows automations, where Harlo may change
    /// that, and lists the schedules.
    pub(super) fn load_schedules(&mut self) -> Task<Message> {
        let list = match self.client.clone() {
            Some(client) => {
                if !matches!(self.sheets.schedules.list, Load::Loaded(_)) {
                    self.sheets.schedules.list = Load::Loading;
                }
                Task::perform(
                    async move { client.automations().await.map_err(|e| e.to_string()) },
                    |list| schedule_message(ScheduleEvent::Loaded(list)),
                )
            }
            None => Task::none(),
        };
        let Some(folder) = self.global_config().map(Path::to_path_buf) else {
            self.sheets.schedules.allowed = Load::Idle;
            return list;
        };
        let allowed = Task::perform(
            async move {
                tokio::task::spawn_blocking(move || config::automations(&folder))
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|read| read.map_err(|e| e.to_string()))
            },
            |read| schedule_message(ScheduleEvent::Allowed(read)),
        );
        Task::batch([list, allowed])
    }

    /// The folder a new schedule starts in: the one new sessions start in,
    /// or the first project's.
    fn schedule_folder(&self) -> Option<String> {
        self.new_folder
            .clone()
            .or_else(|| self.projects.first().map(|p| p.directory.clone()))
    }

    /// Allows automations in the server's global config, or stops allowing
    /// them.
    fn toggle_allowed(&mut self) -> Task<Message> {
        let folder = self.global_config().map(Path::to_path_buf);
        let page = &mut self.sheets.schedules;
        let (Load::Loaded(on), Some(folder)) = (&page.allowed, folder) else {
            return Task::none();
        };
        let on = !*on;
        page.allow_error = None;
        Task::perform(config::set_automations_later(folder, on), move |result| {
            schedule_message(ScheduleEvent::AllowedSet(
                result
                    .map(|()| on)
                    .map_err(|e| format!("Couldn't change opencode.json: {e}")),
            ))
        })
    }

    /// Creates the new schedule: a session for it first when it gets a new
    /// one, archived again if the server refuses the schedule.
    fn create_schedule(&mut self) -> Task<Message> {
        let page = &mut self.sheets.schedules;
        if page.busy {
            return Task::none();
        }
        let draft = page.draft.clone();
        let missing = if draft.name.trim().is_empty() {
            Some("Give it a name.")
        } else if draft.time.trim().is_empty() {
            Some("Say when it runs.")
        } else if draft.prompt.trim().is_empty() {
            Some(draft.does.missing())
        } else if draft.folder.is_none() {
            Some("Choose a project.")
        } else {
            None
        };
        if let Some(missing) = missing {
            page.error = Some(missing.to_owned());
            return Task::none();
        }
        let (Some(client), Some(folder)) = (self.client.clone(), draft.folder.clone()) else {
            return Task::none();
        };
        page.busy = true;
        page.error = None;
        let name = draft.name.trim().to_owned();
        Task::perform(
            async move {
                let text = draft.prompt.trim().to_owned();
                let trigger = draft.when.trigger(&draft.time);
                // Nobody is there to approve a command or code when it's
                // created, so it gets a session of its own whose rules
                // allow it: the user asked for it here.
                let (action, allowed) = match draft.does {
                    Does::Shell => (
                        NewAction::Shell(text.clone()),
                        Some(Rule {
                            action: "shell".to_owned(),
                            resource: text,
                            effect: RuleEffect::Allow,
                        }),
                    ),
                    Does::Code => (
                        NewAction::Code(text),
                        Some(Rule::any("execute", RuleEffect::Allow)),
                    ),
                    Does::Prompt => (NewAction::Prompt(text), None),
                };
                let (session, made) = if let Some(id) = draft.session.filter(|_| allowed.is_none())
                {
                    (id, false)
                } else {
                    let session = client.create_session(&folder).await?;
                    client.set_title(&session.id, &name).await?;
                    if let Some(rule) = allowed {
                        client.set_permissions(&session.id, &[rule]).await?;
                    }
                    (session.id, true)
                };
                let new = NewAutomation {
                    name,
                    session_id: Some(session.clone()),
                    directory: None,
                    trigger,
                    action,
                };
                let created = client.create_automation(&new).await;
                if created.is_err() && made {
                    // Nothing will use it; it leaves the lists.
                    let _ = client.set_archived(&session, true).await;
                }
                created
            },
            |created| {
                schedule_message(ScheduleEvent::Created(
                    created.map_err(|e: harlo_core::server::Error| said(&e)),
                ))
            },
        )
    }

    /// Pauses or resumes the schedule with `id` through `change`.
    fn change_schedule<F, Fut>(&mut self, id: String, change: F) -> Task<Message>
    where
        F: FnOnce(harlo_core::server::Client, String) -> Fut,
        Fut: Future<Output = Result<Automation, harlo_core::server::Error>> + Send + 'static,
    {
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        let page = &mut self.sheets.schedules;
        if page.busy {
            return Task::none();
        }
        page.busy = true;
        page.error = None;
        let future = change(client, id);
        Task::perform(
            async move { future.await.map_err(|e| said(&e)) },
            |result| schedule_message(ScheduleEvent::Changed(result)),
        )
    }

    /// Deletes the schedule with `id`.
    fn delete_schedule(&mut self, id: String) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        let page = &mut self.sheets.schedules;
        page.busy = true;
        page.error = None;
        Task::perform(
            async move {
                let result = client.delete_automation(&id).await.map_err(|e| said(&e));
                (id, result)
            },
            |(id, result)| schedule_message(ScheduleEvent::Deleted(id, result)),
        )
    }
}

/// What the server said about a refused change, or the whole error when
/// it said nothing.
fn said(error: &harlo_core::server::Error) -> String {
    error
        .server_message()
        .map_or_else(|| error.to_string(), ToOwned::to_owned)
}

/// `event` as the app's message.
pub(crate) fn schedule_message(event: ScheduleEvent) -> Message {
    Message::Sheet(SheetEvent::Schedule(event))
}
