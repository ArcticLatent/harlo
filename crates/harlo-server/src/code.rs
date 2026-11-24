//! The tools Code Mode scripts can call on this server, and the progress
//! clients see while a script runs.

// Rust guideline compliant 2026-10-09

use std::sync::Arc;

use harlo_engine::codemode::{Answer, Call, Catalog, Namespace, ToolInfo};
use harlo_engine::now;
use serde_json::{Value, json};

use crate::server::Server;

/// The tools a session's (or an automation's) scripts can call.
#[derive(Debug)]
pub(crate) struct ServerCatalog {
    /// The server.
    pub(crate) server: Arc<Server>,
    /// The session the script runs for, if any.
    pub(crate) session: Option<String>,
}

/// A tool of the `automation` namespace.
fn tool(name: &str, description: &str, signature: &str) -> ToolInfo {
    ToolInfo {
        name: name.to_owned(),
        description: description.to_owned(),
        signature: signature.to_owned(),
    }
}

impl ServerCatalog {
    /// Runs automation tool `tool` with `args`.
    fn automation(&self, tool: &str, mut args: Value) -> Result<Value, String> {
        let server = &self.server;
        let id = || args["id"].as_str().unwrap_or_default().to_owned();
        match tool {
            "list" => {
                let all = server.automation_list(None).map_err(|e| e.to_string())?;
                Ok(json!({ "now": now(), "automations": all }))
            }
            "create" => {
                if args.get("sessionID").is_none()
                    && let Some(session) = &self.session
                {
                    args["sessionID"] = json!(session);
                }
                let new = serde_json::from_value(args).map_err(|e| {
                    format!("The automation doesn't read: {e}. Give name, trigger and action.")
                })?;
                server.create_automation(new).map_err(|e| e.to_string())
            }
            "pause" => server
                .set_automation_status(&id(), "paused")
                .map_err(|e| e.to_string()),
            "resume" => server
                .set_automation_status(&id(), "active")
                .map_err(|e| e.to_string()),
            "delete" => server
                .delete_automation(&id())
                .map(|()| json!({ "deleted": id() }))
                .map_err(|e| e.to_string()),
            other => Err(format!("There's no tool automation.{other}.")),
        }
    }
}

impl Catalog for ServerCatalog {
    fn namespaces(&self) -> Vec<Namespace> {
        vec![Namespace {
            name: "automation".to_owned(),
            description: "Prompts, commands and scripts this server runs on a schedule.".to_owned(),
            tools: vec![
                tool(
                    "list",
                    "Lists the automations and the server's time.",
                    "({}): {now: number, automations: Automation[]}",
                ),
                tool(
                    "create",
                    "Makes an automation. trigger is {type:\"at\", at:\"in 10 minutes\"}, \
                     {type:\"every\", every:\"1 hour\"} or {type:\"cron\", cron:\"0 9 * * 1-5\"}, \
                     with an optional tz; action is {type:\"prompt\", prompt}, \
                     {type:\"shell\", command} or {type:\"code\", code, args?}.",
                    "({name: string, trigger: object, action: object}): Automation",
                ),
                tool(
                    "pause",
                    "Pauses an automation.",
                    "({id: string}): Automation",
                ),
                tool(
                    "resume",
                    "Resumes a paused automation.",
                    "({id: string}): Automation",
                ),
                tool(
                    "delete",
                    "Removes an automation.",
                    "({id: string}): {deleted: string}",
                ),
            ],
        }]
    }

    fn call<'a>(&'a self, namespace: &'a str, tool: &'a str, args: Value) -> Answer<'a> {
        Box::pin(async move {
            match namespace {
                "automation" => self.automation(tool, args),
                other => Err(format!("There's no namespace {other}.")),
            }
        })
    }

    fn progress(&self, message: &str, call: &str, calls: &[Call]) {
        let Some(session) = &self.session else {
            return;
        };
        self.server.emit(
            "session.tool.progress",
            json!({
                "sessionID": session, "assistantMessageID": message, "id": call,
                "metadata": { "toolCalls": calls },
            }),
            None,
            None,
        );
    }
}
