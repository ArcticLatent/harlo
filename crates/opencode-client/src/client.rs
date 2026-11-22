//! The HTTP client for one OpenCode server.

// Rust guideline compliant 2026-10-09

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{Stream, StreamExt, TryStreamExt};
use reqwest::{Method, RequestBuilder, Response};
use serde_json::{Value, json};
use sse_stream::SseByteStream;

use crate::catalog::{Agent, Command, FileMatch, McpResource, Model, Skill};
use crate::generated as g;
use crate::prompt::Prompt;
use crate::types::{
    Answer, Delivery, FileDiff, Form, InboxItem, Message, ModelRef, Permission, PermissionReply,
    Project, Revert, Rule, ServerInfo, Session, SessionContext,
};
use crate::{Endpoint, Error, Event};

/// The user name OpenCode 2 expects with the server password.
const USER: &str = "opencode";

/// Messages asked for per page when loading a whole session.
///
/// Large enough that most sessions load in one or two requests; OpenCode caps
/// the page size on its side anyway.
const PAGE: &str = "200";

/// A client for one OpenCode server.
///
/// Cheap to clone: clones share one connection pool. It never starts, stops or
/// reconfigures the server; dropping it only closes connections.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    endpoint: Endpoint,
}

impl Client {
    /// Creates a client for `endpoint`.
    #[must_use]
    pub fn new(endpoint: Endpoint) -> Self {
        Self {
            http: reqwest::Client::new(),
            endpoint,
        }
    }

