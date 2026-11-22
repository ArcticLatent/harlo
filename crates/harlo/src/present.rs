//! Shows the app's state in the window: fills the UI's `State` global and
//! its models from [`App`].
//!
//! It runs after every batch of updates. Lists keep one model each for the
//! window's lifetime and change only the rows that differ, so the window
//! rebuilds as little as it can; setting a property to the value it has
//! already costs nothing.

// Rust guideline compliant 2026-10-09

mod changes;
mod files;
mod notes;
mod panels;
mod session;
mod terminal;
mod timeline;
mod waiting;

use std::rc::Rc;

use harlo_core::approvals::Shield;
use harlo_core::server::Location;
use harlo_core::sessions::{SessionSummary, VISIBLE_SESSIONS, compact_age, visible};
use harlo_core::theme::{Rgba, Tokens};
use lucide_icons::Icon;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::app::{App, Document, FileBody, Panel, Status, capitalized};
use crate::model::{self, Picker, READ_ONLY, Variant};
use crate::theme::color;
use crate::ui::{
    AppWindow, Chip, Content, InboxRow, LocationRow, MenuItem, Question, RowKind, SidebarRow,
    State, Tab,
};

/// The models the window's lists show, kept for the window's lifetime.
pub struct Presenter {
    sidebar: Rc<VecModel<SidebarRow>>,
    locations: Rc<VecModel<LocationRow>>,
    /// The center's tabs.
    documents: Rc<VecModel<Tab>>,
    /// The terminals' tabs.
    terminals: Rc<VecModel<Tab>>,
    chips: Rc<VecModel<Chip>>,
    menu: Rc<VecModel<MenuItem>>,
    panels: Rc<VecModel<bool>>,
    /// The open session's timeline.
    timeline: timeline::Rows,
    inbox: Rc<VecModel<InboxRow>>,
    questions: Rc<VecModel<Question>>,
    /// The tool panels' lists.
    bodies: panels::Lists,
    changes: changes::Lists,
    files: files::Files,
    notes: notes::Notes,
    terminal: terminal::Terminal,
}

impl std::fmt::Debug for Presenter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Presenter")
            .field("documents", &self.documents.row_count())
            .field("sidebar", &self.sidebar.row_count())
            .finish_non_exhaustive()
    }
}

impl Presenter {
    /// Gives `window` the models it shows.
    #[must_use]
    pub fn new(window: &AppWindow) -> Self {
        let state = window.global::<State>();
        let presenter = Self {
            sidebar: Rc::new(VecModel::default()),
            locations: Rc::new(VecModel::default()),
            documents: Rc::new(VecModel::default()),
            terminals: Rc::new(VecModel::default()),
            chips: Rc::new(VecModel::default()),
            menu: Rc::new(VecModel::default()),
            panels: Rc::new(VecModel::from(vec![false; Panel::ALL.len()])),
            timeline: timeline::Rows::new(),
            inbox: Rc::new(VecModel::default()),
            questions: Rc::new(VecModel::default()),
            bodies: panels::Lists::new(&state),
            changes: changes::Lists::new(&state),
            files: files::Files::new(&state),
            notes: notes::Notes::default(),
            terminal: terminal::Terminal::new(&state),
        };
        state.set_sidebar_rows(ModelRc::from(Rc::clone(&presenter.sidebar)));
        state.set_locations(ModelRc::from(Rc::clone(&presenter.locations)));
        state.set_documents(ModelRc::from(Rc::clone(&presenter.documents)));
        state.set_terminals(ModelRc::from(Rc::clone(&presenter.terminals)));
        state.set_chips(ModelRc::from(Rc::clone(&presenter.chips)));
        state.set_menu_items(ModelRc::from(Rc::clone(&presenter.menu)));
        state.set_panels_shown(ModelRc::from(Rc::clone(&presenter.panels)));
        state.set_timeline(presenter.timeline.model());
        state.set_inbox(ModelRc::from(Rc::clone(&presenter.inbox)));
        state.set_questions(ModelRc::from(Rc::clone(&presenter.questions)));
        presenter
    }

    /// Shows `app` in `window`.
    pub fn present(&mut self, app: &App, window: &AppWindow) {
        let state = window.global::<State>();
        let t = &app.tokens;
        sync(
            &self.panels,
            Panel::ALL.iter().map(|&p| app.panel_shown(p)).collect(),
        );
        Self::sides(app, &state);
        self.sidebar(app, &state);
        self.center(app, &state);
        Self::composer(app, &state, t);
        self.pickers(app, &state, t);
        self.session(app, &state);
        self.bodies.show(app, &state);
        self.changes.show(app, &state);
        self.files.tree(app, &state);
        self.files.editor(app, &state, &mut self.timeline);
        self.notes.show(app, &state);
        self.terminal.show(app, &state);
    }

