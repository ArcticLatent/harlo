//! The Changes panel's body: the open session's changes, then the folder's
//! uncommitted changes with staging, a commit message, commit and push.

// Rust guideline compliant 2026-10-09

use harlo_core::git::FileState;
use harlo_core::theme::Tokens;
use iced::widget::{Column, button, column, container, row, scrollable, space, text, text_input};
use iced::{Center, Element, Fill};
use lucide_icons::Icon;

use crate::app::{App, ChangedFile, Changes, ChangesEvent, GitAction, List, Message};
use crate::ids;
use crate::style::{self, MONO, SANS_MEDIUM, size};
use crate::view::{self, icon};

/// The most diff lines a file shows before "N more lines".
const DIFF_LINES: usize = 40;

/// The gap between the panel's sections.
const SECTION_GAP: f32 = 22.0;

/// The width of a file row's checkbox column.
const CHECK_WIDTH: f32 = 26.0;

/// The space between the list and its scrollbar.
const SCROLLBAR_GAP: f32 = 4.0;

/// The panel's body below its header.
pub fn body(app: &App) -> Element<'_, Message> {
    let t = &app.tokens;
    let changes = &app.changes;
    if changes.is_loading() {
        return note(t, "Loading changes…");
    }
    let mut sections = Column::new().spacing(SECTION_GAP).padding([16, 18]);
    if app.active.is_some() {
        sections = sections.push(session(t, changes));
    }
    sections = sections.push(tree(t, changes));
    scrollable(sections)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().spacing(SCROLLBAR_GAP),
        ))
        .style(style::thin_scroll(t))
        .height(Fill)
        .into()
}

/// Plain text in the panel, for an empty or loading state.
fn note<'a>(t: &Tokens, words: &str) -> Element<'a, Message> {
    container(
        text(words.to_owned())
            .size(size::LABEL)
            .color(style::color(t.text_base)),
    )
    .padding([20, 18])
    .into()
}

/// A section's title, with what it holds on the right.
fn title<'a>(t: &Tokens, name: &str, aside: Element<'a, Message>) -> Element<'a, Message> {
    row![
        text(name.to_owned())
            .font(SANS_MEDIUM)
            .size(size::LABEL)
            .color(style::color(t.text_strong)),
        space().width(Fill),
        aside,
    ]
    .spacing(8)
    .align_y(Center)
    .into()
}

/// Weak text, for counts beside titles and short explanations.
fn weak<'a>(t: &Tokens, words: String) -> Element<'a, Message> {
    text(words)
        .size(size::SMALL)
        .color(style::color(t.text_weak))
        .into()
}

/// "1 file" or "3 files".
fn files_count(n: usize) -> String {
    if n == 1 {
        "1 file".to_owned()
    } else {
        format!("{n} files")
    }
}

/// What the open session changed.
fn session<'a>(t: &Tokens, changes: &'a Changes) -> Element<'a, Message> {
    let mut list = Column::new().spacing(2);
    let aside = match &changes.session {
        Some(Ok(files)) if files.is_empty() => {
            list = list.push(weak(t, "The agent hasn't changed any files.".to_owned()));
            space().into()
        }
        Some(Ok(files)) => {
            for file in files {
                list = list.push(file_view(t, changes, List::Session, file, None));
            }
            weak(t, files_count(files.len()))
        }
        Some(Err(reason)) => {
            list = list.push(failure(t, format!("Couldn't load them: {reason}")));
            space().into()
        }
        None => space().into(),
    };
    column![title(t, "This session", aside), list]
        .spacing(8)
        .into()
}

