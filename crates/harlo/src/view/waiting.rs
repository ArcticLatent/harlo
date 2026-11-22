//! Cards above the composer for what the open session waits on: a staged
//! undo to keep or cancel, an approval to allow or refuse, and questions to answer.
//!
//! The oldest of each shows; a count says how many more are waiting.

// Rust guideline compliant 2026-10-09

use harlo_core::approvals::{self, Draft, Problem};
use harlo_core::diff;
use harlo_core::server::{Field, FieldKind, PermissionReply};
use harlo_core::theme::Tokens;
use harlo_core::workspace::{Form, Permission};
use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Center, Element, Fill, Length};
use lucide_icons::Icon;

use crate::app::{App, Message, StagedUndo};
use crate::ids;
use crate::style::{self, MONO, SANS_MEDIUM, size};
use crate::view::{self, icon};

/// The most patch lines a file shows on an approval card.
const PATCH_LINES: usize = 12;

/// The most lines of a command or path list an approval card shows.
const DETAIL_LINES: usize = 6;

/// The most lines of each restored file an undo card shows.
const UNDO_LINES: usize = 8;

/// The most characters of the undone prompt an undo card's heading quotes.
const PROMPT_CHARS: usize = 48;

/// The space between the cards and their scrollbar.
const SCROLLBAR_GAP: f32 = 4.0;

/// The tallest the cards get before they scroll.
const MAX_HEIGHT: f32 = 420.0;

/// The space inside a card.
const PADDING: u16 = 14;

/// The cards for what the open session waits on, if anything.
pub fn view(app: &App) -> Option<Element<'_, Message>> {
    let approvals = app.waiting_approvals();
    let questions = app.waiting_questions();
    let staged = app.staged_undo();
    let mut cards = column![].spacing(8);
    if let Some(staged) = staged {
        cards = cards.push(undo(app, staged));
    }
    if let Some(asked) = approvals.first() {
        cards = cards.push(approval(app, asked, approvals.len()));
    }
    if let Some(form) = questions.first() {
        cards = cards.push(question(app, form, questions.len()));
    }
    if approvals.is_empty() && questions.is_empty() && staged.is_none() {
        return None;
    }
    Some(
        container(
            scrollable(cards)
                // A gutter of its own, so the bar never covers a card's edge.
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new().spacing(SCROLLBAR_GAP),
                ))
                .style(style::thin_scroll(&app.tokens)),
        )
        .max_height(MAX_HEIGHT)
        .into(),
    )
}

/// The heading of a card: an icon, what's asked, and how many wait.
fn heading<'a>(
    t: &Tokens,
    glyph: Icon,
    accent: harlo_core::theme::Rgba,
    says: String,
    count: usize,
) -> Element<'a, Message> {
    let mut line = row![
        icon(glyph, 15.0, accent),
        text(says)
            .font(SANS_MEDIUM)
            .size(size::TEXT)
            .color(style::color(t.text_strong)),
        space().width(Fill),
    ]
    .spacing(10)
    .align_y(Center);
    if count > 1 {
        line = line.push(
            text(format!("1 of {count}"))
                .size(size::SMALL)
                .color(style::color(t.text_weak)),
        );
    }
    line.into()
}

