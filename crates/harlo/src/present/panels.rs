//! The tool panels' bodies: Context (how full the context window is, what
//! the session used and cost, its instruction files) and Todos (the agent's
//! list as its last todo call wrote it).
//!
//! Each is filled only while it's on screen.

// Rust guideline compliant 2026-10-09

use std::path::Path;
use std::rc::Rc;

use harlo_core::instructions::{self, Scope};
use harlo_core::server::Tokens;
use harlo_core::timeline::{Todo, TodoStatus, todo_counts, todo_progress};
use slint::VecModel;

use super::sync;
use crate::app::{App, ContextFacts, Panel};
use crate::model::{TODO_SETTING, compact, dollars};
use crate::ui::{ContextCard, Fact, InstructionFile, State, TodoRow, TodosCard};

/// How full the window gets before its bar warns, from 0 to 1.
///
/// At 80% a long answer or a few file reads can fill the rest, so it's time
/// to think about compacting.
const WARN_AT: f32 = 0.8;

/// The lists the panels show, kept for the window's lifetime.
pub struct Lists {
    facts: Rc<VecModel<Fact>>,
    files: Rc<VecModel<InstructionFile>>,
    todos: Rc<VecModel<TodoRow>>,
}

impl std::fmt::Debug for Lists {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lists").finish_non_exhaustive()
    }
}

impl Lists {
    /// Gives the window the panels' lists.
    pub fn new(state: &State<'_>) -> Self {
        let lists = Self {
            facts: Rc::new(VecModel::default()),
            files: Rc::new(VecModel::default()),
            todos: Rc::new(VecModel::default()),
        };
        state.set_context_facts(Rc::clone(&lists.facts).into());
        state.set_context_files(Rc::clone(&lists.files).into());
        state.set_todo_rows(Rc::clone(&lists.todos).into());
        lists
    }

    /// Fills the panel on screen.
    pub fn show(&self, app: &App, state: &State<'_>) {
        if app.panel_shown(Panel::Context) {
            self.context(app, state);
        }
        if app.panel_shown(Panel::Todos) {
            self.todos(app, state);
        }
    }

    /// The Context panel.
    fn context(&self, app: &App, state: &State<'_>) {
        let Some(facts) = app.context_facts() else {
            state.set_context_card(ContextCard {
                note: "No session selected.".into(),
                ..ContextCard::default()
            });
            sync(&self.facts, Vec::new());
            sync(&self.files, Vec::new());
            return;
        };
        let mut card = window(&facts);
        sync(&self.facts, usage(&facts));
        let (files, note) = instruction_files(&facts);
        card.files_note = note.into();
        sync(&self.files, files);
        state.set_context_card(card);
    }

    /// The Todos panel.
    fn todos(&self, app: &App, state: &State<'_>) {
        let note = |words: &str| TodosCard {
            note: words.into(),
            ..TodosCard::default()
        };
        let todos = app.active.as_deref().and_then(|id| app.todos(id));
        let card = match todos {
            _ if app.active.is_none() => note("No session selected."),
            Some([]) => note("The agent cleared its list."),
            Some(todos) => {
                let (done, total) = todo_counts(todos);
                TodosCard {
                    progress: todo_progress(todos).into(),
                    done: count(done),
                    total: count(total),
                    ..TodosCard::default()
                }
            }
            None if app.todo_tool() => {
                note("No list yet. The agent writes one when a task has several steps.")
            }
            None => TodosCard {
                off: true,
                setting: TODO_SETTING.into(),
                ..note("Agents here don't keep a todo list.")
            },
        };
        state.set_todos_card(card);
        sync(
            &self.todos,
            todos.unwrap_or_default().iter().map(todo).collect(),
        );
    }
}

