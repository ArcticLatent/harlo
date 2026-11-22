//! Notes: free text kept per folder, where lines like `- [ ] task` are todos.
//!
//! The text is stored by [`crate::layout::Store`]; this module reads todos
//! out of it and ticks them.

// Rust guideline compliant 2026-10-09

use std::time::Duration;

/// How long after the last keystroke a note is saved.
pub const SAVE_DELAY: Duration = Duration::from_millis(500);

/// How an open todo line starts.
pub const TODO: &str = "- [ ] ";

/// How a done todo line starts.
const DONE: &str = "- [x] ";

/// How a done todo line may also start.
const DONE_UPPER: &str = "- [X] ";

/// Whether a line is a todo, and whether it's done.
#[must_use]
pub fn todo_state(line: &str) -> Option<bool> {
    let line = line.trim_start();
    if line.starts_with(TODO) || line.trim_end() == TODO.trim_end() {
        Some(false)
    } else if line.starts_with(DONE)
        || line.starts_with(DONE_UPPER)
        || line.trim_end() == DONE.trim_end()
        || line.trim_end() == DONE_UPPER.trim_end()
    {
        Some(true)
    } else {
        None
    }
}

/// The line with its todo ticked or unticked; a line that isn't a todo
/// becomes an open one.
#[must_use]
pub fn toggle_todo(line: &str) -> String {
    let indent = &line[..line.len() - line.trim_start().len()];
    let rest = line.trim_start();
    let body = |marker: &str| rest.get(marker.len()..).unwrap_or("").to_owned();
    match todo_state(line) {
        Some(false) => format!("{indent}{DONE}{}", body(TODO)),
        Some(true) => format!("{indent}{TODO}{}", body(DONE)),
        None => {
            // A plain list item keeps its text.
            let text = rest
                .strip_prefix("- ")
                .or_else(|| rest.strip_prefix("* "))
                .unwrap_or(rest);
            format!("{indent}{TODO}{text}")
        }
    }
}

/// How many todos are open and how many there are in all.
#[must_use]
pub fn todo_counts(text: &str) -> (usize, usize) {
    text.lines()
        .filter_map(todo_state)
        .fold((0, 0), |(open, all), done| {
            (open + usize::from(!done), all + 1)
        })
}

/// The text with a new open todo added on a line of its own at the end.
#[must_use]
pub fn add_todo(text: &str) -> String {
    let mut text = text.trim_end_matches([' ', '\t']).to_owned();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(TODO);
    text
}

/// Waits out [`SAVE_DELAY`], so a note is saved once typing pauses.
pub async fn pause_before_saving() {
    tokio::time::sleep(SAVE_DELAY).await;
}