    /// The open session: its header, timeline, inbox and waiting cards.
    fn session(&mut self, app: &App, state: &State<'_>) {
        waiting::show(app, state, &self.questions);
        let id = app
            .active
            .as_deref()
            .filter(|id| app.timeline(id).is_some());
        let Some(id) = id else {
            self.timeline.show(&app.tokens, None);
            sync(&self.inbox, Vec::new());
            state.set_session_id(SharedString::new());
            return;
        };
        let crew = app.crew(id);
        if let Some(timeline) = app.timeline(id) {
            self.timeline.show(&app.tokens, Some((id, timeline, &crew)));
        }
        session::header(app, id, state);
        sync(&self.inbox, session::inbox(app, id));
    }

    /// The sidebar, its location button and the location picker.
    fn sidebar(&self, app: &App, state: &State<'_>) {
        let t = &app.tokens;
        state.set_location_label(app.location.label().into());
        state.set_location_glyph(glyph(location_icon(&app.location)));
        state.set_status_color(color(status_color(t, &app.status)));
        state.set_status_text(model::status_text(&app.status).into());
        state.set_picking_location(app.picking_location && app.shown().sidebar.is_some());
        sync(
            &self.locations,
            app.locations
                .iter()
                .map(|location| LocationRow {
                    label: location.label().into(),
                    detail: location_detail(location).into(),
                    glyph: glyph(location_icon(location)),
                    current: *location == app.location,
                })
                .collect(),
        );
        let notice = if app.projects.is_empty() {
            empty_text(&app.status)
        } else {
            String::new()
        };
        state.set_sidebar_notice(notice.into());
        sync(&self.sidebar, sidebar_rows(app));
    }

    /// The sidebar and the right panel: whether they're shown, and how big.
    fn sides(app: &App, state: &State<'_>) {
        let shown = app.shown();
        state.set_sidebar_shown(shown.sidebar.is_some());
        if let Some(width) = shown.sidebar {
            state.set_sidebar_width(width);
        }
        let panel = app.maximized.or_else(|| app.shown_panel());
        state.set_panel(panel.map_or(-1, panel_number));
        state.set_maximized(app.maximized.is_some());
        if let Some(width) = shown.panel {
            state.set_panel_width(width);
        }
        if let Some(panel) = panel {
            state.set_panel_title(panel.title().into());
            state.set_panel_refresh(can_refresh(app, panel));
        }
    }

    /// The center's tabs and what it shows, and the terminals under it.
    fn center(&self, app: &App, state: &State<'_>) {
        let current = app.current_document();
        sync(
            &self.documents,
            app.documents()
                .iter()
                .enumerate()
                .map(|(index, document)| tab(app, document, index, *document == current))
                .collect(),
        );
        let (content, quiet) = content(app, &current);
        state.set_content(content);
        state.set_quiet(quiet.into());
        state.set_terminals_shown(app.terminals_shown());
        state.set_terminal_height(app.terminal_height());
        let active = app.terminals.active().map(|s| s.id);
        sync(
            &self.terminals,
            app.terminals
                .shells()
                .iter()
                .map(|shell| Tab {
                    key: i32::try_from(shell.id).unwrap_or(i32::MAX),
                    title: shell.title.as_str().into(),
                    glyph: glyph(Icon::SquareTerminal),
                    glyph_color: color(app.tokens.icon_base),
                    active: active == Some(shell.id),
                    closable: true,
                    tooltip: shell.folder.as_str().into(),
                    ..Tab::default()
                })
                .collect(),
        );
    }