/// A card for a staged undo: what it put back, whose edits that overwrote,
/// and Undo or Cancel.
fn undo<'a>(app: &'a App, staged: StagedUndo<'_>) -> Element<'a, Message> {
    let StagedUndo {
        revert,
        prompt,
        agent,
    } = staged;
    let t = &app.tokens;
    let session = app.active.clone().unwrap_or_default();
    let busy = app.undos.busy.contains(&session);
    let says = match prompt {
        Some(prompt) => format!("Undo “{}” and everything after it?", clipped(prompt)),
        None => "Undo this prompt and everything after it?".to_owned(),
    };
    let mut body = column![heading(t, Icon::Undo2, t.icon_warning_base, says, 1)].spacing(10);
    let restores = harlo_core::undo::restores(revert, agent);
    let note = if restores.is_empty() {
        "No files change; the conversation goes back to before this prompt."
    } else {
        "These files are back as they were before it. Undo keeps that; Cancel puts them back."
    };
    body = body.push(
        text(note)
            .size(size::LABEL)
            .color(style::color(t.text_base)),
    );
    for restore in &restores {
        let file = &restore.file;
        let shown = diff::lines(&file.patch);
        let name = (file.file.as_str(), file.additions, file.deletions);
        body = body.push(view::diff::file(t, name, &shown, UNDO_LINES, None));
        if let Some(lost) = restore.lost.filter(|n| *n > 0) {
            let lines = if lost == 1 { "line" } else { "lines" };
            body = body.push(
                text(format!(
                    "This overwrites your own edits to {} ({lost} {lines}).",
                    file.file
                ))
                .size(size::SMALL)
                .color(style::color(t.icon_critical_base)),
            );
        }
    }
    if restores.iter().any(|r| r.lost.is_none()) {
        body = body.push(
            text("Couldn't check this undo, so your own edits can't be told from the agent's.")
                .size(size::SMALL)
                .color(style::color(t.text_weak)),
        );
    }
    let keep = (!busy).then(|| Message::UndoKeep(session.clone()));
    let cancel = (!busy).then(|| Message::UndoCancel(session.clone()));
    body = body.push(
        row![
            action(t, "Undo", keep, Some(ids::UNDO_KEEP), true),
            action(t, "Cancel", cancel, None, false),
        ]
        .spacing(8),
    );
    container(body)
        .id(ids::UNDO)
        .width(Fill)
        .padding(PADDING)
        .style(style::waiting_card(t, t.icon_warning_base))
        .into()
}

/// The first line of `text`, cut short to fit a heading.
fn clipped(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default();
    if line.chars().count() <= PROMPT_CHARS && !text.contains('\n') {
        return line.to_owned();
    }
    let cut: String = line.chars().take(PROMPT_CHARS).collect();
    format!("{}…", cut.trim_end())
}

/// A card asking to allow or refuse an action.
fn approval<'a>(app: &'a App, asked: &'a Permission, count: usize) -> Element<'a, Message> {
    let t = &app.tokens;
    let said = approvals::describe(asked);
    let agent = app.agent_name();
    let busy = app.answering.contains(&asked.id);
    let mut body = column![heading(
        t,
        Icon::ShieldAlert,
        t.icon_warning_base,
        format!("{agent} wants to {}", said.wants),
        count,
    )]
    .spacing(10);
    if asked.files.is_empty() {
        if !said.detail.is_empty() {
            body = body.push(detail(t, &said.detail));
        }
    } else {
        for file in &asked.files {
            let shown = diff::lines(&file.patch);
            let name = (file.file.as_str(), file.additions, file.deletions);
            body = body.push(view::diff::file(t, name, &shown, PATCH_LINES, None));
        }
    }
    if let Some(message) = asked.message.as_deref().filter(|m| !m.is_empty()) {
        body = body.push(
            text(message.to_owned())
                .size(size::SMALL)
                .color(style::color(t.text_base)),
        );
    }
    let refusing = app
        .refusing
        .as_ref()
        .filter(|(id, _)| *id == asked.id)
        .map(|(_, reason)| reason.as_str());
    body = body.push(match refusing {
        Some(reason) => refuse_row(t, &agent, reason, busy),
        None => approve_row(t, asked, busy),
    });
    container(body)
        .id(ids::APPROVAL)
        .width(Fill)
        .padding(PADDING)
        .style(style::waiting_card(t, t.icon_warning_base))
        .into()
}

/// Allow once, allow always, refuse, and refuse with a reason.
fn approve_row<'a>(t: &Tokens, asked: &'a Permission, busy: bool) -> Element<'a, Message> {
    let reply = |reply: PermissionReply| {
        (!busy).then(|| Message::Approve {
            request: asked.id.clone(),
            reply,
        })
    };
    let always = if asked.save.is_empty() || asked.save.iter().all(|p| p == "*") {
        "Always allow".to_owned()
    } else {
        format!("Always allow {}", asked.save.join(", "))
    };
    row![
        action(
            t,
            "Allow once",
            reply(PermissionReply::Once),
            Some(ids::ALLOW),
            true
        ),
        action(t, &always, reply(PermissionReply::Always), None, false),
        action(t, "Deny", reply(PermissionReply::Reject), None, false),
        button(
            text("Deny with a reason…")
                .size(size::SMALL)
                .color(style::color(t.text_weak)),
        )
        .padding([5, 8])
        .on_press_maybe((!busy).then(|| Message::Refuse(asked.id.clone())))
        .style(style::ghost(t, false)),
    ]
    .spacing(8)
    .align_y(Center)
    .wrap()
    .vertical_spacing(8)
    .into()
}

