//! The HTTP client for one OpenCode server.

// Rust guideline compliant 2026-10-09

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{Stream, StreamExt, TryStreamExt};
use reqwest::{Method, RequestBuilder, Response};
use serde_json::{Value, json};
use sse_stream::SseByteStream;

use crate::generated as g;
use crate::types::{
    Delivery, FileDiff, Form, InboxItem, Message, Permission, PermissionReply, Revert, ServerInfo,
    Session,
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

    /// Sends a prompt with a client-made message ID (see [`crate::message_id`]).
    ///
    /// Sending the same `message_id` again stores the message once, so a
    /// retry after a lost answer is safe.
    ///
    /// # Errors
    ///
    /// Fails when the session doesn't exist or the server can't be reached.
    pub async fn prompt(
        &self,
        session_id: &str,
        message_id: &str,
        text: &str,
        delivery: Delivery,
    ) -> Result<(), Error> {
        let body = json!({ "id": message_id, "text": text, "delivery": delivery.as_str() });
        Self::send(
            self.request(Method::POST, &format!("/api/session/{session_id}/prompt"))
                .json(&body),
        )
        .await?;
        Ok(())
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

    /// Answers an approval request.
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
    ) -> Result<(), Error> {
        let path = format!("/api/session/{session_id}/permission/{request_id}/reply");
        Self::send(
            self.request(Method::POST, &path)
                .json(&json!({ "decision": reply.as_str() })),
        )
        .await?;
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

    /// Answers a question; `answer` maps each field key to its value.
    ///
    /// # Errors
    ///
    /// When another client answered first, the error's
    /// [`is_settled`](Error::is_settled) is true: drop the question, don't retry.
    pub async fn reply_form(
        &self,
        session_id: &str,
        form_id: &str,
        answer: Value,
    ) -> Result<(), Error> {
        let path = format!("/api/session/{session_id}/form/{form_id}/reply");
        Self::send(
            self.request(Method::POST, &path)
                .json(&json!({ "answer": answer })),
        )
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
