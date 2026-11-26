//! Automations: a prompt, a shell command or a Code Mode script the server
//! runs at a time, every so often, or on a cron schedule, whether or not a
//! client is open.
//!
//! They're off until the config turns them on (`"automations": true`). A
//! command or script runs unattended, so making one needs the session's
//! rules to allow it without asking. A ticker checks once a second for
//! those that are due.

// Rust guideline compliant 2026-10-09

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use harlo_engine::tools::shell::{Ended, execute};
use harlo_engine::{Effect, ascending_id, decide, now};
use jiff::tz::TimeZone;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use crate::server::{Refusal, Server};
use crate::store::{Error, Store};

/// How long an automation's command or script may run.
const RUN_LIMIT: Duration = Duration::from_secs(600);

/// The shortest interval an automation may repeat at.
const SHORTEST: i64 = 60_000;

/// The most of a run's output kept, in bytes: its end.
const KEPT: usize = 32 * 1024;

/// An automation to make, as a client sends it.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct NewAutomation {
    #[serde(rename = "sessionID")]
    session: Option<String>,
    directory: Option<String>,
    name: String,
    trigger: Map<String, Value>,
    action: Map<String, Value>,
}

/// `text` as milliseconds, from words such as "90 seconds" or "2 hours".
fn duration(text: &str) -> Option<i64> {
    let text = text.trim().to_lowercase();
    let split = text.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let (number, unit) = text.split_at(split);
    let number: f64 = number.parse().ok()?;
    let unit = match unit.trim() {
        "ms" | "millisecond" | "milliseconds" => 1.0,
        "s" | "sec" | "secs" | "second" | "seconds" => 1_000.0,
        "m" | "min" | "mins" | "minute" | "minutes" => 60_000.0,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3_600_000.0,
        "d" | "day" | "days" => 86_400_000.0,
        "w" | "week" | "weeks" => 604_800_000.0,
        _ => return None,
    };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "durations are far below i64's range"
    )]
    Some((number * unit).round() as i64)
}

/// The time zone named `name`, or this computer's.
fn zone(name: Option<&str>) -> (TimeZone, String) {
    if let Some(found) = name.and_then(|n| TimeZone::get(n).ok()) {
        return (found, name.unwrap_or_default().to_owned());
    }
    let local = TimeZone::try_system().unwrap_or(TimeZone::UTC);
    let named = local.iana_name().unwrap_or("UTC").to_owned();
    (local, named)
}

/// When `at` is, in Unix milliseconds: "in 10 minutes", an instant with an
/// offset, or a wall-clock time in `tz`.
fn instant(at: &str, tz: &TimeZone) -> Option<i64> {
    if let Some(after) = at.trim().strip_prefix("in ") {
        return duration(after).map(|d| now() + d);
    }
    if let Ok(t) = at.parse::<jiff::Timestamp>() {
        return Some(t.as_millisecond());
    }
    let civil: jiff::civil::DateTime = at.parse().ok()?;
    let zoned = civil.to_zoned_datetime(tz.clone()).ok()?;
    Some(zoned.timestamp().as_millisecond())
}

/// The next time `cron` fires in `tz` after Unix milliseconds `after`.
fn next_cron(cron: &str, tz: &TimeZone, after: i64) -> Result<i64, String> {
    let pattern: croner::Cron = cron
        .parse()
        .map_err(|e| format!("\"{cron}\" isn't a cron schedule: {e}"))?;
    let from = jiff::Timestamp::from_millisecond(after)
        .map_err(|e| e.to_string())?
        .to_zoned_datetime(tz.clone());
    pattern
        .find_next_occurrence(&from, false)
        .map(|z| z.timestamp().as_millisecond())
        .map_err(|e| format!("The cron schedule \"{cron}\" never fires: {e}"))
}

