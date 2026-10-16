//! Desktop notification settings and templates (Settings → Notifications):
//! which events notify, and their title / message with `{variables}`.

use serde_json::{Value, json};

/// The events, in the order Settings shows them.
pub const EVENTS: [&str; 4] = ["completion", "subagent", "error", "question"];

/// Template variables, in the order Settings lists them.
pub const VARIABLES: [&str; 7] = ["project_name", "worktree", "branch", "session_name", "agent_name", "model_name", "last_message"];

/// Default (title, message) per event.
pub fn default_template(event: &str) -> (&'static str, &'static str) {
    match event {
        "completion" => ("{agent_name} is ready", "{model_name} completed the task"),
        "subagent" => ("{agent_name} is ready", "{model_name} completed the task"),
        "error" => ("Tool error", "{last_message}"),
        "question" => ("Input needed", "{last_message}"),
        _ => ("", ""),
    }
}

/// The notification settings, as saved (JSON in the "notify" setting).
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    /// Also while harlo is the focused window (never for the session in
    /// view).
    pub while_focused: bool,
    pub sound: bool,
    /// Per event: on, title, message ("" = the default template).
    pub events: Vec<(String, bool, String, String)>,
    pub quiet: Quiet,
}

/// Quiet hours: between `from` and `to` (minutes after midnight, local
/// time; past midnight when `to` is earlier) harlo shows no notifications
/// on the desktop, and phones get theirs silently (no sound, the screen
/// stays dark), except questions and approvals with `asks`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quiet {
    pub on: bool,
    pub from: u16,
    pub to: u16,
    /// Questions and approvals come through as usual.
    pub asks: bool,
}

impl Default for Quiet {
    fn default() -> Self {
        Self { on: false, from: 22 * 60, to: 8 * 60, asks: false }
    }
}

impl Quiet {
    /// Whether `minute` (after midnight) is in the quiet hours.
    pub fn covers(&self, minute: u16) -> bool {
        if !self.on || self.from == self.to {
            return false;
        }
        if self.from < self.to { (self.from..self.to).contains(&minute) } else { minute >= self.from || minute < self.to }
    }

    /// Whether a notification (`question`: a question or an approval) is
    /// hushed at `minute`.
    pub fn hushes(&self, minute: u16, question: bool) -> bool {
        self.covers(minute) && !(question && self.asks)
    }

    /// Whether it's quiet now, by the local clock.
    pub fn hushes_now(&self, question: bool) -> bool {
        use chrono::Timelike;
        let now = chrono::Local::now();
        self.hushes((now.hour() * 60 + now.minute()) as u16, question)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            while_focused: true,
            sound: false,
            events: EVENTS.iter().map(|e| ((*e).to_owned(), true, String::new(), String::new())).collect(),
            quiet: Quiet::default(),
        }
    }
}

impl Settings {
    /// From the saved JSON, else from the older "notifications" choice
    /// ("all", "background", "off"); missing parts take their defaults.
    pub fn load(saved: &str, legacy: &str) -> Self {
        if saved.trim().is_empty() {
            let mut s = Self::default();
            match legacy {
                "off" => s.enabled = false,
                "background" => s.while_focused = false,
                _ => {}
            }
            return s;
        }
        Self::parse(saved)
    }

    /// From the saved JSON; missing or invalid parts take their defaults.
    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        let Ok(v) = serde_json::from_str::<Value>(text) else { return s };
        s.enabled = v["enabled"].as_bool().unwrap_or(s.enabled);
        s.while_focused = v["whileFocused"].as_bool().unwrap_or(s.while_focused);
        s.sound = v["sound"].as_bool().unwrap_or(s.sound);
        let q = &v["quiet"];
        let minute = |v: &Value, or: u16| v.as_u64().filter(|m| *m < 24 * 60).map_or(or, |m| m as u16);
        s.quiet = Quiet {
            on: q["on"].as_bool().unwrap_or(s.quiet.on),
            from: minute(&q["from"], s.quiet.from),
            to: minute(&q["to"], s.quiet.to),
            asks: q["asks"].as_bool().unwrap_or(s.quiet.asks),
        };
        for (name, on, title, message) in &mut s.events {
            let e = &v["events"][name.as_str()];
            *on = e["on"].as_bool().unwrap_or(*on);
            if let Some(t) = e["title"].as_str() {
                *title = t.to_owned();
            }
            if let Some(m) = e["message"].as_str() {
                *message = m.to_owned();
            }
        }
        s
    }

    pub fn to_json(&self) -> Value {
        let mut events = serde_json::Map::new();
        for (name, on, title, message) in &self.events {
            events.insert(name.clone(), json!({ "on": on, "title": title, "message": message }));
        }
        let q = &self.quiet;
        json!({ "enabled": self.enabled, "whileFocused": self.while_focused, "sound": self.sound, "events": events,
                "quiet": { "on": q.on, "from": q.from, "to": q.to, "asks": q.asks } })
    }

    /// The event's switch and templates (defaults for empty ones).
    pub fn event(&self, name: &str) -> Option<(bool, &str, &str)> {
        let (title, message) = default_template(name);
        self.events.iter().find(|e| e.0 == name).map(|e| {
            (e.1, if e.2.trim().is_empty() { title } else { e.2.as_str() }, if e.3.trim().is_empty() { message } else { e.3.as_str() })
        })
    }

    /// A notification's title and body for `event`: its templates filled
    /// from `vars`, with `fallback` (title, body) where one comes out empty
    /// (or the message needs a `{last_message}` there isn't).
    pub fn render_event(&self, event: &str, vars: &[(&str, String)], fallback: (&str, &str)) -> (String, String) {
        let (_, title_t, message_t) = self.event(event).unwrap_or((true, "", ""));
        let title = render(title_t, vars);
        let last_missing = message_t.contains("{last_message}") && vars.iter().find(|(k, _)| *k == "last_message").is_none_or(|(_, v)| v.trim().is_empty());
        let message = if last_missing { String::new() } else { render(message_t, vars) };
        (
            if title.is_empty() { fallback.0.to_owned() } else { title },
            if message.is_empty() { fallback.1.to_owned() } else { message },
        )
    }
}

