//! What the agent waits on the user for: approvals, questions, and the shield.
//!
//! The [`Shield`] is how much a session may do without asking, kept as the
//! session's own permission rules on the server. A [`Draft`] holds the answers
//! being filled into a question, checks them, and turns them into the typed
//! answers OpenCode takes. [`describe`] puts an approval request into words.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;

use opencode_client::{Answer, Effect, Field, FieldKind, Form, Permission, Rule};

/// How much a session may do without asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shield {
    /// The agent's own rules decide; by default, commands ask first.
    Agent,
    /// Edits and commands ask first.
    Careful,
    /// Nothing asks first.
    Trusted,
    /// Rules set elsewhere that match none of the above.
    Custom,
}

impl Shield {
    /// The modes the user can choose, in menu order.
    pub const CHOICES: [Self; 3] = [Self::Agent, Self::Careful, Self::Trusted];

    /// The mode a session's own `rules` amount to.
    #[must_use]
    pub fn of(rules: &[Rule]) -> Self {
        Self::CHOICES
            .into_iter()
            .find(|mode| mode.rules().as_deref() == Some(rules))
            .unwrap_or(Self::Custom)
    }

    /// The session rules that give this mode; `None` for [`Shield::Custom`].
    #[must_use]
    pub fn rules(self) -> Option<Vec<Rule>> {
        match self {
            Self::Agent => Some(Vec::new()),
            Self::Careful => Some(vec![
                Rule::any("edit", Effect::Ask),
                Rule::any("shell", Effect::Ask),
            ]),
            Self::Trusted => Some(vec![Rule::any("*", Effect::Allow)]),
            Self::Custom => None,
        }
    }

    /// The mode's name.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Agent => "Ask before commands",
            Self::Careful => "Ask before edits and commands",
            Self::Trusted => "Don't ask",
            Self::Custom => "Custom rules",
        }
    }

    /// What the mode means, in a line.
    #[must_use]
    pub fn detail(self) -> &'static str {
        match self {
            Self::Agent => "The agent's own rules decide",
            Self::Careful => "Every file change waits for you too",
            Self::Trusted => "Edits and commands run straight away",
            Self::Custom => "This session has rules set elsewhere",
        }
    }
}

/// An approval request in words: what's asked, and what it would act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Described {
    /// What the agent wants to do, completing "wants to …", such as "run a command".
    pub wants: String,
    /// What it acts on: the command, or the paths, one per line.
    pub detail: String,
}

/// Puts an approval request into words.
#[must_use]
pub fn describe(asked: &Permission) -> Described {
    let count = asked.resources.len();
    let wants = match asked.action.as_str() {
        "shell" if count > 1 => "run commands".to_owned(),
        "shell" => "run a command".to_owned(),
        "edit" if count > 1 => format!("edit {count} files"),
        "edit" => "edit a file".to_owned(),
        "read" => "read a file".to_owned(),
        "external_directory" => "work outside the project".to_owned(),
        "webfetch" => "fetch a web page".to_owned(),
        "websearch" => "search the web".to_owned(),
        "subagent" => "start a subagent".to_owned(),
        "skill" => "load a skill".to_owned(),
        other => format!("use {}", other.replace('_', " ")),
    };
    Described {
        wants,
        detail: asked.resources.join("\n"),
    }
}

/// What "Always allow" on `asked` would cover, in words, such as
/// "commands starting with “cargo test”".
///
/// OpenCode saves the request's patterns (`save`), where `*` stands for
/// anything. A pattern ending in `*` covers what starts with the rest, one
/// ending in a folder covers everything in it, and one without a `*`
/// covers just itself; any other is quoted as written. No patterns, or
/// only `*`, cover every request of the kind.
#[must_use]
pub fn always_scope(asked: &Permission) -> String {
    let shell = asked.action == "shell";
    let patterns: Vec<Pattern<'_>> = asked
        .save
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty() && *p != "*")
        .map(Pattern::read)
        .collect();
    if patterns.is_empty() {
        return match asked.action.as_str() {
            "shell" => "every command".to_owned(),
            "edit" => "every edit".to_owned(),
            "read" => "every read".to_owned(),
            _ => "every request like this one".to_owned(),
        };
    }
    let phrases: Vec<String> = patterns
        .iter()
        .map(|pattern| {
            if shell {
                match pattern {
                    Pattern::Starts(x) => format!("commands starting with “{x}”"),
                    Pattern::In(x) => format!("commands in “{x}”"),
                    Pattern::Like(x) => format!("commands like “{x}”"),
                    Pattern::Exactly(x) => format!("the command “{x}”"),
                }
            } else {
                match pattern {
                    Pattern::Starts(x) => format!("anything starting with “{x}”"),
                    Pattern::In(x) => format!("everything in “{x}”"),
                    Pattern::Like(x) => format!("anything like “{x}”"),
                    Pattern::Exactly(x) => format!("“{x}”"),
                }
            }
        })
        .collect();
    let phrases = match phrases.as_slice() {
        [rest @ .., last] if !rest.is_empty() => format!("{}, or {last}", rest.join(", ")),
        _ => phrases.concat(),
    };
    let what = match asked.action.as_str() {
        "edit" => "edits to ",
        "read" => "reads of ",
        "external_directory" => "work in ",
        "webfetch" => "fetches of ",
        "websearch" => "web searches for ",
        _ => "",
    };
    format!("{what}{phrases}")
}