    /// The composer's text, when the app changed it, and its feedback.
    fn composer(app: &App, state: &State<'_>, t: &Tokens) {
        let revision = i32::try_from(app.composer.revision()).unwrap_or(i32::MAX);
        if state.get_composer_revision() != revision {
            state.set_composer_text(app.composer.as_str().into());
            state.set_composer_cursor(offset(app.composer.offset()));
            state.set_composer_anchor(offset(app.composer.anchor()));
            state.set_composer_revision(revision);
        }
        let feedback = app.feedback();
        state.set_sending(feedback.sending);
        state.set_pasting(feedback.pasting);
        state.set_can_send(
            !app.composer.as_str().trim().is_empty() || !app.draft.attached.is_empty(),
        );
        state.set_send_error(app.send_error.clone().unwrap_or_default().into());
        state.set_starting(app.starting());
        state.set_home_hint(
            if app.starting() {
                "Starting the session…"
            } else {
                "Enter to start · Shift+Enter for a new line"
            }
            .into(),
        );
        let (shield, shield_color) = match app.current_shield() {
            Shield::Agent => (Icon::ShieldUser, t.icon_base),
            Shield::Careful => (Icon::ShieldCheck, t.text_interactive_base),
            Shield::Trusted => (Icon::ShieldOff, t.icon_warning_base),
            Shield::Custom => (Icon::ShieldEllipsis, t.icon_base),
        };
        state.set_shield_glyph(glyph(shield));
        state.set_shield_color(color(shield_color));
    }

    /// The picker chips, and the open picker's menu.
    fn pickers(&self, app: &App, state: &State<'_>, t: &Tokens) {
        let folder = app
            .folder_choices()
            .into_iter()
            .find(|f| f.directory == app.new_folder)
            .map(|f| f.label)
            .unwrap_or_default();
        state.set_folder_chip(Chip {
            picker: Picker::Folder.index(),
            glyph: glyph(if app.new_folder.is_some() {
                Icon::Folder
            } else {
                Icon::House
            }),
            glyph_color: color(t.text_interactive_base),
            label: folder.into(),
            note: SharedString::new(),
        });
        sync(&self.chips, chips(app, t));
        let Some(picker) = app.picker else {
            state.set_picker(-1);
            return;
        };
        state.set_picker(picker.index());
        sync(
            &self.menu,
            model::items(app, picker)
                .into_iter()
                .map(|item| MenuItem {
                    label: item.label.into(),
                    detail: item.detail.unwrap_or_default().into(),
                    dot: item.dot.map(color).unwrap_or_default(),
                    has_dot: item.dot.is_some(),
                    selected: item.selected,
                })
                .collect(),
        );
        state.set_menu_highlight(offset(app.picker_index));
        state.set_menu_filter(app.picker_filter.as_str().into());
        let note = match picker {
            Picker::Agent => app
                .agent_choices()
                .into_iter()
                .nth(app.picker_index)
                .and_then(|agent| agent.description)
                .unwrap_or_default(),
            Picker::Shield => app.current_shield().detail().to_owned(),
            Picker::Folder | Picker::Variant | Picker::Model => String::new(),
        };
        state.set_menu_note(note.into());
    }
}

/// Makes `model` hold `rows`, changing only the rows that differ.
pub fn sync<T: Clone + PartialEq + 'static>(model: &VecModel<T>, rows: Vec<T>) {
    let old = model.row_count();
    for (index, row) in rows.iter().enumerate().take(old) {
        if model.row_data(index).as_ref() != Some(row) {
            model.set_row_data(index, row.clone());
        }
    }
    if rows.len() > old {
        model.extend(rows.into_iter().skip(old));
    } else {
        for index in (rows.len()..old).rev() {
            model.remove(index);
        }
    }
}

/// A Lucide icon's glyph, for the UI's icon font.
fn glyph(icon: Icon) -> SharedString {
    let mut text = SharedString::new();
    text.push_str(icon.unicode().encode_utf8(&mut [0; 4]));
    text
}

/// A byte offset or index as the UI's `int`.
fn offset(at: usize) -> i32 {
    i32::try_from(at).unwrap_or(i32::MAX)
}

/// The colour of the connection dot.
fn status_color(t: &Tokens, status: &Status) -> Rgba {
    match status {
        Status::Connected { .. } => t.icon_success_base,
        Status::Connecting { .. } => t.icon_warning_base,
        Status::Lost { .. } => t.icon_critical_base,
    }
}

/// The icon for a location: this computer, or another server.
fn location_icon(location: &Location) -> Icon {
    match location {
        Location::Shared => Icon::Monitor,
        Location::Server { .. } => Icon::Server,
    }
}

/// What a location is, under its name.
fn location_detail(location: &Location) -> &'static str {
    match location {
        Location::Shared => "OpenCode's background service on this computer",
        Location::Server { .. } => "Server given with --server",
    }
}

/// What the list says when it has no projects to show.
fn empty_text(status: &Status) -> String {
    match status {
        Status::Connecting { .. } => "Connecting to OpenCode…".to_owned(),
        Status::Lost { reason, .. } => format!("Can't connect: {reason}."),
        Status::Connected { .. } => "No projects yet. Add a folder with + Project.".to_owned(),
    }
}

