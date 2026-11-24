//! Permission rules: which actions on which resources run, ask first, or
//! are refused.
//!
//! A rule names an action (`edit`, `shell`, `read`, … or `*`), a resource
//! pattern (a path or command; `*` matches anything) and an effect. Rules
//! come in layers: the agent's own, then the configuration's, then the
//! session's. The last rule that matches decides, as in OpenCode, and the
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

/// One permission rule, in OpenCode 2's shape.
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
        glob(&self.action, action) && glob(&self.resource, resource)
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

/// The rules every session's default agent starts from, as OpenCode's
/// build agent has them: everything runs, except reading secrets and
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
    fn patterns_match_like_opencodes() {
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