/// How long `{last_message}` may be.
pub const LAST_MESSAGE_CHARS: usize = 250;

/// Fills `{variable}`s from `vars` (name → value); unknown or empty ones
/// become "", and the result is tidied (spaces collapsed, edges trimmed).
pub fn render(template: &str, vars: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) if after[..end].chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !after[..end].is_empty() => {
                let name = &after[..end];
                if let Some((_, v)) = vars.iter().find(|(k, _)| *k == name) {
                    out.push_str(v);
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A reply shortened for a notification: markdown markers dropped, one
/// line, at most `max` characters.
pub fn summary(text: &str, max: usize) -> String {
    let plain: String = text
        .lines()
        .map(|l| l.trim().trim_start_matches(['#', '>', '-', '*']).trim())
        .filter(|l| !l.is_empty() && !l.starts_with("```"))
        .collect::<Vec<_>>()
        .join(" ")
        .replace("**", "")
        .replace('`', "");
    let plain = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    if plain.chars().count() <= max {
        return plain;
    }
    let cut: String = plain.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_templates() {
        let vars = [("agent_name", "omp".to_owned()), ("model_name", "GPT-5.6".to_owned()), ("branch", String::new())];
        assert_eq!(render("{agent_name} is ready", &vars), "omp is ready");
        assert_eq!(render("{model_name} completed the task", &vars), "GPT-5.6 completed the task");
        // Empty and unknown variables vanish; stray braces stay.
        assert_eq!(render("on {branch} {nope} done", &vars), "on done");
        assert_eq!(render("a {b c} {", &vars), "a {b c} {");
    }

    #[test]
    fn quiet_hours() {
        let night = Quiet { on: true, from: 22 * 60, to: 7 * 60 + 30, asks: true };
        assert!(night.covers(23 * 60) && night.covers(0) && night.covers(7 * 60 + 29));
        assert!(!night.covers(7 * 60 + 30) && !night.covers(12 * 60) && !night.covers(22 * 60 - 1));
        assert!(night.hushes(23 * 60, false) && !night.hushes(23 * 60, true), "questions come through with asks");
        let lunch = Quiet { on: true, from: 12 * 60, to: 13 * 60, asks: false };
        assert!(lunch.covers(12 * 60) && !lunch.covers(13 * 60) && !lunch.covers(11 * 60));
        assert!(lunch.hushes(12 * 60 + 5, true));
        assert!(!Quiet { on: false, ..lunch }.covers(12 * 60), "off");
        assert!(!Quiet { from: 600, to: 600, ..lunch }.covers(600), "an empty span");
        let mut s = Settings::default();
        s.quiet = night;
        assert_eq!(Settings::parse(&s.to_json().to_string()).quiet, night);
        assert_eq!(Settings::parse(r#"{"quiet":{"on":true,"from":5000}}"#).quiet, Quiet { on: true, ..Quiet::default() }, "a bad time: the default");
    }

    #[test]
    fn settings_round_trip() {
        let mut s = Settings::default();
        assert_eq!(s.event("question"), Some((true, "Input needed", "{last_message}")));
        assert!(!Settings::load("", "off").enabled);
        assert!(!Settings::load("", "background").while_focused);
        assert_eq!(Settings::load("", "all"), Settings::default());
        s.while_focused = true;
        s.events[1].1 = false;
        s.events[2].2 = "Oops".into();
        let back = Settings::parse(&s.to_json().to_string());
        assert_eq!(back, s);
        // Old or broken values: defaults.
        assert_eq!(Settings::parse("all"), Settings::default());
        assert!(!Settings::parse(r#"{"enabled":false}"#).enabled);
    }

    #[test]
    fn renders_events_with_fallbacks() {
        let mut s = Settings::default();
        let vars = [("agent_name", "Build".to_owned()), ("model_name", "GPT".to_owned()), ("last_message", String::new())];
        assert_eq!(s.render_event("completion", &vars, ("x", "y")), ("Build is ready".into(), "GPT completed the task".into()));
        // No last message: the fallback body.
        assert_eq!(s.render_event("question", &vars, ("Input needed", "Waiting for you")), ("Input needed".into(), "Waiting for you".into()));
        // A custom template; one that renders empty falls back.
        s.events[0].2 = "{session_name}".into();
        assert_eq!(s.render_event("completion", &vars, ("Agent is ready", "")).0, "Agent is ready");
    }

    #[test]
    fn summarises_replies() {
        assert_eq!(summary("## Done\n\n**All** tests pass.\n```\ncode\n```", 100), "Done All tests pass. code");
        assert_eq!(summary("abcdefghij", 5), "abcd…");
    }
}
