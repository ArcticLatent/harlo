//! Terminals that run on the server: OpenCode's PTY sessions.
//!
//! A PTY is a shell the server runs in a folder. Its output streams over a
//! WebSocket, which also takes the user's input; its size is set with a
//! separate request. Closing the socket leaves the shell running until it's
//! removed.

// Rust guideline compliant 2026-10-09

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use reqwest::Method;
use serde_json::json;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::Error;
use crate::client::Client;
use crate::generated as g;

/// The first byte of a frame the server sends about the stream itself rather
/// than the shell's output, such as where the output resumes.
const CONTROL: u8 = 0;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A shell running on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pty {
    /// The server's ID for it.
    pub id: String,
    /// Its title.
    pub title: String,
    /// The folder it runs in.
    pub cwd: String,
}

impl From<g::Pty> for Pty {
    fn from(v: g::Pty) -> Self {
        Self {
            id: v.id.to_string(),
            title: v.title,
            cwd: v.cwd,
        }
    }
}

/// The shell's output, as it arrives.
#[derive(Debug)]
pub struct PtyOutput {
    socket: SplitStream<Socket>,
}

impl PtyOutput {
    /// The next piece of output; `None` once the connection has closed or
    /// the shell has exited.
    ///
    /// # Errors
    ///
    /// Fails when the connection breaks.
    pub async fn next(&mut self) -> Option<Result<Vec<u8>, Error>> {
        loop {
            match self.socket.next().await? {
                Ok(Message::Text(text)) => return Some(Ok(text.as_bytes().to_vec())),
                Ok(Message::Binary(bytes)) if bytes.first() == Some(&CONTROL) => {}
                Ok(Message::Binary(bytes)) => return Some(Ok(bytes.to_vec())),
                Ok(Message::Close(_)) => return None,
                Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => {}
                Err(error) => return Some(Err(Error::socket(error))),
            }
        }
    }
}

/// Where the user's input goes.
#[derive(Debug)]
pub struct PtyInput {
    socket: SplitSink<Socket, Message>,
}

impl PtyInput {
    /// Sends what the user typed or pasted to the shell.
    ///
    /// # Errors
    ///
    /// Fails when the connection has closed.
    pub async fn send(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.socket
            .send(Message::Binary(bytes.to_vec().into()))
            .await
            .map_err(Error::socket)
    }

    /// Closes the connection, leaving the shell running.
    ///
    /// # Errors
    ///
    /// Fails when the connection already broke.
    pub async fn close(&mut self) -> Result<(), Error> {
        self.socket.close().await.map_err(Error::socket)
    }
}

impl Client {
    /// Starts a shell in `cwd` on the server, in `directory`'s project.
    ///
    /// The server picks the user's shell when `command` is `None`.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached or can't start it.
    pub async fn create_pty(
        &self,
        directory: &str,
        cwd: &str,
        command: Option<&str>,
    ) -> Result<Pty, Error> {
        let mut body = json!({ "cwd": cwd });
        if let Some(command) = command {
            body["command"] = json!(command);
        }
        let req = self
            .request(Method::POST, "/api/pty")
            .query(&[("location[directory]", directory)])
            .json(&body);
        let mut v = Self::json(req).await?;
        Ok(serde_json::from_value::<g::Pty>(v["data"].take())?.into())
    }

    /// Tells a shell the terminal's size, in cells.
    ///
    /// # Errors
    ///
    /// Fails when the shell is gone or the server can't be reached.
    pub async fn resize_pty(
        &self,
        directory: &str,
        id: &str,
        columns: u16,
        lines: u16,
    ) -> Result<(), Error> {
        let req = self
            .request(Method::PUT, &format!("/api/pty/{id}"))
            .query(&[("location[directory]", directory)])
            .json(&json!({ "size": { "cols": columns, "rows": lines } }));
        Self::send(req).await.map(drop)
    }

    /// Ends a shell and forgets it.
    ///
    /// # Errors
    ///
    /// Fails when it's already gone or the server can't be reached.
    pub async fn remove_pty(&self, directory: &str, id: &str) -> Result<(), Error> {
        let req = self
            .request(Method::DELETE, &format!("/api/pty/{id}"))
            .query(&[("location[directory]", directory)]);
        Self::send(req).await.map(drop)
    }

    /// Connects to a shell's output and input. The output starts with what
    /// the shell printed before.
    ///
    /// # Errors
    ///
    /// Fails when the shell is gone, the password is wrong or the server
    /// can't be reached.
    pub async fn connect_pty(
        &self,
        directory: &str,
        id: &str,
    ) -> Result<(PtyOutput, PtyInput), Error> {
        let socket = self
            .open_socket(
                &format!("/api/pty/{id}/connect"),
                &[("location[directory]", directory)],
            )
            .await?;
        let (sink, stream) = socket.split();
        Ok((PtyOutput { socket: stream }, PtyInput { socket: sink }))
    }
}

impl Client {
    /// Opens a WebSocket to `path` on the server, with `params` and the
    /// server's password.
    pub(crate) async fn open_socket(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, Error> {
        let base = self.endpoint().url();
        let mut url = reqwest::Url::parse_with_params(&format!("{base}{path}"), params)
            .map_err(|e| Error::other(format!("not a server address: {e}")))?;
        let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
        url.set_scheme(scheme)
            .map_err(|()| Error::other("can't make a WebSocket address"))?;
        let mut request = url.as_str().into_client_request().map_err(Error::socket)?;
        if let Some(auth) = self.basic_auth() {
            let value = HeaderValue::from_str(&auth)
                .map_err(|e| Error::other(format!("unusable password: {e}")))?;
            request.headers_mut().insert("Authorization", value);
        }
        let (socket, _) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(Error::socket)?;
        Ok(socket)
    }
}

impl Error {
    pub(crate) fn socket(error: tungstenite::Error) -> Self {
        match error {
            tungstenite::Error::Http(response) => {
                let body = response
                    .body()
                    .as_deref()
                    .map(String::from_utf8_lossy)
                    .unwrap_or_default();
                Self::status(response.status().as_u16(), &body)
            }
            other => Self::other(format!("the terminal's connection failed: {other}")),
        }
    }
}