/// Checks `trigger`; returns it as kept (with its time zone) and when it
/// first fires.
fn first_due(trigger: &Map<String, Value>) -> Result<(Value, i64), String> {
    let text = |k: &str| trigger.get(k).and_then(Value::as_str).unwrap_or_default();
    let (tz, tz_name) = zone(trigger.get("tz").and_then(Value::as_str));
    let mut kept = Value::Object(trigger.clone());
    kept["tz"] = json!(tz_name);
    let due = match text("type") {
        "at" => {
            let at = text("at");
            let due = instant(at, &tz).ok_or_else(|| {
                format!("Invalid time \"{at}\": say \"in 10 minutes\" or 2026-10-27T09:00.")
            })?;
            if due < now() - 1_000 {
                return Err(format!("Invalid time: \"{at}\" has passed."));
            }
            due
        }
        "every" => {
            let every = text("every");
            let ms = duration(every).ok_or_else(|| {
                format!("\"{every}\" isn't a duration; say \"5 minutes\" or \"2 hours\".")
            })?;
            if ms < SHORTEST {
                return Err(format!(
                    "\"{every}\" is too often: an automation repeats at least 1 minute apart."
                ));
            }
            now() + ms
        }
        "cron" => next_cron(text("cron"), &tz, now())?,
        "event" => {
            return Err(
                "Event triggers need an MCP server that sends events; none is connected."
                    .to_owned(),
            );
        }
        other => return Err(format!("\"{other}\" isn't a trigger: at, every or cron.")),
    };
    Ok((kept, due))
}

/// When `automation` fires next after firing at `due`; `None` when it's
/// done.
fn next_due(automation: &Value, due: i64) -> Option<i64> {
    let trigger = &automation["trigger"];
    let (tz, _) = zone(trigger["tz"].as_str());
    match trigger["type"].as_str() {
        Some("every") => {
            let ms = duration(trigger["every"].as_str()?)?;
            // A run missed while the server was down isn't made up.
            let mut next = due + ms;
            while next <= now() {
                next += ms;
            }
            Some(next)
        }
        Some("cron") => next_cron(trigger["cron"].as_str()?, &tz, now()).ok(),
        _ => None,
    }
}

/// The end of `text`, at most [`KEPT`] bytes.
fn kept(text: &str) -> String {
    if text.len() <= KEPT {
        return text.to_owned();
    }
    let mut from = text.len() - KEPT;
    while !text.is_char_boundary(from) {
        from += 1;
    }
    text[from..].to_owned()
}

impl Store {
    /// Makes the automations' table.
    pub(crate) fn make_automations(conn: &rusqlite::Connection) -> Result<(), Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS automation (id TEXT PRIMARY KEY, data TEXT NOT NULL);",
        )
    }

    /// Keeps automation `automation`, replacing what was kept under its ID.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub(crate) fn put_automation(&self, automation: &Value) -> Result<(), Error> {
        self.conn().execute(
            "INSERT INTO automation (id, data) VALUES (?1, ?2)
             ON CONFLICT (id) DO UPDATE SET data = excluded.data",
            rusqlite::params![
                automation["id"].as_str().unwrap_or_default(),
                automation.to_string()
            ],
        )?;
        Ok(())
    }

    /// The automations, oldest first.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub(crate) fn automations(&self) -> Result<Vec<Value>, Error> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT data FROM automation ORDER BY id")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut all = Vec::new();
        for row in rows {
            if let Ok(value) = serde_json::from_str(&row?) {
                all.push(value);
            }
        }
        Ok(all)
    }

    /// Removes automation `id`; whether it was there.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub(crate) fn delete_automation(&self, id: &str) -> Result<bool, Error> {
        Ok(self
            .conn()
            .execute("DELETE FROM automation WHERE id = ?1", [id])?
            > 0)
    }
}

impl Server {
    /// The automations, of project `project` or all.
    ///
    /// # Errors
    ///
    /// When the store fails.
    pub(crate) fn automation_list(&self, project: Option<&str>) -> Result<Vec<Value>, Refusal> {
        Ok(self
            .store
            .automations()?
            .into_iter()
            .filter(|a| project.is_none_or(|p| a["projectID"] == p))
            .collect())
    }

