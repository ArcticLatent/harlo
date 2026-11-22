//! The Changes panel's body: the reviewed turn's changes, the open session's,
//! then the folder's uncommitted changes with a commit message, commit and
//! push.
//!
//! In a dock pane each file's diff opens under its row. Maximized, the
//! lists sit on the left and the selected file's diff fills the right, with
//! the lit hunk outlined.

// Rust guideline compliant 2026-10-09

use harlo_core::diff::{Line, LineKind};
use harlo_core::theme::Tokens;
use iced::widget::{
    Column, button, column, container, mouse_area, row, rule, scrollable, space, text, text_input,
};
use iced::{Center, Element, Fill};
use lucide_icons::Icon;

use crate::app::{App, ChangedFile, Changes, ChangesEvent, GitAction, List, Message};
use crate::ids;
use crate::style::{self, MONO, SANS_MEDIUM, size};
use crate::view::{self, icon};
use crate::widget::ellipsis;

/// The most diff lines a file shows before "N more lines".
const DIFF_LINES: usize = 40;

/// The gap between the panel's sections.
const SECTION_GAP: f32 = 22.0;

/// The space between the list and its scrollbar.
const SCROLLBAR_GAP: f32 = 4.0;

/// The width of the maximized panel's lists.
const LIST_WIDTH: f32 = 380.0;

/// The space around the lists.
const LIST_PADDING: f32 = 16.0;

/// A section title's height, with the gap under it.
const TITLE_ROW: f32 = 28.0;

/// A file row's height, with the gap under it.
const FILE_ROW: f32 = 31.0;

/// A diff line's height.
const DIFF_LINE: f32 = size::CODE * 1.3 + 2.0;

/// The space between two hunks of the maximized diff.
const HUNK_GAP: f32 = 12.0;

/// How far above a hunk the view stops when it scrolls to it.
const HUNK_MARGIN: f32 = 24.0;

/// How a file row acts: opening its diff under it, or selecting it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// In a dock pane: the row opens its diff under it.
    Inline,
    /// Maximized: the row selects the file, whose diff shows beside the lists.
    Select,
}

/// The panel's body in a dock pane.
pub fn body(app: &App) -> Element<'_, Message> {
    lists(app, Mode::Inline)
}

/// The panel maximized: the lists on the left, the selected file's diff on the right.
pub fn maximized(app: &App) -> Element<'_, Message> {
    let t = &app.tokens;
    row![
        container(lists(app, Mode::Select)).width(LIST_WIDTH),
        rule::vertical(1).style(style::divider(t.border_weaker_base)),
        selected_diff(app),
    ]
    .height(Fill)
    .into()
}

/// The turn under review, the session's changes and the uncommitted ones.
fn lists(app: &App, mode: Mode) -> Element<'_, Message> {
    let t = &app.tokens;
    let changes = &app.changes;
    if changes.is_loading() {
        return note(t, "Loading changes…");
    }
    let mut sections = Column::new()
        .spacing(SECTION_GAP)
        .padding([LIST_PADDING, 18.0]);
    if let Some(turn) = turn(t, changes, mode) {
        sections = sections.push(turn);
    }
    if app.active.is_some() {
        sections = sections.push(session(t, changes, mode));
    }
    sections = sections.push(tree(t, changes, mode));
    scrollable(sections)
        .id(ids::CHANGES_LIST)
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

/// The files of a list that may still be loading or have failed.
fn listed<'a>(
    t: &Tokens,
    changes: &'a Changes,
    list: List,
    files: Option<&'a Result<Vec<ChangedFile>, String>>,
    empty: &str,
    mode: Mode,
) -> (Column<'a, Message>, Element<'a, Message>) {
    let mut column = Column::new().spacing(2);
    let aside = match files {
        Some(Ok(files)) if files.is_empty() => {
            column = column.push(weak(t, empty.to_owned()));
            space().into()
        }
        Some(Ok(files)) => {
            for file in files {
                column = column.push(file_view(t, changes, list, file, mode));
            }
            weak(t, files_count(files.len()))
        }
        Some(Err(reason)) => {
            column = column.push(failure(t, format!("Couldn't load them: {reason}")));
            space().into()
        }
        None => space().into(),
    };
    (column, aside)
}

