//! What the composer sends: menus for `/` and `@`, and the outgoing prompt.
//!
//! The composer's text is plain; what it brings along is kept beside it in a
//! [`Draft`]. A `/` at the very start picks a command or skill. An `@` word
//! picks a file, a subagent or an MCP resource; the choice is remembered with
//! the text that names it, and only the names still in the text are sent.

// Rust guideline compliant 2026-10-09

use opencode_client::{Attachment, Delivery, Mention, Picture, Prompt};

use crate::server::{Catalog, Command, FileMatch};

/// The built-in command that summarizes the conversation.
pub const COMPACT: &str = "compact";

/// The built-in command that asks a side question: answered from the
/// session's context, without adding to the conversation.
pub const ASIDE: &str = "btw";

/// Which menu a typed word opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    /// `/` at the start: commands and skills.
    Slash,
    /// `@`: files, subagents and MCP resources.
    At,
}

/// The word under the cursor that opens a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trigger {
    /// Which menu it opens.
    pub menu: Menu,
    /// The line it's on.
    pub line: usize,
    /// Byte offset in the line where the word starts (at the `/` or `@`).
    pub start: usize,
    /// Byte offset just past the word, at the cursor.
    pub end: usize,
    /// What follows the `/` or `@`.
    pub query: String,
}

/// Finds the menu word ending at the cursor, if there is one.
///
/// `line` is the text of the cursor's line, `index` its line number and
/// `column` the cursor's byte offset in it. A slash word only counts at the
/// very start of the text.
#[must_use]
pub fn trigger(line: &str, index: usize, column: usize) -> Option<Trigger> {
    let before = line.get(..column)?;
    let start = before.rfind(char::is_whitespace).map_or(0, |at| {
        at + before[at..].chars().next().map_or(1, char::len_utf8)
    });
    let word = &before[start..];
    let menu = match word.chars().next()? {
        '@' => Menu::At,
        '/' if index == 0 && start == 0 => Menu::Slash,
        _ => return None,
    };
    Some(Trigger {
        menu,
        line: index,
        start,
        end: column,
        query: word[1..].to_owned(),
    })
}

/// Something a menu offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// A server command, by name.
    Command(String),
    /// A skill, by ID.
    Skill(String),
    /// Summarize the conversation.
    Compact,
    /// Ask a side question.
    Aside,
    /// A file or folder, relative to the session's folder.
    File {
        /// The path as the server listed it.
        path: String,
        /// Whether it's a folder.
        directory: bool,
    },
    /// A subagent, by ID.
    Agent(String),
    /// An MCP resource.
    Resource {
        /// Where the server reads it.
        uri: String,
        /// Its name.
        name: String,
    },
}

/// One menu row: what it offers and how it reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    /// What picking it does.
    pub choice: Choice,
    /// The main text.
    pub label: String,
    /// A description, or where it comes from.
    pub detail: String,
    /// How a command runs, when it says.
    pub runner: Option<Runner>,
}

/// How a command runs, as its menu row shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runner {
    /// The agent's name for people, when the command names one.
    pub agent: Option<String>,
    /// That agent's configured colour, such as `#4f9dff`.
    pub color: Option<String>,
    /// Whether it runs as a subagent, in a child session of its own.
    pub subagent: bool,
}

impl Runner {
    /// How `command` runs, or `None` when it runs like a message would.
    fn of(catalog: &Catalog, command: &Command) -> Option<Self> {
        if command.agent.is_none() && !command.subagent {
            return None;
        }
        let known = command
            .agent
            .as_deref()
            .and_then(|id| catalog.agents.iter().find(|a| a.id == id));
        Some(Self {
            agent: known
                .map(|a| a.name.clone())
                .or_else(|| command.agent.clone()),
            color: known.and_then(|a| a.color.clone()),
            subagent: command.subagent,
        })
    }
}