    /// Removes automation `id`.
    ///
    /// # Errors
    ///
    /// When there's no such automation.
    pub(crate) fn delete_automation(&self, id: &str) -> Result<(), Refusal> {
        if self.store.delete_automation(id)? {
            Ok(())
        } else {
            Err(Refusal::NotFound(format!("There's no automation {id}.")))
        }
    }

    /// Makes an automation.
    ///
    /// # Errors
    ///
    /// When automations are off, the trigger or action doesn't read, or the
    /// session's rules would make a command or script wait for approval.
    pub(crate) fn create_automation(&self, new: NewAutomation) -> Result<Value, Refusal> {
        let session = new
            .session
            .as_deref()
            .map(|id| self.session(id))
            .transpose()?;
        let directory = session
            .as_ref()
            .map(|s| s.location.directory.clone())
            .or(new.directory)
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        if !self.folder(&directory).config.automations {
            return Err(Refusal::Bad(
                "Automations are disabled: set \"automations\": true in the config to turn them on."
                    .to_owned(),
            ));
        }
        let (trigger, due) = first_due(&new.trigger).map_err(Refusal::Bad)?;
        let text = |k: &str| {
            new.action
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
        };
        let action = match text("type") {
            "prompt" => {
                let to = new
                    .action
                    .get("to")
                    .cloned()
                    .or_else(|| session.as_ref().map(|s| json!({ "session": s.id })))
                    .ok_or_else(|| Refusal::Bad("A prompt needs a session to go to.".to_owned()))?;
                json!({ "type": "prompt", "prompt": text("prompt"), "to": to })
            }
            "shell" => {
                self.unattended(session.as_ref(), "shell", text("command"))?;
                json!({ "type": "shell", "command": text("command") })
            }
            "code" => {
                self.unattended(session.as_ref(), "execute", "*")?;
                let mut action = json!({ "type": "code", "code": text("code") });
                if let Some(args) = new.action.get("args") {
                    action["args"] = args.clone();
                }
                action
            }
            other => {
                return Err(Refusal::Bad(format!(
                    "\"{other}\" isn't an action: prompt, shell or code."
                )));
            }
        };
        let created = now();
        let project = session.as_ref().map(|s| s.project_id.clone());
        let mut automation = json!({
            "id": ascending_id("aut"), "name": new.name, "directory": directory,
            "trigger": trigger, "tz": trigger["tz"], "action": action, "status": "active",
            "nextDueAt": due, "runCount": 0, "time": { "created": created, "updated": created },
        });
        if let Some(session) = &session {
            automation["sessionID"] = json!(session.id);
        }
        automation["projectID"] = json!(project.unwrap_or_else(|| "global".to_owned()));
        self.store.put_automation(&automation)?;
        Ok(automation)
    }

    /// Refuses `action` on `resource` unless session `session`'s rules
    /// allow it without asking, as it'll run with no one there.
    fn unattended(
        &self,
        session: Option<&crate::store::Session>,
        action: &str,
        resource: &str,
    ) -> Result<(), Refusal> {
        let Some(session) = session else {
            return Err(Refusal::Bad(format!(
                "A {action} automation needs a session whose rules allow it."
            )));
        };
        let folder = self.folder(&session.location.directory);
        let layers = [
            ("config".to_owned(), folder.config.permissions.clone()),
            ("saved".to_owned(), self.saved_rules(&session.project_id)),
            ("session".to_owned(), session.permissions.clone()),
        ];
        let borrowed: Vec<(&str, &[harlo_engine::Rule])> = layers
            .iter()
            .map(|(n, r)| (n.as_str(), r.as_slice()))
            .collect();
        if decide(&borrowed, action, resource).effect == Effect::Allow {
            Ok(())
        } else {
            Err(Refusal::Bad(format!(
                "The session's rules don't let {action} run \"{resource}\" without asking; \
                 allow it first, as it'll run with no one there to approve it."
            )))
        }
    }