    /// Returns the server this client talks to.
    #[must_use]
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let builder = self
            .http
            .request(method, format!("{}{path}", self.endpoint.url()));
        match self.endpoint.password() {
            Some(password) => builder.basic_auth(USER, Some(password)),
            None => builder,
        }
    }

    async fn send(builder: RequestBuilder) -> Result<Response, Error> {
        let response = builder.send().await?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let body = response.text().await.unwrap_or_default();
        Err(Error::status(status.as_u16(), &body))
    }

    async fn json(builder: RequestBuilder) -> Result<Value, Error> {
        let text = Self::send(builder).await?.text().await?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Returns the server's version and addresses.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached or rejects the password.
    pub async fn info(&self) -> Result<ServerInfo, Error> {
        let v = Self::json(self.request(Method::GET, "/api/info")).await?;
        Ok(serde_json::from_value::<g::ServerInfo>(v)?.into())
    }

    /// Lists sessions, newest first, optionally only those in `directory`.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached or answers with an error.
    pub async fn sessions(&self, directory: Option<&str>) -> Result<Vec<Session>, Error> {
        let mut req = self
            .request(Method::GET, "/api/session")
            .query(&[("order", "desc")]);
        if let Some(dir) = directory {
            req = req.query(&[("directory", dir)]);
        }
        let v = Self::json(req).await?;
        decode_list(&v["data"], |item| {
            Ok(serde_json::from_value::<g::SessionInfo>(item.clone())?.into())
        })
    }

    /// Lists every top-level session that isn't archived, newest first.
    ///
    /// Subagent sessions are left out. Follows the server's pages until the
    /// list ends, so one call returns them all.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached or answers with an error.
    pub async fn root_sessions(&self) -> Result<Vec<Session>, Error> {
        let mut sessions = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut req = self.request(Method::GET, "/api/session").query(&[
                ("parentID", "null"),
                ("archived", "false"),
                ("limit", PAGE),
            ]);
            req = match &cursor {
                Some(c) => req.query(&[("cursor", c.as_str())]),
                None => req.query(&[("order", "desc")]),
            };
            let v = Self::json(req).await?;
            let page = decode_list(&v["data"], |item| {
                Ok(serde_json::from_value::<g::SessionInfo>(item.clone())?.into())
            })?;
            let full = page.len() >= PAGE.parse().unwrap_or(usize::MAX);
            sessions.extend(page);
            cursor = v["cursor"]["next"].as_str().map(str::to_owned);
            if !full || cursor.is_none() {
                return Ok(sessions);
            }
        }
    }

    /// Lists the projects OpenCode knows, archived ones left out.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached or answers with an error.
    pub async fn projects(&self) -> Result<Vec<Project>, Error> {
        let v = Self::json(self.request(Method::GET, "/api/project")).await?;
        decode_list(&v, |item| {
            Ok(serde_json::from_value::<g::Project>(item.clone())?.into())
        })
    }

    /// Lists a project's folders: its main checkout and its worktrees.
    ///
    /// # Errors
    ///
    /// Fails when the project doesn't exist or the server can't be reached.
    pub async fn worktrees(&self, project_id: &str) -> Result<Vec<String>, Error> {
        let req = self
            .request(Method::GET, "/api/worktree")
            .query(&[("projectID", project_id)]);
        let v = Self::json(req).await?;
        decode_list(&v, |item| {
            Ok(
                serde_json::from_value::<g::WorktreeDirectory>(item.clone())?
                    .directory
                    .to_string(),
            )
        })
    }

    /// Returns one session.
    ///
    /// # Errors
    ///
    /// Fails when the session doesn't exist or the server can't be reached.
    pub async fn session(&self, session_id: &str) -> Result<Session, Error> {
        let v =
            Self::json(self.request(Method::GET, &format!("/api/session/{session_id}"))).await?;
        Ok(serde_json::from_value::<g::SessionInfo>(v["data"].clone())?.into())
    }

    /// Creates a session working in `directory`.
    ///
    /// # Errors
    ///
    /// Fails when the server rejects the folder or can't be reached.
    pub async fn create_session(&self, directory: &str) -> Result<Session, Error> {
        let body = json!({ "location": { "directory": directory } });
        let v = Self::json(self.request(Method::POST, "/api/session").json(&body)).await?;
        Ok(serde_json::from_value::<g::SessionInfo>(v["data"].clone())?.into())
    }

    /// Sends a prompt, with its attachments, skills and named subagents.
    ///
    /// Sending a prompt with the same [`Prompt::id`] again stores the message
    /// once, so a retry after a lost answer is safe.
    ///
    /// # Errors
    ///
    /// Fails when the session doesn't exist, an attachment can't be read (an
    /// image that doesn't decode, a missing file), or the server can't be reached.
    pub async fn prompt(&self, session_id: &str, prompt: &Prompt) -> Result<(), Error> {
        self.post(&format!("/api/session/{session_id}/prompt"), &prompt.json())
            .await
    }

    /// Runs a slash command, such as `review`, with the text typed after it.
    ///
    /// # Errors
    ///
    /// Fails when the command doesn't exist or the server can't be reached.
    pub async fn run_command(
        &self,
        session_id: &str,
        name: &str,
        text: &str,
        delivery: Delivery,
    ) -> Result<(), Error> {
        let body = json!({ "name": name, "text": text, "delivery": delivery.as_str() });
        self.post(&format!("/api/session/{session_id}/command"), &body)
            .await
    }

    /// Asks OpenCode to compact the session: summarize it to free up context.
    ///
    /// # Errors
    ///
    /// Fails when the session doesn't exist or the server can't be reached.
    pub async fn compact(&self, session_id: &str) -> Result<(), Error> {
        self.post(&format!("/api/session/{session_id}/compact"), &json!({}))
            .await
    }

    /// Chooses the model, and its reasoning level, for the session's next steps.
    ///
    /// # Errors
    ///
    /// Fails when the model doesn't exist or the server can't be reached.
    pub async fn set_model(&self, session_id: &str, model: &ModelRef) -> Result<(), Error> {
        let mut reference = json!({ "id": model.id, "providerID": model.provider_id });
        if let Some(variant) = &model.variant {
            reference["variant"] = json!(variant);
        }
        self.post(
            &format!("/api/session/{session_id}/model"),
            &json!({ "model": reference }),
        )
        .await
    }

    /// Chooses the agent that runs the session's next steps.
    ///
    /// # Errors
    ///
    /// Fails when the agent doesn't exist or the server can't be reached.
    pub async fn set_agent(&self, session_id: &str, agent: &str) -> Result<(), Error> {
        self.post(
            &format!("/api/session/{session_id}/agent"),
            &json!({ "agent": agent }),
        )
        .await
    }

    /// Replaces the session's own permission rules, which apply after its agent's.
    ///
    /// An empty list leaves the agent's rules alone.
    ///
    /// # Errors
    ///
    /// Fails when the session doesn't exist or the server can't be reached.
    pub async fn set_permissions(&self, session_id: &str, rules: &[Rule]) -> Result<(), Error> {
        let rules: Vec<Value> = rules.iter().map(Rule::json).collect();
        Self::send(
            self.request(Method::PATCH, &format!("/api/session/{session_id}"))
                .json(&json!({ "permissions": rules })),
        )
        .await?;
        Ok(())
    }

    async fn post(&self, path: &str, body: &Value) -> Result<(), Error> {
        Self::send(self.request(Method::POST, path).json(body)).await?;
        Ok(())
    }

    /// Lists the models available in `directory`, enabled ones only.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn models(&self, directory: &str) -> Result<Vec<Model>, Error> {
        let v = self.list("/api/model", directory, &[]).await?;
        Ok(each(&v, Model::from_json))
    }

    /// Returns the model new sessions in `directory` use, if one is configured.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn default_model(&self, directory: &str) -> Result<Option<Model>, Error> {
        let v = self.list("/api/model/default", directory, &[]).await?;
        Ok(Model::from_json(&v))
    }

    /// Lists the agents in `directory`, hidden ones left out.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn agents(&self, directory: &str) -> Result<Vec<Agent>, Error> {
        let v = self.list("/api/agent", directory, &[]).await?;
        Ok(each(&v, Agent::from_json))
    }

    /// Lists the slash commands in `directory`.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn commands(&self, directory: &str) -> Result<Vec<Command>, Error> {
        let v = self.list("/api/command", directory, &[]).await?;
        Ok(each(&v, Command::from_json))
    }

    /// Lists the skills in `directory`.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn skills(&self, directory: &str) -> Result<Vec<Skill>, Error> {
        let v = self.list("/api/skill", directory, &[]).await?;
        Ok(each(&v, Skill::from_json))
    }

    /// Lists the resources MCP servers offer in `directory`.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn mcp_resources(&self, directory: &str) -> Result<Vec<McpResource>, Error> {
        let v = self.list("/api/mcp/resource", directory, &[]).await?;
        Ok(each(&v["resources"], McpResource::from_json))
    }

    /// Finds files and folders in `directory` whose path matches `query`, best first.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn find_files(
        &self,
        directory: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<FileMatch>, Error> {
        let limit = limit.to_string();
        let v = self
            .list(
                "/api/fs/find",
                directory,
                &[("query", query), ("limit", &limit)],
            )
            .await?;
        Ok(each(&v, FileMatch::from_json))
    }

    /// GETs a folder-scoped list and returns its `data`.
    async fn list(
        &self,
        path: &str,
        directory: &str,
        query: &[(&str, &str)],
    ) -> Result<Value, Error> {
        let req = self
            .request(Method::GET, path)
            .query(&[("location[directory]", directory)])
            .query(query);
        let mut v = Self::json(req).await?;
        Ok(v["data"].take())
    }

    /// Returns a session's whole timeline, oldest first.
    ///
    /// # Errors
    ///
    /// Fails when the session doesn't exist, the server can't be reached, or a
    /// message Harlo shows doesn't match the pinned schema.
    pub async fn messages(&self, session_id: &str) -> Result<Vec<Message>, Error> {
        let path = format!("/api/session/{session_id}/message");
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut req = self.request(Method::GET, &path).query(&[("limit", PAGE)]);
            req = match &cursor {
                Some(c) => req.query(&[("cursor", c.as_str())]),
                None => req.query(&[("order", "asc")]),
            };
            let v = Self::json(req).await?;
            let page = v["data"].as_array().cloned().unwrap_or_default();
            if page.is_empty() {
                break;
            }
            for item in page {
                out.push(Message::from_json(item)?);
            }
            match v["cursor"]["next"].as_str() {
                Some(next) if Some(next) != cursor.as_deref() => cursor = Some(next.to_owned()),
                _ => break,
            }
        }
        Ok(out)
    }

    /// Returns the IDs of sessions that are working right now.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn active(&self) -> Result<Vec<String>, Error> {
        let v = Self::json(self.request(Method::GET, "/api/session/active")).await?;
        Ok(v["data"]
            .as_object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default())
    }

    /// Returns a session's pending approvals.
    ///
    /// # Errors
    ///
    /// Fails when the session doesn't exist or the server can't be reached.
    pub async fn permissions(&self, session_id: &str) -> Result<Vec<Permission>, Error> {
        let v = Self::json(self.request(
            Method::GET,
            &format!("/api/session/{session_id}/permission"),
        ))
        .await?;
        decode_list(&v["data"], |item| {
            Ok(serde_json::from_value::<g::PermissionRequest>(item.clone())?.into())
        })
    }

    /// Returns pending approvals for every session working in `directory`.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn permissions_in(&self, directory: &str) -> Result<Vec<Permission>, Error> {
        let req = self
            .request(Method::GET, "/api/permission/request")
            .query(&[("location[directory]", directory)]);
        let v = Self::json(req).await?;
        decode_list(&v["data"], |item| {
            Ok(serde_json::from_value::<g::PermissionRequest>(item.clone())?.into())
        })
    }

    /// Answers an approval request, optionally telling the agent why.
    ///
    /// A `message` with [`PermissionReply::Reject`] tells the agent what to
    /// do instead.
    ///
    /// # Errors
    ///
    /// When another client answered first, the error's
    /// [`is_settled`](Error::is_settled) is true: drop the request, don't retry.
    pub async fn reply_permission(
        &self,
        session_id: &str,
        request_id: &str,
        reply: PermissionReply,
        message: Option<&str>,
    ) -> Result<(), Error> {
        let path = format!("/api/session/{session_id}/permission/{request_id}/reply");
        let mut body = json!({ "decision": reply.as_str() });
        if let Some(message) = message {
            body["message"] = Value::from(message);
        }
        Self::send(self.request(Method::POST, &path).json(&body)).await?;
        Ok(())
    }

    /// Returns a session's pending questions.
    ///
    /// # Errors
    ///
    /// Fails when the session doesn't exist or the server can't be reached.
    pub async fn forms(&self, session_id: &str) -> Result<Vec<Form>, Error> {
        let v = Self::json(self.request(Method::GET, &format!("/api/session/{session_id}/form")))
            .await?;
        Ok(v["data"]
            .as_array()
            .map(|a| a.iter().map(Form::from_json).collect())
            .unwrap_or_default())
    }

    /// Returns pending questions for every session working in `directory`.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn forms_in(&self, directory: &str) -> Result<Vec<Form>, Error> {
        let req = self
            .request(Method::GET, "/api/form")
            .query(&[("location[directory]", directory)]);
        let v = Self::json(req).await?;
        Ok(v["data"]
            .as_array()
            .map(|a| a.iter().map(Form::from_json).collect())
            .unwrap_or_default())
    }

    /// Answers a question with each field's answer, by field key.
    ///
    /// # Errors
    ///
    /// When another client answered first, the error's
    /// [`is_settled`](Error::is_settled) is true: drop the question, don't retry.
    pub async fn reply_form(
        &self,
        session_id: &str,
        form_id: &str,
        answers: &[(String, Answer)],
    ) -> Result<(), Error> {
        let path = format!("/api/session/{session_id}/form/{form_id}/reply");
        let answer: serde_json::Map<String, Value> = answers
            .iter()
            .map(|(key, answer)| (key.clone(), answer.json()))
            .collect();
        Self::send(
            self.request(Method::POST, &path)
                .json(&json!({ "answer": answer })),
        )
        .await?;
        Ok(())
    }

    /// Dismisses a question without answering it.
    ///
    /// # Errors
    ///
    /// When another client answered first, the error's
    /// [`is_settled`](Error::is_settled) is true: drop the question, don't retry.
    pub async fn cancel_form(&self, session_id: &str, form_id: &str) -> Result<(), Error> {
        Self::send(self.request(
            Method::DELETE,
            &format!("/api/session/{session_id}/form/{form_id}"),
        ))
        .await?;
        Ok(())
    }

    /// Returns work waiting in a session's inbox, in delivery order.
    ///
    /// # Errors
    ///
    /// Fails when the session doesn't exist or the server can't be reached.
    pub async fn inbox(&self, session_id: &str) -> Result<Vec<InboxItem>, Error> {
        let v = Self::json(self.request(Method::GET, &format!("/api/session/{session_id}/inbox")))
            .await?;
        Ok(v["data"]
            .as_array()
            .map(|a| a.iter().map(InboxItem::from_json).collect())
            .unwrap_or_default())
    }

    /// Changes how a waiting prompt is delivered: now, into the running turn
    /// ([`Delivery::Steer`]), or after it ([`Delivery::Queue`]).
    ///
    /// # Errors
    ///
    /// Fails when the item was already delivered or the server can't be reached.
    pub async fn set_delivery(
        &self,
        session_id: &str,
        item_id: &str,
        delivery: Delivery,
    ) -> Result<(), Error> {
        Self::send(
            self.request(
                Method::PATCH,
                &format!("/api/session/{session_id}/inbox/{item_id}"),
            )
            .json(&json!({ "delivery": delivery.as_str() })),
        )
        .await?;
        Ok(())
    }

    /// Removes a waiting item from a session's inbox.
    ///
    /// Removing an item that is already gone succeeds.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn cancel_inbox(&self, session_id: &str, item_id: &str) -> Result<(), Error> {
        Self::send(self.request(
            Method::DELETE,
            &format!("/api/session/{session_id}/inbox/{item_id}"),
        ))
        .await?;
        Ok(())
    }

    /// Returns the files the session's agent changed, as diffs.
    ///
    /// # Errors
    ///
    /// Fails when the session doesn't exist or the server can't be reached.
    pub async fn diff(&self, session_id: &str) -> Result<Vec<FileDiff>, Error> {
        let v = Self::json(self.request(Method::GET, &format!("/api/session/{session_id}/diff")))
            .await?;
        decode_list(&v["data"], |item| {
            Ok(serde_json::from_value::<g::FileDiffInfo>(item.clone())?.into())
        })
    }

    /// Returns what the session's model sees now, since its last compaction.
    ///
    /// # Errors
    ///
    /// Fails when the session doesn't exist or the server can't be reached.
    pub async fn context(&self, session_id: &str) -> Result<SessionContext, Error> {
        let v =
            Self::json(self.request(Method::GET, &format!("/api/session/{session_id}/context")))
                .await?;
        let messages = v["data"].as_array().map(Vec::as_slice).unwrap_or_default();
        Ok(SessionContext::from_json(messages))
    }

    /// Returns the uncommitted changes in `directory`'s repository, as diffs.
    ///
    /// Each file's patch compares it with the last commit, so staged and
    /// unstaged changes show together. Untracked files count as added.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached or `directory` has no repository.
    pub async fn working_changes(&self, directory: &str) -> Result<Vec<FileDiff>, Error> {
        let v = self
            .list(
                "/api/vcs/diff",
                directory,
                &[("mode", "working"), ("untracked", "files")],
            )
            .await?;
        decode_list(&v, |item| {
            Ok(serde_json::from_value::<g::FileDiffInfo>(item.clone())?.into())
        })
    }

    /// Returns the files the agent changed in `from`'s turn through `to`'s.
    ///
    /// Both are user messages; without `to`, only `from`'s turn counts. The
    /// patches compare the agent's own snapshots, so edits the user made to
    /// the files since don't show.
    ///
    /// # Errors
    ///
    /// Fails when a message isn't in the session, `to` comes before `from`,
    /// or the server can't be reached.
    pub async fn diff_since(
        &self,
        session_id: &str,
        from: &str,
        to: Option<&str>,
    ) -> Result<Vec<FileDiff>, Error> {
        let mut req = self
            .request(Method::GET, &format!("/api/session/{session_id}/diff"))
            .query(&[("from", from)]);
        if let Some(to) = to {
            req = req.query(&[("to", to)]);
        }
        let v = Self::json(req).await?;
        decode_list(&v["data"], |item| {
            Ok(serde_json::from_value::<g::FileDiffInfo>(item.clone())?.into())
        })
    }

    /// Stages an undo back to before `message_id`.
    ///
    /// With `files`, the agent's file changes are restored right away and the
    /// result lists exactly what changed on disk, including any of the user's
    /// own edits to those files that the restore removed. Until it is kept
    /// ([`Client::commit_revert`]), [`Client::cancel_revert`] puts every file
    /// back as it was. Without `files`, nothing on disk changes and the result
    /// lists no files.
    ///
    /// # Errors
    ///
    /// Fails when the message isn't in the session or the server can't be reached.
    pub async fn stage_revert(
        &self,
        session_id: &str,
        message_id: &str,
        files: bool,
    ) -> Result<Revert, Error> {
        let body = json!({ "messageID": message_id, "files": files });
        let v = Self::json(
            self.request(
                Method::POST,
                &format!("/api/session/{session_id}/revert/stage"),
            )
            .json(&body),
        )
        .await?;
        Ok(serde_json::from_value::<g::SessionRevert>(v["data"].clone())?.into())
    }

    /// Keeps a staged undo: the messages after the boundary are removed for good.
    ///
    /// # Errors
    ///
    /// Fails when nothing is staged or the server can't be reached.
    pub async fn commit_revert(&self, session_id: &str) -> Result<(), Error> {
        Self::send(
            self.request(
                Method::POST,
                &format!("/api/session/{session_id}/revert/commit"),
            )
            .json(&json!({})),
        )
        .await?;
        Ok(())
    }

    /// Cancels a staged undo, restoring the conversation and files.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn cancel_revert(&self, session_id: &str) -> Result<(), Error> {
        Self::send(self.request(Method::DELETE, &format!("/api/session/{session_id}/revert")))
            .await?;
        Ok(())
    }

    /// Stops the session's current run.
    ///
    /// # Errors
    ///
    /// Fails when the session doesn't exist or the server can't be reached.
    pub async fn interrupt(&self, session_id: &str) -> Result<(), Error> {
        Self::send(self.request(
            Method::POST,
            &format!("/api/session/{session_id}/interrupt"),
        ))
        .await?;
        Ok(())
    }

    /// Opens the live event stream.
    ///
    /// The stream ends when the connection drops. Read it promptly, on a task
    /// of its own: OpenCode fails the stream for a slow reader. After it ends,
    /// open a new one and reload state, since events in between are lost.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached or rejects the password. Items
    /// fail when the connection breaks or an event can't be decoded.
    pub async fn events(&self) -> Result<Events, Error> {
        let response = Self::send(self.request(Method::GET, "/api/event")).await?;
        let bytes = response.bytes_stream().map_err(Error::from);
        let stream = SseByteStream::new(bytes).filter_map(|item| {
            std::future::ready(match item {
                Ok(sse) => sse
                    .data
                    .map(|data| Event::parse(&data).map_err(Error::from)),
                Err(e) => Some(Err(Error::other(format!("event stream ended: {e}")))),
            })
        });
        Ok(Events {
            inner: Box::pin(stream),
        })
    }
}

/// A live event stream from one server; see [`Client::events`].
///
/// Dropping it closes the connection.
pub struct Events {
    inner: Pin<Box<dyn Stream<Item = Result<Event, Error>> + Send>>,
}

impl Events {
    /// Waits for the next event; `None` when the connection closed.
    pub async fn next(&mut self) -> Option<Result<Event, Error>> {
        self.inner.next().await
    }
}

impl Stream for Events {
    type Item = Result<Event, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

impl std::fmt::Debug for Events {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Events").finish_non_exhaustive()
    }
}

/// Decodes each item of a JSON array, skipping items `decode` rejects.
fn each<T>(list: &Value, decode: impl Fn(&Value) -> Option<T>) -> Vec<T> {
    list.as_array()
        .map(|items| items.iter().filter_map(decode).collect())
        .unwrap_or_default()
}

/// Decodes each item of a JSON array, skipping kinds the client doesn't know.
fn decode_list<T>(
    list: &Value,
    decode: impl Fn(&Value) -> Result<T, Error>,
) -> Result<Vec<T>, Error> {
    let Some(items) = list.as_array() else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(decode(item)?);
    }
    Ok(out)
}