/// How the slash menu reads for `query`: matching commands, skills and `compact`.
#[must_use]
pub fn slash_options(catalog: &Catalog, query: &str) -> Vec<MenuItem> {
    let mut options: Vec<MenuItem> = catalog
        .commands
        .iter()
        .map(|c| MenuItem {
            choice: Choice::Command(c.name.clone()),
            label: format!("/{}", c.name),
            detail: c.description.clone().unwrap_or_default(),
            runner: Runner::of(catalog, c),
        })
        .collect();
    if !catalog.commands.iter().any(|c| c.name == COMPACT) {
        options.push(MenuItem {
            choice: Choice::Compact,
            label: format!("/{COMPACT}"),
            detail: "Summarize the conversation to free up context".to_owned(),
            runner: None,
        });
    }
    if !catalog.commands.iter().any(|c| c.name == ASIDE) {
        options.push(MenuItem {
            choice: Choice::Aside,
            label: format!("/{ASIDE}"),
            detail: "Ask a side question, kept out of the chat".to_owned(),
            runner: None,
        });
    }
    options.extend(catalog.skills.iter().map(|s| MenuItem {
        choice: Choice::Skill(s.id.clone()),
        label: format!("/{}", s.name),
        detail: s.description.clone().unwrap_or_else(|| "Skill".to_owned()),
        runner: None,
    }));
    rank(options, query)
}

/// How the `@` menu reads for `query`, given the files the server found for it.
#[must_use]
pub fn at_options(catalog: &Catalog, files: &[FileMatch], query: &str) -> Vec<MenuItem> {
    let agents = catalog
        .agents
        .iter()
        .filter(|a| !a.primary)
        .map(|a| MenuItem {
            choice: Choice::Agent(a.id.clone()),
            label: format!("@{}", a.id),
            detail: a
                .description
                .clone()
                .unwrap_or_else(|| "Subagent".to_owned()),
            runner: None,
        });
    let resources = catalog.resources.iter().map(|r| MenuItem {
        choice: Choice::Resource {
            uri: r.uri.clone(),
            name: r.name.clone(),
        },
        label: format!("@{}", r.name),
        detail: r.server.clone(),
        runner: None,
    });
    let mut options = rank(agents.chain(resources).collect(), query);
    // The server already matched and ordered the files; they only move
    // ahead of agents when their name starts with the query.
    options.extend(files.iter().map(|f| MenuItem {
        choice: Choice::File {
            path: f.path.clone(),
            directory: f.directory,
        },
        label: format!("@{}", f.path),
        detail: if f.directory { "Folder" } else { "File" }.to_owned(),
        runner: None,
    }));
    let query = query.to_lowercase();
    options.sort_by_key(|option| !starts_with(option, &query));
    options
}

/// Whether an option's name, or a file's own name, starts with `query` (lowercase).
fn starts_with(option: &MenuItem, query: &str) -> bool {
    let name = option.label.trim_start_matches(['/', '@']).to_lowercase();
    let file = name
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default();
    name.starts_with(query) || file.starts_with(query)
}

/// Keeps the options whose label contains `query`, labels starting with it first.
fn rank(options: Vec<MenuItem>, query: &str) -> Vec<MenuItem> {
    let query = query.to_lowercase();
    let mut kept: Vec<(bool, MenuItem)> = options
        .into_iter()
        .filter_map(|option| {
            let label = option.label.to_lowercase();
            let name = label.trim_start_matches(['/', '@']);
            name.contains(&query)
                .then(|| (!name.starts_with(&query), option))
        })
        .collect();
    kept.sort_by_key(|(later, _)| *later);
    kept.into_iter().map(|(_, option)| option).collect()
}

/// The text a picked option puts in place of the menu word, with a space after it.
#[must_use]
pub fn insertion(choice: &Choice, catalog: &Catalog) -> String {
    match choice {
        Choice::Command(name) => format!("/{name} "),
        Choice::Compact => format!("/{COMPACT}"),
        Choice::Aside => format!("/{ASIDE} "),
        Choice::Skill(id) => {
            let name = catalog
                .skills
                .iter()
                .find(|s| s.id == *id)
                .map_or(id.as_str(), |s| s.name.as_str());
            format!("/{name} ")
        }
        Choice::File { path, .. } => format!("@{path} "),
        Choice::Agent(id) => format!("@{id} "),
        Choice::Resource { name, .. } => format!("@{name} "),
    }
}