/// How full the context window is, when it's known.
fn window(facts: &ContextFacts<'_>) -> ContextCard {
    if let Some(error) = facts.error {
        return ContextCard {
            error: error.into(),
            ..ContextCard::default()
        };
    }
    if facts.loading {
        return ContextCard {
            window_note: "Reading what the model sees…".into(),
            ..ContextCard::default()
        };
    }
    let Some(step) = facts.context.and_then(|c| c.last_step) else {
        return ContextCard {
            window_note: "Nothing in it until the model answers.".into(),
            ..ContextCard::default()
        };
    };
    let used = step.total();
    let mut card = ContextCard {
        used: compact(used).into(),
        of: facts
            .limit
            .map_or_else(
                || "tokens".to_owned(),
                |l| format!("of {} tokens", compact(l)),
            )
            .into(),
        ..ContextCard::default()
    };
    if let Some(limit) = facts.limit.filter(|&l| l > 0) {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a share of the window needs no more than f32's precision"
        )]
        let fill = (used as f32 / limit as f32).min(1.0);
        let percent = used.saturating_mul(100) / limit;
        card.bar = true;
        card.fill = fill;
        card.warn = fill >= WARN_AT;
        card.percent = if percent == 0 && used > 0 {
            "Under 1% full".to_owned()
        } else {
            format!("{percent}% full")
        }
        .into();
    }
    if let Some(context) = facts.context {
        let messages = if context.messages == 1 {
            "1 message".to_owned()
        } else {
            format!("{} messages", context.messages)
        };
        card.messages = format!("{messages} since the last compaction").into();
    }
    card
}

/// The model, agent, and what the session used and cost.
fn usage(facts: &ContextFacts<'_>) -> Vec<Fact> {
    let fact = |label: &str, value: String| Fact {
        label: label.into(),
        value: value.into(),
    };
    let mut rows = Vec::new();
    if let Some(model) = &facts.model {
        rows.push(fact("Model", model.clone()));
    }
    if let Some(agent) = &facts.agent {
        rows.push(fact("Agent", agent.clone()));
    }
    let Tokens {
        input,
        output,
        reasoning,
        cache_read,
        cache_write,
    } = facts.tokens;
    rows.push(fact("Input", compact(input)));
    rows.push(fact("Output", compact(output)));
    // Only some providers count these; nothing to say where they don't.
    for (label, n) in [
        ("Reasoning", reasoning),
        ("Cache read", cache_read),
        ("Cache write", cache_write),
    ] {
        if n > 0 {
            rows.push(fact(label, compact(n)));
        }
    }
    rows.push(fact("Cost", dollars(facts.cost)));
    rows
}

/// The instruction files given from the start, then those picked up on the
/// way, and what the section says besides.
fn instruction_files(facts: &ContextFacts<'_>) -> (Vec<InstructionFile>, &'static str) {
    let row = |path: &Path, aside: &str| InstructionFile {
        path: instructions::display_in(path, facts.root, facts.home).into(),
        aside: aside.into(),
    };
    let loaded = facts
        .context
        .map(|c| c.loaded.as_slice())
        .unwrap_or_default();
    let mut rows = Vec::new();
    let note = match facts.files {
        Some([]) if loaded.is_empty() => "No AGENTS.md or CLAUDE.md applies to this session.",
        Some(files) => {
            for file in files {
                let scope = match file.scope {
                    Scope::Global => "everywhere",
                    Scope::Project => "project",
                };
                rows.push(row(&file.path, scope));
            }
            ""
        }
        None if facts.loading => "",
        None => "The files given from the start can only be listed for folders on this computer.",
    };
    for path in loaded {
        rows.push(row(Path::new(path), "picked up"));
    }
    (rows, note)
}

/// A todo's row.
fn todo(todo: &Todo) -> TodoRow {
    TodoRow {
        text: todo.content.as_str().into(),
        status: match todo.status {
            TodoStatus::Pending => 0,
            TodoStatus::InProgress => 1,
            TodoStatus::Completed => 2,
            TodoStatus::Cancelled => 3,
        },
    }
}

/// A count as the UI's `int`.
fn count(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}