/// A reason to give the agent, then refuse.
fn refuse_row<'a>(t: &Tokens, agent: &str, reason: &str, busy: bool) -> Element<'a, Message> {
    let field = text_input(&format!("Tell {agent} what to do instead"), reason)
        .id(ids::REFUSE_REASON)
        .on_input(Message::RefuseReason)
        .on_submit(Message::RefuseSend)
        .size(size::LABEL)
        .padding([7, 10])
        .style(style::menu_filter(t));
    row![
        field,
        action(
            t,
            "Deny",
            (!busy).then_some(Message::RefuseSend),
            None,
            true
        ),
        action(t, "Cancel", Some(Message::RefuseCancel), None, false),
    ]
    .spacing(8)
    .align_y(Center)
    .into()
}

/// A command or list of paths, in a sunken monospace box.
fn detail<'a>(t: &Tokens, detail: &str) -> Element<'a, Message> {
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
    container(
        text(shown)
            .font(MONO)
            .size(size::CODE)
            .color(style::color(t.text_strong)),
    )
    .width(Fill)
    .padding([8, 10])
    .style(style::boxed(t.surface_inset_base, None, size::RADIUS))
    .into()
}

/// A card with the agent's questions and a way to answer or dismiss them.
fn question<'a>(app: &'a App, form: &'a Form, count: usize) -> Element<'a, Message> {
    let t = &app.tokens;
    let draft = app.draft(form);
    let problems: &[Problem] = app.problems.get(&form.id).map_or(&[], Vec::as_slice);
    let busy = app.answering.contains(&form.id);
    let mut says = format!("{} has a question", app.agent_name());
    if !form.title.is_empty() && form.title != "Questions" {
        says = format!("{says}: {}", form.title);
    }
    let mut body = column![heading(
        t,
        Icon::MessageCircleQuestionMark,
        t.text_interactive_base,
        says,
        count,
    )]
    .spacing(14);
    for field in form.fields.iter().filter(|f| draft.asks(form, f)) {
        let mut part = column![].spacing(6);
        if let Some(title) = &field.title {
            part = part.push(
                text(title.clone())
                    .font(SANS_MEDIUM)
                    .size(size::LABEL)
                    .color(style::color(t.text_strong)),
            );
        }
        if let Some(description) = &field.description {
            part = part.push(
                text(description.clone())
                    .size(size::LABEL)
                    .color(style::color(t.text_base)),
            );
        }
        part = part.push(answer(t, &form.id, field, &draft));
        for problem in problems.iter().filter(|p| p.key == field.key) {
            part = part.push(
                text(problem.text.clone())
                    .size(size::SMALL)
                    .color(style::color(t.icon_critical_base)),
            );
        }
        body = body.push(part);
    }
    let send = (!busy).then(|| Message::FormSend(form.id.clone()));
    let dismiss = (!busy).then(|| Message::FormDismiss(form.id.clone()));
    body = body.push(
        row![
            action(t, "Send answer", send, Some(ids::ANSWER), true),
            action(t, "Dismiss", dismiss, None, false),
        ]
        .spacing(8),
    );
    container(body)
        .id(ids::QUESTION)
        .width(Fill)
        .padding(PADDING)
        .style(style::waiting_card(t, t.text_interactive_base))
        .into()
}