    /// Changes automation `id`'s status to `status`; returns it.
    ///
    /// # Errors
    ///
    /// When there's no such automation, or it's done.
    pub(crate) fn set_automation_status(&self, id: &str, status: &str) -> Result<Value, Refusal> {
        let mut automation = self.automation(id)?;
        if automation["status"] == "done" {
            return Err(Refusal::Bad(format!("The automation {id} has finished.")));
        }
        automation["status"] = json!(status);
        automation["time"]["updated"] = json!(now());
        if status == "active"
            && automation["nextDueAt"].as_i64().is_none_or(|d| d < now())
            && let Some(next) = next_due(&automation, now())
        {
            automation["nextDueAt"] = json!(next);
        }
        self.store.put_automation(&automation)?;
        Ok(automation)
    }

    /// Automation `id`.
    ///
    /// # Errors
    ///
    /// When there's no such automation.
    pub(crate) fn automation(&self, id: &str) -> Result<Value, Refusal> {
        self.store
            .automations()?
            .into_iter()
            .find(|a| a["id"] == id)
            .ok_or_else(|| Refusal::NotFound(format!("There's no automation {id}.")))
    }

    /// Marks automation `automation`, due at `due`, as run: counted, and
    /// moved on to its next time or done. Kept before it runs, so a long
    /// run isn't started twice.
    fn mark(&self, mut automation: Value, due: i64) -> Value {
        let runs = automation["runCount"].as_u64().unwrap_or_default();
        automation["runCount"] = json!(runs + 1);
        automation["lastRunAt"] = json!(now());
        if let Some(next) = next_due(&automation, due) {
            automation["nextDueAt"] = json!(next);
        } else {
            automation["status"] = json!("done");
            if let Some(a) = automation.as_object_mut() {
                a.remove("nextDueAt");
            }
        }
        let _ = self.store.put_automation(&automation);
        automation
    }

    /// Runs marked automation `automation`, which was due at `due`, and
    /// keeps how it went.
    async fn fire(self: &Arc<Self>, mut automation: Value, due: i64) {
        let runs = automation["runCount"].as_u64().unwrap_or(1) - 1;
        let late = automation["lastRunAt"].as_i64().unwrap_or_else(now) - due > 60_000;
        let (ok, output) = self.run_action(&mut automation, runs).await;
        automation["lastOutcome"] = json!(match (ok, late) {
            (false, _) => "failed",
            (true, true) => "late",
            (true, false) => "fired",
        });
        automation["lastOutput"] = json!(kept(&output));
        automation["time"]["updated"] = json!(now());
        // A pause, a removal or a status the run set meanwhile is kept.
        if let Ok(kept_now) = self.automation(automation["id"].as_str().unwrap_or_default()) {
            if kept_now["status"] == "paused" {
                automation["status"] = json!("paused");
            }
            let _ = self.store.put_automation(&automation);
        }
    }

