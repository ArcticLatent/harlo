//! Desktop notifications: a run finished, failed, or the agent needs the
//! user. Clicking one shows the window on its session.
//!
//! What goes out follows Settings → Notifications: the events switched on,
//! quiet hours, whether to notify while Harlo has focus (never for the
//! session in view), the sound, and each event's title and message
//! templates.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt as _;
use harlo_core::approvals;
use harlo_core::git::Repo;
use harlo_core::notifications::{
    Kind, LAST_MESSAGE_CHARS, Notifications, Values, local_minute, summary,
};
use harlo_core::workspace::EventKind;

use super::waiting::capitalized;
use super::{App, Message};
use crate::task::{Effect, Task};

/// The name notifications are sent under.
pub(super) const APP_NAME: &str = "Harlo";

/// The freedesktop icon shown with a notification.
const NOTICE_ICON: &str = "dialog-information";

/// The freedesktop sound a notification plays, when sounds are on.
const NOTICE_SOUND: &str = "message-new-instant";

/// How long a click on a notification still opens its session; the desktop
/// may keep it in its list long after it shows.
const CLICK_WAIT: Duration = Duration::from_hours(4);

/// What the notifications quote, per session.
#[derive(Debug, Default)]
pub(super) struct Notices {
    /// Whether notifications are shown at all; tests leave them off.
    pub(super) enabled: bool,
    /// Whether the window has focus.
    pub(super) focused: bool,
    /// Each session's last answer text, or its last error, since its run
    /// started.
    last_words: HashMap<String, String>,
}

impl Notices {
    /// Notifications on or off, starting focused.
    pub(super) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            focused: true,
            last_words: HashMap::new(),
        }
    }
}

/// A notification Harlo meant to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// What happened.
    pub kind: Kind,
    /// The session it's about, which a click opens; none for a test.
    pub session_id: Option<String>,
    /// Its title, as rendered before a `{branch}` is read.
    pub title: String,
    /// Its message, likewise.
    pub body: String,
}

impl App {
    /// Notes what a later notification may quote, and notifies for a run
    /// that ended or an agent that needs the user, as Settings allows.
    pub(super) fn notice(&mut self, kind: &EventKind) -> Task<Message> {
        let (kind, session, said) = match kind {
            EventKind::Busy { session_id } => {
                self.notices.last_words.remove(session_id);
                return Task::none();
            }
            EventKind::TextEnded {
                session_id, text, ..
            } if !text.trim().is_empty() => {
                let words = crate::markdown::plain(text);
                self.notices
                    .last_words
                    .insert(session_id.clone(), summary(&words, LAST_MESSAGE_CHARS));
                return Task::none();
            }
            EventKind::StepFailed {
                session_id, error, ..
            } => {
                self.notices
                    .last_words
                    .insert(session_id.clone(), summary(error, LAST_MESSAGE_CHARS));
                return Task::none();
            }
            EventKind::Done {
                session_id,
                outcome,
            } => {
                let kind = match outcome.as_str() {
                    "succeeded" => Kind::Finished,
                    "failed" => Kind::Failed,
                    // Stopped by the user: nothing to tell them.
                    _ => return Task::none(),
                };
                // A subagent's run ends inside its parent's, which says so.
                if self
                    .workspace
                    .session(session_id)
                    .is_some_and(|s| s.parent_id.is_some())
                {
                    return Task::none();
                }
                let said = self
                    .notices
                    .last_words
                    .get(session_id)
                    .cloned()
                    .unwrap_or_default();
                (kind, session_id, said)
            }
            EventKind::PermissionAsked(asked) => {
                let said = approvals::describe(asked);
                let mut text = format!("Wants to {}", said.wants);
                if let Some(detail) = said.detail.lines().next().filter(|l| !l.is_empty()) {
                    text.push_str(": ");
                    text.push_str(detail);
                }
                (Kind::Asks, &asked.session_id, text)
            }
            EventKind::FormCreated(form) => {
                let question = form
                    .fields
                    .iter()
                    .find(|f| !f.hidden)
                    .and_then(|f| f.description.clone().or_else(|| f.title.clone()))
                    .unwrap_or_else(|| "Has a question".to_owned());
                (Kind::Asks, &form.session_id, question)
            }
            _ => return Task::none(),
        };
        let settings = &self.preferences.notifications;
        if !settings.allows(kind, local_minute(self.now)) {
            return Task::none();
        }
        if self.notices.focused
            && (!settings.while_focused || self.active.as_deref() == Some(session.as_str()))
        {
            return Task::none();
        }
        let values = Values {
            last_message: summary(&said, LAST_MESSAGE_CHARS),
            ..self.values(session)
        };
        let session = session.clone();
        self.send_notice(kind, Some(session), values)
    }

