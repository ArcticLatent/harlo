//! The tools Code Mode scripts can call on this server, and the progress
//! clients see while a script runs.

// Rust guideline compliant 2026-10-09

use std::sync::Arc;

use harlo_engine::codemode::{Answer, Call, Catalog, Direct, Namespace, ToolInfo};
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
    /// The MCP servers its scripts reach.
    pub(crate) links: Vec<Arc<crate::mcp::Link>>,
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

impl ServerCatalog {
    /// Whether `namespace` is a plugin's to give: the server's own and an
    /// MCP server's of the same name come first.
    fn plugins(&self, namespace: &str) -> bool {
        !["automation", "project"].contains(&namespace)
            && !self.links.iter().any(|link| link.name == namespace)
    }

    /// Runs tool `tool` of the plugin whose namespace is `namespace`, which
    /// may call the session's own tools through `tools`.
    async fn plugin(
        &self,
        namespace: &str,
        tool: &str,
        args: Value,
        tools: Option<&dyn harlo_engine::codemode::SessionTools>,
    ) -> Result<Value, String> {
        let program = self
            .server
            .plugin_program(namespace)
            .filter(|_| self.plugins(namespace))
            .ok_or_else(|| format!("There's no namespace {namespace}."))?;
        let session = self
            .session
            .as_deref()
            .and_then(|id| self.server.session(id).ok());
        let session = session
            .as_ref()
            .map(|s| (s.id.as_str(), s.location.directory.as_str()));
        let within = crate::plugins::code::Within { session, tools };
        program
            .call(
                &self.server,
                crate::plugins::code::Kind::Tool,
                tool,
                args,
                within,
            )
            .await
    }

    /// Runs `tool` of `namespace`, which may call the session's own tools
    /// through `tools`.
    async fn called(
        &self,
        namespace: &str,
        tool: &str,
        args: Value,
        tools: Option<&dyn harlo_engine::codemode::SessionTools>,
    ) -> Result<Value, String> {
        match namespace {
            "automation" => self.automation(tool, args),
            other => {
                let session = self.session.as_deref();
                match crate::mcp::call(&self.links, other, tool, args.clone(), session).await {
                    Some(answer) => answer,
                    None => self.plugin(other, tool, args, tools).await,
                }
            }
        }
    }
}

impl Catalog for ServerCatalog {
    fn namespaces(&self) -> Vec<Namespace> {
        let mut all = vec![Namespace {
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
        }];
        all.extend(crate::mcp::namespaces(&self.links));
        all.extend(
            self.server
                .plugin_programs()
                .iter()
                .filter(|program| self.plugins(&program.namespace))
                .map(|program| program.listed()),
        );
        all
    }

    fn direct(&self) -> Vec<Direct> {
        crate::mcp::direct(&self.links)
    }

    fn call<'a>(&'a self, namespace: &'a str, tool: &'a str, args: Value) -> Answer<'a> {
        Box::pin(self.called(namespace, tool, args, None))
    }

    fn reaches_session(&self) -> bool {
        self.server
            .plugin_programs()
            .iter()
            .any(|program| program.reaches_session() && self.plugins(&program.namespace))
    }

    fn call_within<'a>(
        &'a self,
        namespace: &'a str,
        tool: &'a str,
        args: Value,
        session: &'a dyn harlo_engine::codemode::SessionTools,
    ) -> Answer<'a> {
        Box::pin(self.called(namespace, tool, args, Some(session)))
    }

    fn call_seen<'a>(
        &'a self,
        namespace: &'a str,
        tool: &'a str,
        args: Value,
    ) -> harlo_engine::codemode::Seen<'a> {
        Box::pin(async move {
            match namespace {
                "automation" => Ok((self.automation(tool, args)?, Vec::new())),
                other => {
                    let session = self.session.as_deref();
                    let seen =
                        crate::mcp::call_seen(&self.links, other, tool, args.clone(), session)
                            .await;
                    match seen {
                        Some(answer) => answer,
                        None => Ok((self.plugin(other, tool, args, None).await?, Vec::new())),
                    }
                }
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
