//! Permission rules: which actions on which resources run, ask first, or
//! are refused.
//!
//! A rule names an action (`edit`, `shell`, `read`, … or `*`), a resource
//! pattern (a path or command; `*` matches anything) and an effect. Rules
//! come in layers: the agent's own, then the configuration's, then the
//! session's. The last rule that matches decides, and the
//! decision says which rule and layer it was, so a client can show why.

// Rust guideline compliant 2026-10-09

use serde::{Deserialize, Serialize};

/// What a matching rule does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    /// Runs without asking.
    Allow,
    /// Waits for a person's answer.
    Ask,
    /// Refused.
    Deny,
}

/// Programs whose second word names what they do (`git status`), so an
/// approval of one of their commands covers that subcommand only.
const SUBCOMMANDS: &[&str] = &[
    "bun", "cargo", "docker", "gh", "git", "go", "kubectl", "npm", "pip", "pnpm", "uv", "yarn",
];

/// The patterns "always" would allow for `action` on `resources`: for a
/// shell command, its program (and subcommand) with any arguments, such
/// as `git status *`; for anything else, every resource.
#[must_use]
pub fn save_patterns(action: &str, resources: &[String]) -> Vec<String> {
    if action != "shell" {
        return vec!["*".to_owned()];
    }
    let mut out: Vec<String> = Vec::new();
    for command in resources {
        let mut words = command.split_whitespace();
        let Some(program) = words.next() else {
            continue;
        };
        let mut lead = program.to_owned();
        if SUBCOMMANDS.contains(&program)
            && let Some(sub) = words.next().filter(|w| {
                w.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') && !w.starts_with('-')
            })
        {
            lead = format!("{lead} {sub}");
        }
        let pattern = format!("{lead} *");
        if !out.contains(&pattern) {
            out.push(pattern);
        }
    }
    if out.is_empty() {
        out.push("*".to_owned());
    }
    out
}

/// One permission rule, as the config writes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// The action it covers, or `*` for every action.
    pub action: String,
    /// The resources it covers: a pattern where `*` matches any text.
    pub resource: String,
    /// What it does to a match.
    pub effect: Effect,
}

impl Rule {
    /// A rule for `action` on every resource.
    #[must_use]
    pub fn any(action: &str, effect: Effect) -> Self {
        Self {
            action: action.to_owned(),
            resource: "*".to_owned(),
            effect,
        }
    }

    fn matches(&self, action: &str, resource: &str) -> bool {
        // A pattern ending in " *" also covers the command with no
        // arguments: `git status *` covers `git status`.
        let bare = self
            .resource
            .strip_suffix(" *")
            .is_some_and(|lead| glob(lead, resource));
        glob(&self.action, action) && (bare || glob(&self.resource, resource))
    }
}

/// What the rules say about one action on one resource, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// Allow, ask or deny.
    pub effect: Effect,
    /// The rule that decided, with its layer's name (`agent`, `config` or
    /// `session`); none when no rule matched and the default applied.
    pub because: Option<(String, Rule)>,
}

/// The rules every session's default agent starts from: everything runs,
/// except reading secrets and
/// shell commands, which ask.
#[must_use]
pub fn agent_rules() -> Vec<Rule> {
    let rule = |action: &str, resource: &str, effect| Rule {
        action: action.to_owned(),
        resource: resource.to_owned(),
        effect,
    };
    vec![
        rule("*", "*", Effect::Allow),
        rule("read", "*.env", Effect::Ask),
        rule("read", "*.env.*", Effect::Ask),
        rule("read", "*.env.example", Effect::Allow),
        rule("shell", "*", Effect::Ask),
    ]
}

/// Decides `action` on `resource` from rule layers, each named, applied in
/// order; the last matching rule of all decides, and with none, it asks.
#[must_use]
pub fn decide(layers: &[(&str, &[Rule])], action: &str, resource: &str) -> Decision {
    let found = layers.iter().rev().find_map(|(layer, rules)| {
        rules
            .iter()
            .rev()
            .find(|r| r.matches(action, resource))
            .map(|r| (*layer, r))
    });
    match found {
        Some((layer, rule)) => Decision {
            effect: rule.effect,
            because: Some((layer.to_owned(), rule.clone())),
        },
        None => Decision {
            effect: Effect::Ask,
            because: None,
        },
    }
}

/// Whether `text` matches `pattern`, where `*` stands for any text
/// (including none) and `?` for one character.
pub(crate) fn glob(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti) = (0, 0);
    // Where the last `*` was, and how much text it had taken then.
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        // A `*` in the pattern is always a wildcard, even against a `*`
        // in the text (a glob pattern as the resource).
        if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trailing_star_covers_the_bare_command() {
        let rule = Rule {
            action: "shell".to_owned(),
            resource: "git status *".to_owned(),
            effect: Effect::Allow,
        };
        assert!(rule.matches("shell", "git status"));
        assert!(rule.matches("shell", "git status --short"));
        assert!(!rule.matches("shell", "git statuses"));
    }

    #[test]
    fn always_saves_a_commands_program_and_subcommand() {
        let saved = save_patterns(
            "shell",
            &["git status --short".to_owned(), "ls -la".to_owned()],
        );
        assert_eq!(saved, ["git status *", "ls *"]);
        assert_eq!(save_patterns("edit", &["a.txt".to_owned()]), ["*"]);
    }

    #[test]
    fn patterns_match_like_globs() {
        assert!(glob("*", "anything at/all"));
        assert!(glob("*", "*.txt") && glob("src/*", "src/**/*.rs"));
        assert!(glob("*.env", "secrets/.env") && glob("*.env", "a.env"));
        assert!(!glob("*.env", "a.env.example"));
        assert!(glob("git *", "git status") && !glob("git *", "gitk"));
        assert!(glob("a?c", "abc") && !glob("a?c", "ac"));
    }

    #[test]
    fn the_last_matching_rule_decides_and_says_which() {
        let agent = agent_rules();
        let session = [Rule::any("edit", Effect::Ask)];
        let layers: [(&str, &[Rule]); 2] = [("agent", &agent), ("session", &session)];
        let edit = decide(&layers, "edit", "a.txt");
        assert_eq!(edit.effect, Effect::Ask);
        assert_eq!(
            edit.because.as_ref().map(|(layer, _)| layer.as_str()),
            Some("session")
        );
        let read = decide(&layers, "read", "a.txt");
        assert_eq!(
            (read.effect, read.because.map(|(l, r)| (l, r.action))),
            (Effect::Allow, Some(("agent".into(), "*".into())))
        );
        assert_eq!(decide(&layers, "read", "x/.env").effect, Effect::Ask);
        assert_eq!(
            decide(&[], "edit", "a").effect,
            Effect::Ask,
            "no rules: ask"
        );
    }
}