/// The controls that answer one field: options to pick, a text box, or a link.
fn answer<'a>(t: &Tokens, form: &str, field: &Field, draft: &Draft) -> Element<'a, Message> {
    let (form, key) = (form.to_owned(), field.key.clone());
    let choose = |value: &str| Message::FormChoose {
        form: form.clone(),
        key: key.clone(),
        value: value.to_owned(),
    };
    let typing = {
        let (form, key) = (form.clone(), key.clone());
        move |text: String| Message::FormType {
            form: form.clone(),
            key: key.clone(),
            text,
        }
    };
    let typed = draft.typed(&field.key);
    let mut part = column![].spacing(6);
    match &field.kind {
        FieldKind::Text {
            options,
            custom,
            placeholder,
            ..
        } => {
            for option in options {
                let chosen = draft.is_chosen(&field.key, &option.value);
                let glyph = if chosen {
                    Icon::CircleDot
                } else {
                    Icon::Circle
                };
                let label = (option.label.as_str(), option.description.as_deref());
                part = part.push(choice(t, glyph, label, chosen, choose(&option.value)));
            }
            if options.is_empty() || *custom {
                let fallback = if options.is_empty() {
                    "Your answer"
                } else {
                    "Something else…"
                };
                let hint = placeholder.as_deref().unwrap_or(fallback);
                part = part.push(field_input(t, hint, typed, typing));
            }
        }
        FieldKind::Multiselect {
            options, custom, ..
        } => {
            for option in options {
                let chosen = draft.is_chosen(&field.key, &option.value);
                let glyph = if chosen {
                    Icon::SquareCheck
                } else {
                    Icon::Square
                };
                let label = (option.label.as_str(), option.description.as_deref());
                part = part.push(choice(t, glyph, label, chosen, choose(&option.value)));
            }
            if *custom {
                part = part.push(field_input(t, "Something else…", typed, typing));
            }
        }
        FieldKind::Number { .. } => {
            part = part.push(container(field_input(t, "A number", typed, typing)).width(160));
        }
        FieldKind::Boolean { .. } => {
            let flag = draft.flag(&field.key);
            let mut pair = row![].spacing(6);
            for (label, value) in [("Yes", true), ("No", false)] {
                let chosen = flag == Some(value);
                let glyph = if chosen {
                    Icon::CircleDot
                } else {
                    Icon::Circle
                };
                let set = Message::FormFlag {
                    form: form.clone(),
                    key: key.clone(),
                    flag: value,
                };
                pair = pair.push(choice(t, glyph, (label, None), chosen, set));
            }
            part = part.push(pair);
        }
        FieldKind::External { url } => {
            let open = Some(Message::OpenLink(url.clone()));
            part = part.push(
                row![
                    action(t, "Open link", open, None, false),
                    text(url.clone())
                        .font(MONO)
                        .size(size::SMALL)
                        .color(style::color(t.text_weak)),
                ]
                .spacing(10)
                .align_y(Center),
            );
        }
    }
    part.into()
}

/// An option to pick: a mark, its name, and more about it.
fn choice<'a>(
    t: &Tokens,
    glyph: Icon,
    (label, description): (&str, Option<&str>),
    chosen: bool,
    message: Message,
) -> Element<'a, Message> {
    let mark = if chosen {
        t.text_interactive_base
    } else {
        t.icon_base
    };
    let mut words = row![
        text(label.to_owned())
            .size(size::LABEL)
            .color(style::color(t.text_strong)),
    ]
    .spacing(10)
    .align_y(Center);
    if let Some(description) = description {
        words = words.push(
            text(description.to_owned())
                .size(size::SMALL)
                .color(style::color(t.text_weak)),
        );
    }
    button(
        row![icon(glyph, 14.0, mark), words]
            .spacing(10)
            .align_y(Center),
    )
    .padding([6, 10])
    .on_press(message)
    .style(style::menu_row(t, chosen))
    .into()
}

/// A text box for a field.
fn field_input<'a>(
    t: &Tokens,
    placeholder: &str,
    value: &str,
    on_input: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    text_input(placeholder, value)
        .on_input(on_input)
        .size(size::LABEL)
        .padding([7, 10])
        .width(Length::Fill)
        .style(style::menu_filter(t))
        .into()
}

/// A card's button: filled when it's the main action, plain otherwise.
fn action<'a>(
    t: &Tokens,
    label: &str,
    message: Option<Message>,
    id: Option<&'static str>,
    main: bool,
) -> Element<'a, Message> {
    let face = text(label.to_owned()).size(size::LABEL).font(SANS_MEDIUM);
    let face = if main {
        face
    } else {
        face.color(style::color(t.text_strong))
    };
    let button = button(face).padding([6, 12]).on_press_maybe(message);
    let button = if main {
        button.style(style::primary(t))
    } else {
        button.style(style::chip(t))
    };
    match id {
        Some(id) => container(button).id(id).into(),
        None => button.into(),
    }
}
