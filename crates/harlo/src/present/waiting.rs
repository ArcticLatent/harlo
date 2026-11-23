//! Cards above the composer for what the open session waits on: a staged
//! undo to keep or cancel, an approval to allow or refuse, and questions to
//! answer.
//!
//! The oldest of each shows; a count says how many more are waiting.

// Rust guideline compliant 2026-10-09

use std::rc::Rc;

use harlo_core::approvals::{self, Draft};
use harlo_core::diff;
use harlo_core::server::{Field, FieldKind};
use harlo_core::workspace::{FileDiff, Form, Permission};
use slint::{Model as _, ModelRc, SharedString, VecModel};

use super::{sync, timeline::diff_line};
use crate::app::{App, StagedUndo};
use crate::ui::{Answer, ApprovalCard, CardFile, Choice, Question, QuestionCard, State, UndoCard};

/// The most patch lines a file shows on an approval card.
const PATCH_LINES: usize = 12;

/// The most lines of a command or path list an approval card shows.
const DETAIL_LINES: usize = 6;

/// The most lines of each restored file an undo card shows.
const UNDO_LINES: usize = 8;

/// Shows the cards for what the open session waits on; the questions go
/// into `questions`, row by row, so a field being typed in keeps its focus.
pub fn show(app: &App, state: &State<'_>, questions: &Rc<VecModel<Question>>) {
    let undo = app.staged_undo().map(|staged| undo_card(app, staged));
    let approvals = app.waiting_approvals();
    let approval = approvals
        .first()
        .map(|asked| approval_card(app, asked, approvals.len()));
    let forms = app.waiting_questions();
    let (card, asked) = forms
        .first()
        .map(|form| question_card(app, form, forms.len()))
        .unwrap_or_default();
    // Lists compare by identity, so a card is set again only when what it
    // holds changed; its file lists then keep their rows.
    let undo = undo.unwrap_or_default();
    let old = state.get_undo_card();
    if (UndoCard {
        files: old.files.clone(),
        ..undo.clone()
    }) != old
        || !files_eq(&undo.files, &old.files)
    {
        state.set_undo_card(undo);
    }
    let approval = approval.unwrap_or_default();
    let old = state.get_approval_card();
    if (ApprovalCard {
        files: old.files.clone(),
        ..approval.clone()
    }) != old
        || !files_eq(&approval.files, &old.files)
    {
        state.set_approval_card(approval);
    }
    if card != state.get_question_card() {
        state.set_question_card(card);
    }
    // A question whose options didn't change keeps their list.
    let asked = asked
        .into_iter()
        .enumerate()
        .map(|(index, mut question)| {
            if let Some(old) = questions.row_data(index)
                && rows_eq(&old.choices, &question.choices)
            {
                question.choices = old.choices;
            }
            question
        })
        .collect();
    sync(questions, asked);
}

/// Whether two lists hold the same rows.
fn rows_eq<T: Clone + PartialEq + 'static>(a: &ModelRc<T>, b: &ModelRc<T>) -> bool {
    a.row_count() == b.row_count() && a.iter().eq(b.iter())
}

/// Whether two lists of files hold the same files and lines.
fn files_eq(a: &ModelRc<CardFile>, b: &ModelRc<CardFile>) -> bool {
    a.row_count() == b.row_count()
        && a.iter().zip(b.iter()).all(|(x, y)| {
            rows_eq(&x.lines, &y.lines)
                && CardFile {
                    lines: y.lines.clone(),
                    ..x
                } == y
        })
}

/// The card for a staged undo: what it put back, whose edits that
/// overwrote, and Undo or Cancel undo.
fn undo_card(app: &App, staged: StagedUndo<'_>) -> UndoCard {
    let StagedUndo {
        revert,
        prompt,
        agent,
    } = staged;
    let session = app.active.clone().unwrap_or_default();
    let restores = harlo_core::undo::restores(revert, agent);
    // The prompt in full, on a line of its own under the heading.
    let quote = prompt.map_or_else(String::new, |p| {
        format!("“{}”", p.split_whitespace().collect::<Vec<_>>().join(" "))
    });
    // OpenCode puts the files back as soon as an undo is staged, so the card
    // says that first; only the conversation waits for the answer.
    let (title, note) = if restores.is_empty() {
        (
            "Undo this prompt?",
            "No files change. Undo permanently removes this prompt and all later messages.",
        )
    } else {
        (
            "Files restored to before this prompt",
            "Undo permanently removes this prompt and all later messages. Cancel undo to \
             restore the edits.",
        )
    };
    let files: Vec<CardFile> = restores
        .iter()
        .map(|restore| {
            let mut card = card_file(&restore.file, UNDO_LINES);
            if let Some(lost) = restore.lost.filter(|n| *n > 0) {
                let lines = if lost == 1 { "line" } else { "lines" };
                card.warning = format!(
                    "This overwrites your own edits to {} ({lost} {lines}).",
                    restore.file.file
                )
                .into();
            }
            card
        })
        .collect();
    UndoCard {
        shown: true,
        busy: app.undos.busy.contains(&session),
        session: session.into(),
        title: title.into(),
        quote: quote.into(),
        note: note.into(),
        files: model(files),
        unchecked: restores.iter().any(|r| r.lost.is_none()),
    }
}

