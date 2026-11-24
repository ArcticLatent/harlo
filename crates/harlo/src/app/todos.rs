//! The agent's todo list: the one its last todo call wrote, and whether
//! the folder's agents have the todo tool at all.

// Rust guideline compliant 2026-10-09

use harlo_core::timeline::{Todo, current_todos};

use super::App;

impl App {
    /// A session's todo list as its agent last wrote it, once its history is loaded.
    #[must_use]
    pub fn todos(&self, session: &str) -> Option<&[Todo]> {
        current_todos(self.timelines.get(session)?.entries())
    }

    /// Whether agents in the current folder have the todo tool.
    ///
    /// The server leaves it off unless the configuration turns on
    /// `experimental.todo`; Harlo only reads that setting.
    #[must_use]
    pub fn todo_tool(&self) -> bool {
        self.catalog().is_some_and(|c| c.todo_tool)
    }
}