/// The folder's uncommitted changes, with staging, commit and push.
fn tree<'a>(t: &Tokens, changes: &'a Changes) -> Element<'a, Message> {
    let status = changes.status.as_ref();
    let mut aside = row![].spacing(6).align_y(Center);
    if let Some(branch) = status.and_then(|s| s.branch.as_deref()) {
        aside = aside
            .push(icon(Icon::GitBranch, 12.0, t.icon_weak_base))
            .push(weak(t, branch.to_owned()));
    }
    let mut list = Column::new().spacing(2);
    let files = match &changes.tree {
        Some(Ok(files)) => files.as_slice(),
        Some(Err(reason)) => {
            list = list.push(failure(t, format!("Couldn't load them: {reason}")));
            &[]
        }
        None => &[],
    };
    if files.is_empty() && changes.tree.as_ref().is_some_and(Result::is_ok) {
        list = list.push(weak(t, "Nothing to commit.".to_owned()));
    }
    let busy = changes.running.is_some();
    for file in files {
        let check = status.map(|_| checkbox(t, file.git.as_ref(), busy));
        list = list.push(file_view(t, changes, List::Tree, file, check));
    }
    let mut section = column![title(t, "Uncommitted", aside.into())].spacing(8);
    if status.is_some() && !files.is_empty() {
        section = section.push(stage_all(t, files, busy));
    }
    section = section.push(list);
    match status {
        Some(status) => {
            if !files.is_empty() {
                section = section.push(commit_box(t, changes, status.has_staged()));
            }
            if let Some(push) = push_row(t, changes) {
                section = section.push(push);
            }
        }
        None if !files.is_empty() => {
            section = section.push(weak(
                t,
                "Staging and committing need the folder on this computer, in a Git repository."
                    .to_owned(),
            ));
        }
        None => {}
    }
    if let Some(outcome) = &changes.outcome {
        section = section.push(match outcome {
            Ok(said) => weak(t, said.clone()),
            Err(reason) => failure(t, reason.clone()),
        });
    }
    section.into()
}

/// Text in the critical colour, for something that failed.
fn failure<'a>(t: &Tokens, words: String) -> Element<'a, Message> {
    text(words)
        .size(size::SMALL)
        .color(style::color(t.icon_critical_base))
        .into()
}

/// A file's row, and its diff when open.
///
/// `check` is the staging checkbox, for an uncommitted file in a repository here.
fn file_view<'a>(
    t: &Tokens,
    changes: &Changes,
    list: List,
    file: &'a ChangedFile,
    check: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let key = Changes::key(list, &file.path);
    let open = changes.open.contains(&key);
    let (folder, name) = match file.path.rsplit_once('/') {
        Some((folder, name)) => (Some(folder), name),
        None => (None, file.path.as_str()),
    };
    let chevron = if open {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };
    let mut label = row![
        icon(chevron, 13.0, t.icon_weak_base),
        text(name.to_owned())
            .font(MONO)
            .size(size::CODE)
            .color(style::color(t.text_strong))
            .wrapping(text::Wrapping::None),
    ]
    .spacing(6)
    .align_y(Center);
    if let Some(folder) = folder {
        label = label.push(
            text(folder.to_owned())
                .size(size::SMALL)
                .color(style::color(t.text_weak))
                .wrapping(text::Wrapping::None)
                .width(Fill),
        );
    } else {
        label = label.push(space().width(Fill));
    }
    let deleted = file.git.is_none() && file.lines.is_empty() && file.additions == 0;
    label = label.push(if deleted {
        space().into()
    } else {
        view::diff::counts(t, file.additions, file.deletions)
    });
    let mut face = button(label.clip(true))
        .width(Fill)
        .padding([5, 6])
        .style(style::ghost(t, false));
    if !file.lines.is_empty() {
        face = face.on_press(Message::Changes(ChangesEvent::Toggle(key.clone())));
    }
    let head: Element<'a, Message> = match check {
        Some(check) => row![check, face].align_y(Center).into(),
        None => face.into(),
    };
    if !open || file.lines.is_empty() {
        return head;
    }
    let limit = if changes.whole.contains(&key) {
        usize::MAX
    } else {
        DIFF_LINES
    };
    let more = Message::Changes(ChangesEvent::Whole(key));
    column![
        head,
        container(view::diff::lines(t, &file.lines, limit, Some(more))).padding(
            iced::padding::left(if list == List::Tree && file.git.is_some() {
                CHECK_WIDTH
            } else {
                0.0
            })
            .bottom(6)
        ),
    ]
    .spacing(2)
    .into()
}