/// The sidebar's rows: each project's heading and sessions, then its
/// worktrees' headings and sessions.
fn sidebar_rows(app: &App) -> Vec<SidebarRow> {
    let filtering = !app.filter.trim().is_empty();
    let mut rows = Vec::new();
    for project in &app.projects {
        let start = rows.len();
        rows.push(SidebarRow {
            kind: RowKind::Project,
            id: project.directory.as_str().into(),
            title: project.name.as_str().into(),
            detail: project.total().to_string().into(),
            busy: project.working() > 0,
            ..SidebarRow::default()
        });
        let mut any = session_rows(app, &project.directory, &project.sessions, &mut rows);
        for worktree in &project.worktrees {
            let heading = rows.len();
            rows.push(SidebarRow {
                kind: RowKind::Worktree,
                id: worktree.directory.as_str().into(),
                title: worktree.name.as_str().into(),
                detail: worktree.sessions.len().to_string().into(),
                ..SidebarRow::default()
            });
            if session_rows(app, &worktree.directory, &worktree.sessions, &mut rows) {
                any = true;
            } else if filtering {
                rows.truncate(heading);
            }
        }
        if filtering && !any {
            rows.truncate(start);
        } else if let Some(last) = rows.last_mut() {
            last.gap = true;
        }
    }
    rows
}

/// Adds one folder's session rows, with "Show N more"; returns whether
/// any showed.
fn session_rows(
    app: &App,
    folder: &str,
    sessions: &[SessionSummary],
    rows: &mut Vec<SidebarRow>,
) -> bool {
    let t = &app.tokens;
    let filtering = !app.filter.trim().is_empty();
    let expanded = app.expanded.contains(folder);
    let (shown, hidden) = visible(sessions, &app.filter, expanded);
    if shown.is_empty() {
        return false;
    }
    for session in shown {
        let selected = app.active.as_deref() == Some(session.id.as_str());
        let (maker, tone) = maker(t, session.maker.as_deref());
        rows.push(SidebarRow {
            kind: RowKind::Session,
            id: session.id.as_str().into(),
            title: if session.title.is_empty() {
                "Untitled session".into()
            } else {
                session.title.as_str().into()
            },
            untitled: session.title.is_empty(),
            detail: compact_age(session.age).into(),
            selected,
            busy: session.busy,
            waiting: session.waiting,
            maker,
            tone: color(tone),
            ..SidebarRow::default()
        });
        // Its subagents, while they matter: it or one of them is open, or
        // one is still working or asking.
        let open = selected
            || session
                .children
                .iter()
                .any(|c| app.active.as_deref() == Some(c.id.as_str()));
        for child in &session.children {
            if open || child.busy || child.waiting {
                rows.push(child_row(app, child));
            }
        }
    }
    if hidden > 0 || (expanded && !filtering && sessions.len() > VISIBLE_SESSIONS) {
        let title = if hidden > 0 {
            format!("Show {hidden} more")
        } else {
            "Show fewer".to_owned()
        };
        rows.push(SidebarRow {
            kind: RowKind::More,
            id: folder.into(),
            title: title.into(),
            ..SidebarRow::default()
        });
    }
    true
}

/// A subagent's session under its parent.
fn child_row(app: &App, child: &SessionSummary) -> SidebarRow {
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
    SidebarRow {
        kind: RowKind::Child,
        id: child.id.as_str().into(),
        title: child.title.as_str().into(),
        agent: name.into(),
        tone: color(agent.and_then(|a| a.color).unwrap_or(t.icon_weak_base)),
        selected: app.active.as_deref() == Some(child.id.as_str()),
        busy: child.busy,
        waiting: child.waiting,
        ..SidebarRow::default()
    }
}

/// The logo of a session's model maker, by OpenCode provider ID, and its
/// tone. Providers that serve one maker's models under several IDs, such
/// as `zai` and `zai-coding-plan`, share a logo.
fn maker(t: &Tokens, provider: Option<&str>) -> (i32, Rgba) {
    let is = |key: &str| provider.is_some_and(|id| id == key || id.starts_with(&format!("{key}-")));
    if is("anthropic") {
        (1, Rgba::parse("#e99b73").unwrap_or(t.text_base))
    } else if is("openai") {
        (2, t.text_base)
    } else if is("zai") {
        (3, Rgba::parse("#849cff").unwrap_or(t.text_base))
    } else {
        (0, t.text_base)
    }
}

