//! Widget IDs for controls that have no text of their own to find them by.

// Rust guideline compliant 2026-10-09

/// The left rail's sidebar toggle.
pub const SIDEBAR_TOGGLE: &str = "sidebar-toggle";

/// The session filter.
pub const SESSION_FILTER: &str = "session-filter";

/// The right panel's close button.
pub const PANEL_CLOSE: &str = "panel-close";

/// The composer box.
pub const COMPOSER: &str = "composer";

/// The location picker's button.
pub const LOCATION: &str = "location";

/// The composer's send button.
pub const SEND: &str = "send";

/// The active tab's close button.
pub const TAB_CLOSE: &str = "tab-close";

/// The folder picker on the start screen.
pub const FOLDER: &str = "folder";

/// The composer's attach button.
pub const ATTACH: &str = "attach";

/// The stop button, shown while the session works.
pub const STOP: &str = "stop";

/// The composer's `/` and `@` menu.
pub const MENU: &str = "composer-menu";

/// The agent picker.
pub const AGENT: &str = "agent";

/// The reasoning level picker.
pub const VARIANT: &str = "variant";

/// The model picker.
pub const MODEL: &str = "model";

/// The composer's shield, which sets what asks first.
pub const SHIELD: &str = "shield";

/// The card asking to approve an action.
pub const APPROVAL: &str = "approval";

/// The approval card's "Allow once" button.
pub const ALLOW: &str = "allow";

/// The reason typed when refusing an approval.
pub const REFUSE_REASON: &str = "refuse-reason";

/// The card with the agent's questions.
pub const QUESTION: &str = "question";

/// The question card's "Send answer" button.
pub const ANSWER: &str = "answer";

/// The card for a staged undo.
pub const UNDO: &str = "undo";

/// The undo card's "Undo" button, which keeps it.
pub const UNDO_KEEP: &str = "undo-keep";

/// The Changes panel's commit message.
pub const COMMIT_MESSAGE: &str = "commit-message";