    /// Does what `automation` does; returns whether it worked, and what it
    /// printed or where it went.
    async fn run_action(self: &Arc<Self>, automation: &mut Value, runs: u64) -> (bool, String) {
        let directory = automation["directory"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let action = automation["action"].clone();
        match action["type"].as_str() {
            Some("prompt") => {
                let Some(session) = action["to"]["session"].as_str() else {
                    return (false, "It has no session to send to.".to_owned());
                };
                let prompt = action["prompt"].as_str().unwrap_or_default();
                match self.prompt(session, None, prompt, "queue", Vec::new(), Vec::new()) {
                    Ok(_) => (true, format!("Sent to session {session}.")),
                    Err(why) => (false, why.to_string()),
                }
            }
            Some("shell") => {
                let command = action["command"].as_str().unwrap_or_default();
                let ran = execute(
                    command,
                    Path::new(&directory),
                    None,
                    Some(RUN_LIMIT),
                    &CancellationToken::new(),
                )
                .await;
                match ran.ended {
                    Ended::Exited(Some(0)) => (true, ran.output),
                    Ended::Exited(code) => (
                        false,
                        format!("{}\n\nExit code: {}", ran.output, code.unwrap_or(-1)),
                    ),
                    Ended::TimedOut => (false, format!("{}\n\nIt ran out of time.", ran.output)),
                    Ended::Stopped => (false, ran.output),
                    Ended::NotStarted(why) => (false, why),
                }
            }
            Some("code") => self.run_code(automation, runs).await,
            _ => (false, "It has no action.".to_owned()),
        }
    }

    /// Runs `automation`'s Code Mode script; keeps the state it returns,
    /// stops it when it says it's done, and sends what it asks to send.
    async fn run_code(self: &Arc<Self>, automation: &mut Value, runs: u64) -> (bool, String) {
        let session = automation["sessionID"].as_str().map(str::to_owned);
        let directory = session
            .as_deref()
            .and_then(|s| self.store.session(s).ok().flatten())
            .map(|s| s.location.directory);
        let links = match directory {
            Some(directory) => self.mcp_ready(&directory).await,
            None => Vec::new(),
        };
        let catalog = crate::code::ServerCatalog {
            server: Arc::clone(self),
            session,
            links,
        };
        let input = json!({
            "state": automation.get("state").cloned().unwrap_or(Value::Null),
            "now": now(), "runs": runs, "args": automation["action"]["args"].clone(),
        });
        let code = automation["action"]["code"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let ran = harlo_engine::codemode::run(
            &catalog,
            &code,
            &input,
            RUN_LIMIT,
            &CancellationToken::new(),
            &|_| {},
        )
        .await;
        let (text, _) = ran.text();
        let Ok(value) = &ran.value else {
            return (false, text);
        };
        let value = value.clone().unwrap_or(Value::Null);
        if let Some(state) = value.get("state") {
            automation["state"] = state.clone();
        }
        if value["done"] == true {
            automation["status"] = json!("done");
        }
        let mut output = text;
        if let Some(prompt) = value["send"]["prompt"].as_str()
            && let Some(session) = automation["sessionID"].as_str()
        {
            match self.prompt(session, None, prompt, "queue", Vec::new(), Vec::new()) {
                Ok(_) => output = format!("Sent to session {session}.\n\n{output}"),
                Err(why) => return (false, why.to_string()),
            }
        }
        (true, output)
    }

    /// Fires the automations that are due.
    fn tick(self: &Arc<Self>) {
        let Ok(all) = self.store.automations() else {
            return;
        };
        for automation in all {
            let due = automation["nextDueAt"].as_i64();
            if automation["status"] == "active"
                && let Some(due) = due.filter(|d| *d <= now())
            {
                let marked = self.mark(automation, due);
                let server = Arc::clone(self);
                tokio::spawn(async move { server.fire(marked, due).await });
            }
        }
    }

    /// Starts checking once a second for automations that are due.
    pub fn start_automations(self: &Arc<Self>) {
        let server = Arc::clone(self);
        tokio::spawn(async move {
            let mut every = tokio::time::interval(Duration::from_secs(1));
            loop {
                every.tick().await;
                server.tick();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_and_times_read() {
        assert_eq!(duration("90 seconds"), Some(90_000));
        assert_eq!(duration("2 hours"), Some(7_200_000));
        assert_eq!(duration("soon"), None);
        let utc = TimeZone::UTC;
        assert_eq!(instant("2026-10-27T09:00", &utc), Some(1_793_091_600_000));
        assert!(instant("tomorrow-ish", &utc).is_none());
        let due = next_cron("0 9 * * 1-5", &utc, 1_793_091_600_000).expect("next");
        // From Tuesday 09:00, the next weekday 09:00 is Wednesday's.
        assert_eq!(due, 1_793_091_600_000 + 86_400_000);
        next_cron("nope", &utc, 0).unwrap_err();
    }
}