    /// A notification as a finished run with made-up details, whatever the
    /// focus, quiet hours and event switches say.
    pub(super) fn test_notice(&mut self) -> Task<Message> {
        let values = Values {
            project_name: "my-project".to_owned(),
            worktree: String::new(),
            branch: "main".to_owned(),
            session_name: "Try notifications".to_owned(),
            agent_name: "Build".to_owned(),
            model_name: "Big Pickle".to_owned(),
            last_message: "This is how a finished run looks.".to_owned(),
        };
        self.send_notice(Kind::Finished, None, values)
    }

    /// Shows the notification for `kind` with `values`, reading the branch
    /// first when a template asks for it, and opens its session on a click.
    fn send_notice(
        &mut self,
        kind: Kind,
        session_id: Option<String>,
        values: Values,
    ) -> Task<Message> {
        let settings = self.preferences.notifications.clone();
        let (title, body) = settings.render(kind, &values);
        self.last_notice = Some(Notice {
            kind,
            session_id: session_id.clone(),
            title,
            body,
        });
        if !self.notices.enabled {
            return Task::none();
        }
        let folder = session_id
            .as_deref()
            .and_then(|id| self.workspace.session(id))
            .filter(|_| self.location.is_local())
            .map(|s| s.directory.clone());
        let wants_branch = uses_branch(&settings, kind);
        let opened = async move {
            let mut values = values;
            if wants_branch && let Some(folder) = folder {
                values.branch = Repo::new(folder)
                    .status()
                    .await
                    .ok()
                    .and_then(|s| s.branch)
                    .unwrap_or_default();
            }
            let (title, body) = settings.render(kind, &values);
            let clicked = show(&title, &body, settings.sound).await;
            clicked
                .then_some(session_id)
                .flatten()
                .map(Message::NoticeOpened)
        };
        Task::stream(futures_util::stream::once(opened).filter_map(std::future::ready))
    }

    /// The variables for `session`, all but `{last_message}`.
    fn values(&self, session: &str) -> Values {
        let found = self.workspace.session(session);
        let folder = found.map(|s| s.directory.as_str()).unwrap_or_default();
        let catalog = self.catalogs.get(folder);
        // Without an agent of its own, a session runs its folder's default one.
        let agent_name = found
            .and_then(|s| {
                s.agent
                    .clone()
                    .or_else(|| Some(catalog?.primary_agents().next()?.id.clone()))
            })
            .map_or_else(|| "The agent".to_owned(), |name| capitalized(&name));
        let model_name = found
            .and_then(|s| s.model.as_ref())
            .map(|m| {
                catalog
                    .and_then(|c| c.model(&m.provider_id, &m.id))
                    .map_or_else(|| m.id.clone(), |model| model.name.clone())
            })
            .unwrap_or_default();
        let (project_name, worktree) = self
            .projects
            .iter()
            .find_map(|project| {
                if project.directory == folder {
                    return Some((project.name.clone(), String::new()));
                }
                let worktree = project.worktrees.iter().find(|w| w.directory == folder)?;
                Some((project.name.clone(), worktree.name.clone()))
            })
            .unwrap_or_default();
        Values {
            project_name,
            worktree,
            branch: String::new(),
            session_name: found
                .and_then(|s| s.title.clone())
                .unwrap_or_else(|| "Untitled session".to_owned()),
            agent_name,
            model_name,
            last_message: String::new(),
        }
    }

    /// Shows the window on the session whose notification was clicked.
    pub(super) fn notice_opened(&mut self, session_id: String) -> Task<Message> {
        Task::batch([Task::effect(Effect::Raise), self.open(session_id)])
    }
}

/// Whether `kind`'s templates use `{branch}`, which takes a git call.
fn uses_branch(settings: &Notifications, kind: Kind) -> bool {
    let template = settings.template(kind);
    let (title, message) = kind.defaults();
    [
        template.title.as_str(),
        template.message.as_str(),
        title,
        message,
    ]
    .iter()
    .any(|t| t.contains("{branch}"))
}

/// Shows a notification, and returns whether it was clicked.
///
/// Without a notification service there's nothing to show it on; the
/// session still shows what happened.
async fn show(title: &str, body: &str, sound: bool) -> bool {
    let mut notice = notify_rust::Notification::new();
    notice
        .appname(APP_NAME)
        .summary(title)
        .body(body)
        .icon(NOTICE_ICON)
        // Clicking the notification itself.
        .action("default", "Open");
    if sound {
        notice.sound_name(NOTICE_SOUND);
    } else {
        notice.hint(notify_rust::Hint::SuppressSound(true));
    }
    let Ok(handle) = notice.show_async().await else {
        return false;
    };
    let mut clicked = false;
    let wait = handle.wait_for_action_async(|response| {
        clicked = matches!(response, notify_rust::NotificationResponse::Default)
            || matches!(response, notify_rust::NotificationResponse::Action(a) if a == "default");
    });
    // Gone unclicked after a while: nothing to open.
    let _ = tokio::time::timeout(CLICK_WAIT, wait).await;
    clicked
}