/// The card asking to allow or refuse an action.
fn approval_card(app: &App, asked: &Permission, count: usize) -> ApprovalCard {
    let said = approvals::describe(asked);
    let agent = app.agent_name();
    let refusing = app.refusing.as_ref().filter(|(id, _)| *id == asked.id);
    let files: Vec<CardFile> = asked
        .files
        .iter()
        .map(|file| card_file(file, PATCH_LINES))
        .collect();
    ApprovalCard {
        shown: true,
        id: asked.id.as_str().into(),
        title: format!("Allow {agent} to {}?", said.wants).into(),
        count: number(count),
        detail: if asked.files.is_empty() {
            clipped_detail(&said.detail).into()
        } else {
            SharedString::new()
        },
        files: model(files),
        message: asked.message.clone().unwrap_or_default().into(),
        always: "Always allow".into(),
        command: asked.action == "shell" && asked.files.is_empty(),
        // The scope in words, on a line of its own so it's never cut short.
        always_note: format!(
            "Always allow applies to {} in every session of this project.\n\
             Manage saved approvals in Settings → Agents.",
            approvals::always_scope(asked)
        )
        .into(),
        busy: app.answering.contains(&asked.id),
        refusing: refusing.is_some(),
        reason: refusing
            .map(|(_, reason)| reason.as_str())
            .unwrap_or_default()
            .into(),
        reason_hint: format!("Tell {agent} what to do instead").into(),
    }
}

/// A command or list of paths, cut to its first lines.
fn clipped_detail(detail: &str) -> String {
    let lines: Vec<&str> = detail.lines().collect();
    let mut shown = lines
        .iter()
        .take(DETAIL_LINES)
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    if lines.len() > DETAIL_LINES {
        shown = format!("{shown}\n… {} more", lines.len() - DETAIL_LINES);
    }
    shown
}

/// The card with the agent's questions, and the questions it asks now.
fn question_card(app: &App, form: &Form, count: usize) -> (QuestionCard, Vec<Question>) {
    let draft = app.draft(form);
    let problems = app.problems.get(&form.id).map_or(&[][..], Vec::as_slice);
    let mut title = format!("{} has a question", app.agent_name());
    if !form.title.is_empty() && form.title != "Questions" {
        title = format!("{title}: {}", form.title);
    }
    let questions = form
        .fields
        .iter()
        .filter(|f| draft.asks(form, f))
        .map(|field| {
            let mut question = question(&form.id, field, &draft);
            question.problems = problems
                .iter()
                .filter(|p| p.key == field.key)
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>()
                .join("\n")
                .into();
            question
        })
        .collect();
    let card = QuestionCard {
        shown: true,
        id: form.id.as_str().into(),
        title: title.into(),
        count: number(count),
        busy: app.answering.contains(&form.id),
    };
    (card, questions)
}

/// One question: its options to pick, a text box, or a link.
fn question(form: &str, field: &Field, draft: &Draft) -> Question {
    let choices = |options: &[harlo_core::server::Choice]| -> Vec<Choice> {
        options
            .iter()
            .map(|option| Choice {
                label: option.label.as_str().into(),
                description: option.description.clone().unwrap_or_default().into(),
                value: option.value.as_str().into(),
                chosen: draft.is_chosen(&field.key, &option.value),
            })
            .collect()
    };
    let mut question = Question {
        form: form.into(),
        key: field.key.as_str().into(),
        title: field.title.clone().unwrap_or_default().into(),
        description: field.description.clone().unwrap_or_default().into(),
        typed: draft.typed(&field.key).into(),
        flag: draft.flag(&field.key).map_or(-1, i32::from),
        ..Question::default()
    };
    match &field.kind {
        FieldKind::Text {
            options,
            custom,
            placeholder,
            ..
        } => {
            question.answer = Answer::One;
            question.choices = model(choices(options));
            question.input = options.is_empty() || *custom;
            let fallback = if options.is_empty() {
                "Your answer"
            } else {
                "Something else…"
            };
            question.hint = placeholder.as_deref().unwrap_or(fallback).into();
        }
        FieldKind::Multiselect {
            options, custom, ..
        } => {
            question.answer = Answer::Many;
            question.choices = model(choices(options));
            question.input = *custom;
            question.hint = "Something else…".into();
        }
        FieldKind::Number { .. } => {
            question.answer = Answer::Number;
            question.input = true;
            question.hint = "A number".into();
        }
        FieldKind::Boolean { .. } => question.answer = Answer::YesNo,
        FieldKind::External { url } => {
            question.answer = Answer::Link;
            question.url = url.as_str().into();
        }
    }
    question
}

/// A file's name, counts and first `limit` patch lines.
fn card_file(file: &FileDiff, limit: usize) -> CardFile {
    let lines = diff::lines(&file.patch);
    let hidden = lines.len().saturating_sub(limit);
    CardFile {
        name: file.file.as_str().into(),
        added: count(file.additions),
        removed: count(file.deletions),
        lines: model(lines.iter().take(limit).map(diff_line).collect()),
        more: if hidden > 0 {
            format!("… {hidden} more lines").into()
        } else {
            SharedString::new()
        },
        warning: SharedString::new(),
    }
}

/// A list for the UI.
fn model<T: Clone + 'static>(rows: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(rows))
}

/// A count as the UI's `int`.
fn number(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// A line count as the UI's `int`.
fn count(n: u64) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}