/// A file or image attached to the draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attached {
    /// The name shown on its chip.
    pub name: String,
    /// What's sent.
    pub attachment: Attachment,
    /// The picture it carries inline, for its thumbnail.
    pub picture: Option<Picture>,
}

impl Attached {
    /// An attachment shown as `name`, its picture read out once.
    #[must_use]
    pub fn new(name: &str, attachment: Attachment) -> Self {
        Self {
            name: name.to_owned(),
            picture: attachment.picture(),
            attachment,
        }
    }
}

/// What the composer brings along with its text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Draft {
    /// Files and images attached with the button or by pasting.
    pub attached: Vec<Attached>,
    /// `@` choices, with the text that names them (such as `@src/main.rs`).
    pub mentions: Vec<(String, Choice)>,
}

impl Draft {
    /// Remembers an `@` choice, named in the text by `typed`.
    pub fn mention(&mut self, typed: String, choice: Choice) {
        if !self.mentions.iter().any(|(t, _)| *t == typed) {
            self.mentions.push((typed, choice));
        }
    }

    /// Whether there's nothing attached.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.attached.is_empty() && self.mentions.is_empty()
    }
}

/// What sending the composer does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outgoing {
    /// Send a prompt.
    Prompt(Prompt),
    /// Run a server command with its arguments.
    Command {
        /// The command's name.
        name: String,
        /// The text after the command.
        arguments: String,
    },
    /// Summarize the conversation.
    Compact,
    /// Ask a side question, possibly empty: answered from the session's
    /// context without adding to the conversation.
    Aside(String),
}

/// Turns the composer's text and draft into what's sent.
///
/// `folder` is the session's folder; mentioned files are sent by their path
/// in it. A leading `/name` runs a command, loads a skill, compacts, or
/// asks a side question.
#[must_use]
pub fn outgoing(
    text: &str,
    draft: &Draft,
    catalog: &Catalog,
    folder: &str,
    delivery: Delivery,
) -> Outgoing {
    let text = text.trim();
    let mut skills = Vec::new();
    let mut body = text.to_owned();
    if let Some(rest) = text.strip_prefix('/') {
        let (name, arguments) = rest
            .split_once(char::is_whitespace)
            .map_or((rest, ""), |(n, a)| (n, a.trim_start()));
        if catalog.commands.iter().any(|c| c.name == name) {
            return Outgoing::Command {
                name: name.to_owned(),
                arguments: arguments.to_owned(),
            };
        }
        if name == COMPACT && arguments.is_empty() {
            return Outgoing::Compact;
        }
        if name == ASIDE {
            return Outgoing::Aside(arguments.to_owned());
        }
        if let Some(skill) = catalog
            .skills
            .iter()
            .find(|s| s.name == name || s.id == name)
        {
            skills.push(skill.id.clone());
            if !arguments.is_empty() {
                arguments.clone_into(&mut body);
            }
        }
    }

    let mut prompt = Prompt::new(body, delivery);
    prompt.skills = skills;
    for (typed, choice) in &draft.mentions {
        let Some(mention) = Mention::find(&prompt.text, typed) else {
            // The mention was edited away.
            continue;
        };
        match choice {
            Choice::File { path, .. } => {
                let absolute = if path.starts_with('/') {
                    path.clone()
                } else {
                    format!("{}/{path}", folder.trim_end_matches('/'))
                };
                let mut file = Attachment::file(&absolute).mentioned(Some(mention));
                file.name = Some(path.trim_end_matches('/').to_owned());
                prompt.files.push(file);
            }
            Choice::Agent(id) => prompt.agents.push(id.clone()),
            Choice::Resource { uri, name } => prompt.files.push(Attachment {
                uri: uri.clone(),
                name: Some(name.clone()),
                mention: Some(mention),
            }),
            Choice::Command(_) | Choice::Skill(_) | Choice::Compact | Choice::Aside => {}
        }
    }
    prompt
        .files
        .extend(draft.attached.iter().map(|a| a.attachment.clone()));
    Outgoing::Prompt(prompt)
}