/// The agent, reasoning level and model chips.
fn chips(app: &App, t: &Tokens) -> Vec<Chip> {
    let mut chips = Vec::new();
    if !app.agent_choices().is_empty() {
        // A subagent's session runs an agent the picker doesn't offer.
        let agent = app.current_agent().and_then(|id| app.agent(&id));
        let read_only = agent.as_ref().is_some_and(|a| a.read_only);
        chips.push(Chip {
            picker: Picker::Agent.index(),
            // An agent that leaves files alone looks, rather than builds.
            glyph: glyph(if read_only { Icon::Eye } else { Icon::Bot }),
            glyph_color: color(
                agent
                    .as_ref()
                    .and_then(|a| a.color)
                    .unwrap_or(t.text_interactive_base),
            ),
            label: agent
                .as_ref()
                .map_or_else(|| "Agent".to_owned(), |a| a.name.clone())
                .into(),
            note: if read_only { READ_ONLY } else { "" }.into(),
        });
    }
    if app.variant_choices().len() > 1 {
        let label = Variant(app.current_model().and_then(|m| m.variant)).to_string();
        chips.push(plain_chip(t, Picker::Variant, Icon::Brain, label));
    }
    let label = app
        .current_model_choice()
        .map_or_else(|| "Model".to_owned(), |m| m.name);
    chips.push(plain_chip(t, Picker::Model, Icon::Sparkles, label));
    chips
}

/// A chip with a glyph and a label alone.
fn plain_chip(t: &Tokens, picker: Picker, icon: Icon, label: String) -> Chip {
    Chip {
        picker: picker.index(),
        glyph: glyph(icon),
        glyph_color: color(t.text_interactive_base),
        label: label.into(),
        note: SharedString::new(),
    }
}

/// A center tab: its title, icon, and whether it can close. `index` is its
/// place among the center's tabs.
fn tab(app: &App, document: &Document, index: usize, active: bool) -> Tab {
    let t = &app.tokens;
    let (glyph, file_icon, glyph_color) = match document {
        Document::Editor => {
            let icon = crate::file_icons::icon(app.fs.open.first().map_or("", |f| f.path.as_str()));
            let mut text = SharedString::new();
            text.push_str(icon.glyph.encode_utf8(&mut [0; 4]));
            (text, true, icon.slint_color())
        }
        Document::Home => (glyph(Icon::Plus), false, color(t.icon_base)),
        Document::Session(_) => (glyph(Icon::Bot), false, color(t.icon_base)),
    };
    let (title, tooltip) = match document {
        Document::Home => ("New session".to_owned(), None),
        Document::Session(id) => (
            Some(app.title(id))
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| "Untitled session".to_owned()),
            app.session_folder(id),
        ),
        Document::Editor => (
            app.fs
                .open
                .first()
                .map_or_else(|| "Editor".to_owned(), |f| f.name().to_owned()),
            app.fs.open.first().map(|f| f.path.clone()),
        ),
    };
    Tab {
        key: offset(index),
        title: title.into(),
        glyph,
        file_icon,
        glyph_color,
        active,
        closable: *document != Document::Home,
        modified: *document == Document::Editor
            && app.fs.open.iter().any(crate::app::OpenFile::is_dirty),
        tooltip: tooltip.unwrap_or_default().into(),
    }
}

/// What the center shows for `document`, and its quiet text if it has
/// nothing yet.
fn content(app: &App, document: &Document) -> (Content, &'static str) {
    match document {
        Document::Home => (Content::Home, ""),
        Document::Session(id) if app.timeline(id).is_some() => (Content::Session, ""),
        Document::Session(_) => (Content::Quiet, "Opening the session…"),
        Document::Editor => match app.fs.open.first() {
            Some(file) if !matches!(file.body, FileBody::Loading) => (Content::Editor, ""),
            Some(_) => (Content::Quiet, "Opening the file…"),
            None => (Content::Quiet, "No file is open."),
        },
    }
}

/// Whether a panel's header offers Refresh.
fn can_refresh(app: &App, panel: Panel) -> bool {
    match panel {
        Panel::Context => app.active.is_some(),
        Panel::Changes | Panel::Files => true,
        Panel::Todos | Panel::Notes => false,
    }
}

/// A panel's number in the UI: its place in [`Panel::ALL`].
#[must_use]
pub fn panel_number(panel: Panel) -> i32 {
    Panel::ALL
        .iter()
        .position(|&p| p == panel)
        .map_or(-1, offset)
}

/// The panel the UI numbers `number`.
#[must_use]
pub fn panel_of(number: i32) -> Option<Panel> {
    usize::try_from(number)
        .ok()
        .and_then(|i| Panel::ALL.get(i).copied())
}