/// A saved pattern, read for putting into words.
enum Pattern<'a> {
    /// What starts with this, from a trailing `*`.
    Starts(&'a str),
    /// Everything in this folder, from a trailing `/*` or `/**`.
    In(&'a str),
    /// A pattern with a `*` or `?` elsewhere, quoted as written.
    Like(&'a str),
    /// Just this.
    Exactly(&'a str),
}

impl<'a> Pattern<'a> {
    /// Reads `pattern`, already trimmed.
    fn read(pattern: &'a str) -> Self {
        let wild = |s: &str| s.contains(['*', '?']);
        if let Some(folder) = pattern
            .strip_suffix("**")
            .or_else(|| pattern.strip_suffix('*'))
            .filter(|s| s.ends_with('/') && !wild(s))
        {
            return Self::In(folder);
        }
        if let Some(start) = pattern
            .strip_suffix('*')
            .map(str::trim_end)
            .filter(|s| !s.is_empty() && !wild(s))
        {
            return Self::Starts(start);
        }
        if wild(pattern) {
            Self::Like(pattern)
        } else {
            Self::Exactly(pattern)
        }
    }
}

/// The answers being filled into one question, before they're sent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Draft {
    fields: HashMap<String, Entry>,
}

/// One field's answer so far.
#[derive(Debug, Clone, Default, PartialEq)]
struct Entry {
    /// Chosen option values, in the order chosen.
    chosen: Vec<String>,
    /// Text typed: a free answer, an "other" answer, or a number.
    typed: String,
    /// A yes or no.
    flag: Option<bool>,
}

/// Why a question's answers can't be sent yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The field it's about.
    pub key: String,
    /// What's wrong, for the user.
    pub text: String,
}

impl Draft {
    /// Starts from each field's default.
    #[must_use]
    pub fn new(form: &Form) -> Self {
        let fields = form
            .fields
            .iter()
            .map(|field| {
                let mut entry = Entry::default();
                match &field.kind {
                    FieldKind::Text {
                        options, default, ..
                    } => {
                        if let Some(default) = default {
                            if options.iter().any(|o| o.value == *default) {
                                entry.chosen.push(default.clone());
                            } else {
                                entry.typed.clone_from(default);
                            }
                        }
                    }
                    FieldKind::Number {
                        default: Some(n), ..
                    } => entry.typed = number_text(*n),
                    FieldKind::Boolean { default } => entry.flag = *default,
                    FieldKind::Multiselect { default, .. } => entry.chosen.clone_from(default),
                    FieldKind::Number { default: None, .. } | FieldKind::External { .. } => {}
                }
                (field.key.clone(), entry)
            })
            .collect();
        Self { fields }
    }

    /// Chooses `value` for a single-choice field, or toggles it in a multiple choice.
    ///
    /// A multiple choice already at its most choices takes no more.
    pub fn choose(&mut self, field: &Field, value: &str) {
        let entry = self.fields.entry(field.key.clone()).or_default();
        if let FieldKind::Multiselect { max_items, .. } = &field.kind {
            if let Some(at) = entry.chosen.iter().position(|v| v == value) {
                entry.chosen.remove(at);
            } else if max_items.is_none_or(|max| (entry.chosen.len() as u64) < max) {
                entry.chosen.push(value.to_owned());
            }
        } else {
            entry.chosen = vec![value.to_owned()];
            // Choosing an option replaces an answer of one's own.
            entry.typed.clear();
        }
    }

    /// Sets the text typed for a field: its answer, an answer of one's own, or a number.
    pub fn type_text(&mut self, field: &Field, text: String) {
        let entry = self.fields.entry(field.key.clone()).or_default();
        // In a single choice, an answer of one's own replaces the option.
        if matches!(&field.kind, FieldKind::Text { options, .. } if !options.is_empty())
            && !text.is_empty()
        {
            entry.chosen.clear();
        }
        entry.typed = text;
    }

    /// Sets a yes-or-no field.
    pub fn set_flag(&mut self, field: &Field, flag: bool) {
        self.fields.entry(field.key.clone()).or_default().flag = Some(flag);
    }

    /// Returns whether `value` is chosen in a field.
    #[must_use]
    pub fn is_chosen(&self, key: &str, value: &str) -> bool {
        self.fields
            .get(key)
            .is_some_and(|e| e.chosen.iter().any(|v| v == value))
    }

    /// Returns the text typed for a field.
    #[must_use]
    pub fn typed(&self, key: &str) -> &str {
        self.fields.get(key).map_or("", |e| e.typed.as_str())
    }

    /// Returns a yes-or-no field's answer, if given.
    #[must_use]
    pub fn flag(&self, key: &str) -> Option<bool> {
        self.fields.get(key).and_then(|e| e.flag)
    }

    /// Returns whether a field is asked now: not hidden, and its conditions hold.
    #[must_use]
    pub fn asks(&self, form: &Form, field: &Field) -> bool {
        !field.hidden && self.applies(form, field)
    }

    /// Returns whether a field's conditions hold for the answers so far.
    fn applies(&self, form: &Form, field: &Field) -> bool {
        field.when.iter().all(|condition| {
            let other = form.fields.iter().find(|f| f.key == condition.key);
            let answer = other.and_then(|f| self.answer(f));
            (answer.as_ref() == Some(&condition.value)) == condition.equal
        })
    }

    /// Returns a field's answer as it stands, if it has one.
    fn answer(&self, field: &Field) -> Option<Answer> {
        let entry = self.fields.get(&field.key)?;
        match &field.kind {
            FieldKind::Text { .. } => {
                let typed = entry.typed.trim();
                if typed.is_empty() {
                    entry.chosen.first().cloned().map(Answer::Text)
                } else {
                    Some(Answer::Text(typed.to_owned()))
                }
            }
            FieldKind::Number { .. } => entry
                .typed
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .map(Answer::Number),
            FieldKind::Boolean { .. } => entry.flag.map(Answer::Boolean),
            FieldKind::Multiselect { .. } => {
                let mut list = entry.chosen.clone();
                let typed = entry.typed.trim();
                if !typed.is_empty() {
                    list.push(typed.to_owned());
                }
                (!list.is_empty()).then_some(Answer::List(list))
            }
            FieldKind::External { .. } => None,
        }
    }

    /// Checks the answers and returns them as OpenCode takes them, by field key.
    ///
    /// Fields whose conditions don't hold are left out; hidden fields send
    /// their defaults.
    ///
    /// # Errors
    ///
    /// Returns every [`Problem`]: a required field without an answer, a
    /// number that isn't one or is out of range, or too few or many choices.
    pub fn answers(&self, form: &Form) -> Result<Vec<(String, Answer)>, Vec<Problem>> {
        let mut answers = Vec::new();
        let mut problems = Vec::new();
        for field in form.fields.iter().filter(|f| self.applies(form, f)) {
            let problem = |text: String| Problem {
                key: field.key.clone(),
                text,
            };
            let typed = self.typed(&field.key).trim();
            let answer = self.answer(field);
            match (&field.kind, &answer) {
                (FieldKind::Number { .. }, None) if !typed.is_empty() => {
                    problems.push(problem("Enter a number".to_owned()));
                    continue;
                }
                (
                    FieldKind::Number {
                        integer,
                        minimum,
                        maximum,
                        ..
                    },
                    Some(Answer::Number(n)),
                ) => {
                    if *integer && n.fract() != 0.0 {
                        problems.push(problem("Enter a whole number".to_owned()));
                    } else if minimum.is_some_and(|min| *n < min) {
                        problems.push(problem(format!(
                            "At least {}",
                            number_text(minimum.unwrap_or_default())
                        )));
                    } else if maximum.is_some_and(|max| *n > max) {
                        problems.push(problem(format!(
                            "At most {}",
                            number_text(maximum.unwrap_or_default())
                        )));
                    }
                }
                (
                    FieldKind::Multiselect {
                        min_items,
                        max_items,
                        ..
                    },
                    _,
                ) => {
                    let count = match &answer {
                        Some(Answer::List(list)) => list.len() as u64,
                        _ => 0,
                    };
                    if let Some(min) = min_items.filter(|min| count < *min && count > 0) {
                        problems.push(problem(format!("Choose at least {min}")));
                    } else if let Some(max) = max_items.filter(|max| count > *max) {
                        problems.push(problem(format!("Choose at most {max}")));
                    }
                }
                _ => {}
            }
            match answer {
                Some(answer) => answers.push((field.key.clone(), answer)),
                None if field.required && !matches!(field.kind, FieldKind::External { .. }) => {
                    problems.push(problem("Needs an answer".to_owned()));
                }
                None => {}
            }
        }
        if problems.is_empty() {
            Ok(answers)
        } else {
            Err(problems)
        }
    }
}

/// Writes a number without a trailing `.0` when it's whole.
fn number_text(n: f64) -> String {
    let text = n.to_string();
    text.strip_suffix(".0")
        .map_or_else(|| text.clone(), str::to_owned)
}