/// The turn under review: what it changed, named by what was asked.
fn turn<'a>(t: &Tokens, changes: &'a Changes, mode: Mode) -> Option<Element<'a, Message>> {
    let review = changes.review.as_ref()?;
    let (list, _) = listed(
        t,
        changes,
        List::Turn,
        changes.turn.as_ref(),
        "This turn didn't change any files.",
        mode,
    );
    let close = button(icon(Icon::X, 12.0, t.icon_weak_base))
        .padding(3)
        .on_press(Message::Changes(ChangesEvent::EndReview))
        .style(style::round(t));
    let asked = ellipsis(review.asked.lines().next().unwrap_or_default().to_owned())
        .size(size::SMALL)
        .color(style::color(t.text_weak));
    Some(
        column![title(t, "This turn", close.into()), asked, list,]
            .spacing(6)
            .into(),
    )
}

/// What the open session changed.
fn session<'a>(t: &Tokens, changes: &'a Changes, mode: Mode) -> Element<'a, Message> {
    let (list, aside) = listed(
        t,
        changes,
        List::Session,
        changes.session.as_ref(),
        "The agent hasn't changed any files.",
        mode,
    );
    column![title(t, "This session", aside), list]
        .spacing(8)
        .into()
}

/// The folder's uncommitted changes, with commit and push.
fn tree<'a>(t: &Tokens, changes: &'a Changes, mode: Mode) -> Element<'a, Message> {
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
    for file in files {
        list = list.push(file_view(t, changes, List::Tree, file, mode));
    }
    let mut section = column![title(t, "Uncommitted", aside.into()), list].spacing(8);
    match status {
        Some(_) => {
            if !files.is_empty() {
                section = section.push(commit_box(t, changes));
            }
            if let Some(push) = push_row(t, changes) {
                section = section.push(push);
            }
        }
        None if !files.is_empty() => {
            section = section.push(weak(
                t,
                "Committing needs the folder on this computer, in a Git repository.".to_owned(),
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

/// A file's row, and in a dock pane its diff when open.
///
/// The name comes first and stays whole; the folder shortens to make room,
/// and the counts keep their place at the end.
fn file_view<'a>(
    t: &Tokens,
    changes: &Changes,
    list: List,
    file: &'a ChangedFile,
    mode: Mode,
) -> Element<'a, Message> {
    let key = Changes::key(list, &file.path);
    let selected = changes.selected.as_deref() == Some(key.as_str());
    let open = mode == Mode::Inline && changes.open.contains(&key);
    let (folder, name) = match file.path.rsplit_once('/') {
        Some((folder, name)) => (Some(folder), name),
        None => (None, file.path.as_str()),
    };
    let mut label = row![].spacing(6).align_y(Center);
    if mode == Mode::Inline {
        let chevron = if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        label = label.push(icon(chevron, 13.0, t.icon_weak_base));
    }
    label = label.push(view::file_icon(name, 13.0, false)).push(
        text(name.to_owned())
            .font(MONO)
            .size(size::CODE)
            .color(style::color(t.text_strong))
            .wrapping(text::Wrapping::None),
    );
    label = label.push(match folder {
        Some(folder) => Element::from(
            container(
                ellipsis(folder.to_owned())
                    .size(size::SMALL)
                    .color(style::color(t.text_weak)),
            )
            .width(Fill),
        ),
        None => space().width(Fill).into(),
    });
    let deleted = file.git.is_none() && file.lines.is_empty() && file.additions == 0;
    if !deleted {
        label = label.push(view::diff::counts(t, file.additions, file.deletions));
    }
    let mut face = button(label.clip(true))
        .width(Fill)
        .padding([5, 6])
        .style(style::ghost(t, selected && mode == Mode::Select));
    if !file.lines.is_empty() {
        face = face.on_press(Message::Changes(match mode {
            Mode::Inline => ChangesEvent::Toggle(key.clone()),
            Mode::Select => ChangesEvent::Select(key.clone()),
        }));
    }
    let head: Element<'a, Message> = face.into();
    if !open || file.lines.is_empty() {
        return head;
    }
    let limit = if changes.whole.contains(&key) {
        usize::MAX
    } else {
        DIFF_LINES
    };
    let expand = Message::Changes(ChangesEvent::Whole(key));
    column![
        head,
        container(view::diff::lines(t, &file.lines, limit, Some(expand)))
            .padding(iced::padding::bottom(6)),
    ]
    .spacing(2)
    .into()
}

/// The selected file's whole diff, hunk by hunk, under its path; the lit
/// hunk is outlined, and clicking a hunk lights it.
fn selected_diff(app: &App) -> Element<'_, Message> {
    let t = &app.tokens;
    let Some(file) = app.changes.selected_file() else {
        return container(weak(t, "Select a file to see its changes.".to_owned()))
            .center(Fill)
            .into();
    };
    let mut path = row![icon(Icon::Folder, 14.0, t.icon_weak_base)]
        .spacing(8)
        .align_y(Center);
    let parts: Vec<&str> = file.path.split('/').collect();
    for (index, part) in parts.iter().enumerate() {
        let last = index + 1 == parts.len();
        if index > 0 {
            path = path.push(weak(t, "/".to_owned()));
        }
        path = path.push(
            text((*part).to_owned())
                .size(size::LABEL)
                .color(style::color(if last { t.text_strong } else { t.text_weak })),
        );
    }
    let mut hunks = Column::new().spacing(HUNK_GAP).padding(LIST_PADDING);
    for (index, lines) in split_hunks(&file.lines).into_iter().enumerate() {
        let lit = app.changes.hunk == Some(index);
        let outline = lit.then_some(t.text_interactive_base);
        hunks = hunks.push(
            mouse_area(
                container(view::diff::lines(t, lines, usize::MAX, None::<Message>))
                    .padding(1)
                    .style(style::boxed(t.background_base, outline, size::RADIUS)),
            )
            .on_press(Message::Changes(ChangesEvent::Hunk(index))),
        );
    }
    column![
        container(path).padding([12.0, LIST_PADDING + 2.0]),
        rule::horizontal(1).style(style::divider(t.border_weaker_base)),
        scrollable(hunks)
            .id(ids::CHANGES_DIFF)
            .style(style::thin_scroll(t))
            .height(Fill),
    ]
    .width(Fill)
    .height(Fill)
    .into()
}

/// A diff's hunks: its lines between the gaps.
fn split_hunks(lines: &[Line]) -> Vec<&[Line]> {
    lines
        .split(|line| line.kind == LineKind::Gap)
        .filter(|hunk| !hunk.is_empty())
        .collect()
}

/// A count of lines, as a height in pixels.
#[expect(
    clippy::cast_precision_loss,
    reason = "a diff has far fewer lines than an f32 holds"
)]
fn lines_height(lines: usize) -> f32 {
    lines as f32 * DIFF_LINE
}