/// A file's staging checkbox: ticked when staged, a dash when partly staged.
fn checkbox<'a>(t: &Tokens, state: Option<&FileState>, busy: bool) -> Element<'a, Message> {
    let Some(state) = state else {
        return space().width(CHECK_WIDTH).into();
    };
    let (glyph, colour, event) = match (state.staged, state.unstaged) {
        (true, false) => (
            Icon::SquareCheck,
            t.text_interactive_base,
            ChangesEvent::Unstage(vec![state.path.clone()]),
        ),
        (true, true) => (
            Icon::SquareMinus,
            t.text_interactive_base,
            ChangesEvent::Stage(vec![state.path.clone()]),
        ),
        _ => (
            Icon::Square,
            t.icon_weak_base,
            ChangesEvent::Stage(vec![state.path.clone()]),
        ),
    };
    let mut check = button(container(icon(glyph, 15.0, colour)).center(CHECK_WIDTH - 4.0))
        .padding(0)
        .width(CHECK_WIDTH - 4.0)
        .style(style::ghost(t, false));
    if !busy {
        check = check.on_press(Message::Changes(event));
    }
    container(check).width(CHECK_WIDTH).into()
}

/// "Stage all" or "Unstage all", whichever applies.
fn stage_all<'a>(t: &Tokens, files: &[ChangedFile], busy: bool) -> Element<'a, Message> {
    let states: Vec<&FileState> = files.iter().filter_map(|f| f.git.as_ref()).collect();
    let all_staged = states.iter().all(|s| s.staged && !s.unstaged);
    let paths: Vec<String> = states.iter().map(|s| s.path.clone()).collect();
    let (words, event) = if all_staged {
        ("Unstage all", ChangesEvent::Unstage(paths))
    } else {
        ("Stage all", ChangesEvent::Stage(paths))
    };
    let mut action = button(
        text(words)
            .size(size::SMALL)
            .color(style::color(t.text_interactive_base)),
    )
    .padding([3, 6])
    .style(style::ghost(t, false));
    if !busy {
        action = action.on_press(Message::Changes(event));
    }
    row![space().width(Fill), action].into()
}

/// The commit message and the Commit button.
fn commit_box<'a>(t: &Tokens, changes: &'a Changes, staged: bool) -> Element<'a, Message> {
    let ready = staged && !changes.message.trim().is_empty() && changes.running.is_none();
    let mut field = text_input("Commit message", &changes.message)
        .id(ids::COMMIT_MESSAGE)
        .on_input(|text| Message::Changes(ChangesEvent::Message(text)))
        .size(size::LABEL)
        .padding([7, 10])
        .style(style::menu_filter(t));
    if ready {
        field = field.on_submit(Message::Changes(ChangesEvent::Commit));
    }
    let words = if changes.running == Some(GitAction::Commit) {
        "Committing…"
    } else {
        "Commit"
    };
    let mut commit = button(text(words).size(size::LABEL))
        .padding([6, 12])
        .style(style::primary(t));
    if ready {
        commit = commit.on_press(Message::Changes(ChangesEvent::Commit));
    }
    let mut column = column![row![field, commit].spacing(8).align_y(Center)].spacing(6);
    if !staged {
        column = column.push(weak(
            t,
            "Tick files to stage them for the commit.".to_owned(),
        ));
    }
    column.into()
}

/// The Push button with how many commits it sends, when there are any.
fn push_row<'a>(t: &Tokens, changes: &Changes) -> Option<Element<'a, Message>> {
    let status = changes.status.as_ref()?;
    let upstream = status.upstream.as_deref()?;
    if status.ahead == 0 {
        return None;
    }
    let commits = if status.ahead == 1 {
        "1 commit".to_owned()
    } else {
        format!("{} commits", status.ahead)
    };
    let words = if changes.running == Some(GitAction::Push) {
        "Pushing…".to_owned()
    } else {
        format!("Push {commits}")
    };
    let mut push = button(
        row![
            icon(Icon::ArrowUp, 13.0, t.icon_base),
            text(words).size(size::LABEL)
        ]
        .spacing(6)
        .align_y(Center),
    )
    .padding([6, 10])
    .style(style::chip(t));
    if changes.running.is_none() {
        push = push.on_press(Message::Changes(ChangesEvent::Push));
    }
    let mut line = row![push, weak(t, format!("to {upstream}"))]
        .spacing(8)
        .align_y(Center);
    if status.behind > 0 {
        line = line.push(weak(t, format!("({} behind)", status.behind)));
    }
    Some(line.into())
}
