//! The tool panels' bodies: Context (how full the context window is, what
//! the session used and cost, its instruction files) and Todos (the agent's
//! list as its last todo call wrote it), Subagents and Side questions.
//!
//! Each is filled only while it's on screen.

// Rust guideline compliant 2026-10-09

use std::path::Path;
use std::rc::Rc;

use harlo_core::instructions::{self, Scope};
use harlo_core::server::Tokens;
use harlo_core::timeline::{Todo, TodoStatus, todo_counts, todo_progress};
use slint::VecModel;

use harlo_core::sessions::{SessionSummary, compact_age};

use super::sync;
use crate::app::{Answer, App, ContextFacts, Panel, capitalized};
use crate::markdown;
use crate::model::{TODO_SETTING, compact, dollars};
use crate::theme::color;
use crate::ui::{
    AsideRow, ContextCard, Fact, InstructionFile, State, SubagentRow, TodoRow, TodosCard,
};

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
    subagents: Rc<VecModel<SubagentRow>>,
    asides: Rc<VecModel<AsideRow>>,
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
            subagents: Rc::new(VecModel::default()),
            asides: Rc::new(VecModel::default()),
        };
        state.set_aside_rows(Rc::clone(&lists.asides).into());
        state.set_subagent_rows(Rc::clone(&lists.subagents).into());
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
        if app.panel_shown(Panel::Subagents) {
            self.subagents(app, state);
        }
        if app.panel_shown(Panel::Asides) {
            self.asides(app, state);
        }
    }

    /// The Side questions panel: the open session's, oldest first.
    fn asides(&self, app: &App, state: &State<'_>) {
        let asides = app.active.as_deref().map(|id| app.asides(id));
        let note = match asides {
            None => "No session selected.",
            Some([]) => {
                "No side questions yet. Type /btw and a question to ask about this session \
                 without adding to it."
            }
            Some(_) => "",
        };
        state.set_asides_note(note.into());
        sync(
            &self.asides,
            asides
                .unwrap_or_default()
                .iter()
                .map(|aside| aside_row(&app.tokens, &aside.question, &aside.answer))
                .collect(),
        );
    }

    /// The Subagents panel: the session's subagents, newest first, each
    /// opening its own session.
    fn subagents(&self, app: &App, state: &State<'_>) {
        let parent = app.subagent_parent();
        let note = match parent {
            _ if app.active.is_none() => "No session selected.",
            Some(parent) if !parent.children.is_empty() => "",
            _ => {
                "No subagents in this session. An agent starts one to work on part of a task apart."
            }
        };
        state.set_subagents_note(note.into());
        sync(
            &self.subagents,
            parent
                .map(|p| p.children.iter().map(|c| subagent_row(app, c)).collect())
                .unwrap_or_default(),
        );
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

/// A subagent's row: its agent, in the agent's colour, what it works on,
/// and how it stands.
fn subagent_row(app: &App, child: &SessionSummary) -> SubagentRow {
    let t = &app.tokens;
    let agent = child.agent.as_deref().and_then(|id| app.agent(id));
    let name = agent.as_ref().map_or_else(
        || {
            child
                .agent
                .as_deref()
                .map_or_else(|| "Subagent".to_owned(), capitalized)
        },
        |a| a.name.clone(),
    );
    SubagentRow {
        id: child.id.as_str().into(),
        agent: name.into(),
        title: if child.title.is_empty() {
            "Starting…".into()
        } else {
            child.title.as_str().into()
        },
        tone: color(
            agent
                .and_then(|a| a.color)
                .map_or_else(|| crate::theme::quiet_icon(t), |c| crate::theme::ink(t, c)),
        ),
        selected: app.active.as_deref() == Some(child.id.as_str()),
        busy: child.busy,
        waiting: child.waiting,
        asking: child.asking,
        detail: compact_age(child.age).into(),
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
            "Under 1% of what the model can hold".to_owned()
        } else {
            format!("{percent}% of what the model can hold")
        }
        .into();
    }
    if let Some(context) = facts.context {
        let messages = if context.messages == 1 {
            "1 message".to_owned()
        } else {
            format!("{} messages", context.messages)
        };
        card.messages = format!("{messages} since it was last summarized").into();
    }
    card
}

/// The model, agent, and what the session used and cost.
fn usage(facts: &ContextFacts<'_>) -> Vec<Fact> {
    let fact = |label: &str, value: String| Fact {
        label: label.into(),
        value: value.into(),
        kind: 1,
    };
    let mut rows = Vec::new();
    if let Some(model) = &facts.model {
        rows.push(Fact {
            kind: 0,
            ..fact("Model", model.clone())
        });
    }
    if let Some(agent) = &facts.agent {
        rows.push(Fact {
            kind: 2,
            ..fact("Agent", agent.clone())
        });
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

/// A side question's row: the question, then its answer or where that stands.
fn aside_row(t: &harlo_core::theme::Tokens, question: &str, answer: &Answer) -> AsideRow {
    let mut row = AsideRow {
        question: question.into(),
        ..AsideRow::default()
    };
    match answer {
        Answer::Waiting => row.waiting = true,
        Answer::Given(text) => {
            // Markup the window can't draw reads as its plain words.
            row.answer = slint::StyledText::from_markdown(&markdown::painted(t, text))
                .unwrap_or_else(|_| slint::StyledText::from_plain_text(&markdown::plain(text)));
        }
        Answer::Failed(why) => row.failed = why.as_str().into(),
    }
    row
}