/// How far down the maximized diff the lit hunk starts.
#[must_use]
pub fn hunk_top(changes: &Changes) -> f32 {
    let (Some(file), Some(hunk)) = (changes.selected_file(), changes.hunk) else {
        return 0.0;
    };
    let above: f32 = split_hunks(&file.lines)
        .iter()
        .take(hunk)
        .map(|lines| lines_height(lines.len()) + 12.0 + HUNK_GAP)
        .sum();
    (LIST_PADDING + above - HUNK_MARGIN).max(0.0)
}

/// How far down the panel's list the selected file's lit hunk starts (or
/// the file's row, without one), roughly.
#[must_use]
pub fn selected_top(changes: &Changes) -> f32 {
    let Some(selected) = changes.selected.as_deref() else {
        return 0.0;
    };
    let mut y = LIST_PADDING;
    let mut list = None;
    for (key, file) in changes.files() {
        let this = key.split(':').next().map(str::to_owned);
        if this != list {
            // A new section: its title, and the turn's question under it.
            y += TITLE_ROW
                + if this.as_deref() == Some("turn") {
                    20.0
                } else {
                    0.0
                };
            list = this;
        }
        if key == selected {
            let hunk = changes.hunk.unwrap_or(0);
            let before = file
                .lines
                .iter()
                .take_while({
                    let mut gaps = 0;
                    move |line| {
                        if line.kind == LineKind::Gap {
                            gaps += 1;
                        }
                        gaps < hunk
                    }
                })
                .count();
            let into_diff = if hunk > 0 {
                FILE_ROW + lines_height(before)
            } else {
                0.0
            };
            return (y + into_diff - HUNK_MARGIN).max(0.0);
        }
        y += FILE_ROW;
        if changes.open.contains(&key) {
            let shown = if changes.whole.contains(&key) {
                file.lines.len()
            } else {
                file.lines.len().min(DIFF_LINES)
            };
            y += lines_height(shown) + 16.0;
        }
    }
    0.0
}

/// The commit message and the Commit button, which commits every
/// uncommitted file.
fn commit_box<'a>(t: &Tokens, changes: &'a Changes) -> Element<'a, Message> {
    let ready = !changes.message.trim().is_empty() && changes.running.is_none();
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
    row![field, commit].spacing(8).align_y(Center).into()
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
