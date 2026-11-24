//! Tools a client offers OpenCode's agents: an MCP server the client hosts
//! over a WebSocket the OpenCode server opens to it, with the tools
//! available to every session on the server while the socket stays open.
//!
//! OpenCode shows them to agents in its Code Mode catalog, under the name
//! the client attaches with, such as `tools.harlo.past_sessions`.

// Rust guideline compliant 2026-10-09

use std::pin::Pin;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorData,
    Implementation, JsonObject, ListToolsResult, PaginatedRequestParams, ServerCapabilities,
    ServerConfig, Tool,
};
use rmcp::service::{RequestContext, RoleServer, RxJsonRpcMessage, TxJsonRpcMessage};
use rmcp::{ServerHandler, ServiceExt};
use serde_json::Value;
use tokio_tungstenite::tungstenite::{self, Message};

use crate::Error;
use crate::client::Client;

/// The `_meta` key OpenCode sends the calling session's ID under.
const SESSION_KEY: &str = "ai.opencode/sessionID";

/// One tool a client offers.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    /// Its name, as agents call it after the client's name.
    pub name: String,
    /// What it does and when to use it, for the model.
    pub description: String,
    /// The JSON Schema of its input: an object.
    pub input_schema: Value,
}

/// What a tool call returns: its text, or why it failed.
pub type ToolOutcome = Result<String, String>;

/// A tool call under way.
pub type ToolRun = Pin<Box<dyn Future<Output = ToolOutcome> + Send>>;

/// The tools a client offers and how it runs them.
pub trait ToolHost: Send + Sync + 'static {
    /// The tools, as agents see them.
    fn tools(&self) -> Vec<ToolSpec>;

    /// Runs the tool named `name` with `input`, for the session with ID
    /// `session` when OpenCode says which one called.
    fn call(&self, name: String, input: Value, session: Option<String>) -> ToolRun;
}

/// The MCP side of a [`ToolHost`].
struct Handler {
    /// The name the tools are offered under: OpenCode names them after the
    /// server's name, so it's the same.
    name: String,
    /// The tools.
    host: Arc<dyn ToolHost>,
}

impl ServerHandler for Handler {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info = Implementation::new(&self.name, env!("CARGO_PKG_VERSION"));
        info
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        let tools = self
            .host
            .tools()
            .into_iter()
            .map(|spec| {
                let schema = match spec.input_schema {
                    Value::Object(map) => map,
                    _ => JsonObject::new(),
                };
                Tool::new(spec.name, spec.description, Arc::new(schema))
            })
            .collect();
        std::future::ready(Ok(ListToolsResult::with_all_items(tools)))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let session = context
            .meta
            .get(SESSION_KEY)
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let input = request.arguments.map_or(Value::Null, Value::Object);
        let result = match self
            .host
            .call(request.name.into_owned(), input, session)
            .await
        {
            Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
            Err(why) => CallToolResult::error(vec![ContentBlock::text(why)]),
        };
        Ok(result.into())
    }
}

impl Client {
    /// Offers `host`'s tools to OpenCode's agents under `name` until the
    /// connection ends.
    ///
    /// The tools are available to every session on the server while this
    /// runs; they go away when it returns or is dropped.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached, rejects the password, or the
    /// connection breaks.
    pub async fn host_tools(
        &self,
        directory: &str,
        name: &str,
        host: Arc<dyn ToolHost>,
    ) -> Result<(), Error> {
        let socket = self
            .open_socket(
                "/api/experimental/client-mcp",
                &[("location[directory]", directory), ("name", name)],
            )
            .await?;
        let (sink, stream) = socket.split();
        // One JSON-RPC message per text frame each way.
        let sink = sink.with(|message: TxJsonRpcMessage<RoleServer>| async move {
            serde_json::to_string(&message)
                .map(|text| Message::Text(text.into()))
                .map_err(|e| tungstenite::Error::Io(std::io::Error::other(e)))
        });
        let stream = stream.filter_map(|frame| async move {
            match frame {
                Ok(Message::Text(text)) => {
                    serde_json::from_str::<RxJsonRpcMessage<RoleServer>>(&text).ok()
                }
                _ => None,
            }
        });
        let handler = Handler {
            name: name.to_owned(),
            host,
        };
        let service = handler
            .serve((Box::pin(sink), Box::pin(stream)))
            .await
            .map_err(|e| Error::other(format!("couldn't offer the tools: {e}")))?;
        service
            .waiting()
            .await
            .map(drop)
            .map_err(|e| Error::other(format!("the tools stopped: {e}")))
    }
}
