//! Signing in to remote MCP servers with OAuth.
//!
//! Each remote server is an integration (`mcp_` and a hash of its URL)
//! with one browser sign-in method, unless its entry says `"oauth": false`.
//! Signing in finds the server's authorization server from its metadata,
//! registers Harlo there when no client ID is configured, and sends the
//! browser to it; the browser comes back to a loopback address
//! (`http://127.0.0.1:19876/mcp/oauth/callback` unless the entry's `oauth`
//! says another). The tokens are kept as the integration's credential, and
//! refreshed when they run out.

// Rust guideline compliant 2026-10-09

use std::sync::{Arc, Weak};

use rmcp::transport::auth::{
    AuthError, AuthorizationManager, AuthorizationRequest, AuthorizationSession, CredentialStore,
    StoredCredentials,
};
use serde_json::{Value, json};
use sha2::Digest as _;

use crate::oauth::{Return, wait};
use crate::server::{Refusal, Server};

/// What every MCP server's integration ID starts with.
pub(crate) const PREFIX: &str = "mcp_";

/// The path the browser comes back to, unless an entry says another.
const CALLBACK: &str = "/mcp/oauth/callback";

/// The port the browser comes back to, unless an entry says another.
const PORT: u16 = 19876;

/// The integration that signs in to remote server entry `spec`, unless it
/// isn't remote or its sign-in is turned off.
pub(crate) fn integration_id(spec: &Value) -> Option<String> {
    let url = spec["url"].as_str().filter(|u| !u.is_empty())?;
    if spec["type"] != "remote" || spec["oauth"] == false {
        return None;
    }
    let hash = hex::encode(sha2::Sha256::digest(url.as_bytes()));
    Some(format!("{PREFIX}{}", &hash[..16]))
}

/// Where the browser comes back to for entry `spec`.
fn redirect_uri(spec: &Value) -> String {
    let oauth = &spec["oauth"];
    if let Some(uri) = oauth["redirectUri"].as_str().filter(|u| !u.is_empty()) {
        return uri.to_owned();
    }
    let port = oauth["callbackPort"]
        .as_u64()
        .and_then(|p| u16::try_from(p).ok())
        .unwrap_or(PORT);
    format!("http://127.0.0.1:{port}{CALLBACK}")
}

/// Whether `error` means the server wants a sign-in.
pub(crate) fn wants_sign_in(error: &str) -> bool {
    let error = error.to_lowercase();
    [
        "401",
        "unauthorized",
        "auth required",
        "authorization required",
        "insufficient scope",
    ]
    .iter()
    .any(|w| error.contains(w))
}

/// A remote server's tokens, kept as its integration's credential.
pub(crate) struct Kept {
    server: Weak<Server>,
    integration: String,
    label: String,
}

impl Kept {
    /// The tokens for `integration`, named `label`, in `server`'s store.
    pub(crate) fn new(server: Weak<Server>, integration: &str, label: &str) -> Self {
        Self {
            server,
            integration: integration.to_owned(),
            label: label.to_owned(),
        }
    }

    fn server(&self) -> Result<Arc<Server>, AuthError> {
        self.server
            .upgrade()
            .ok_or_else(|| AuthError::CredentialStoreError("The server is stopping.".to_owned()))
    }
}

/// `e` as a credential-store failure.
fn store_error(e: &dyn std::fmt::Display) -> AuthError {
    AuthError::CredentialStoreError(e.to_string())
}

#[async_trait::async_trait]
impl CredentialStore for Kept {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        let server = self.server()?;
        let credentials = server.store.credentials().map_err(|e| store_error(&e))?;
        Ok(credentials
            .iter()
            .find(|c| c["integrationID"] == self.integration.as_str())
            .and_then(|c| serde_json::from_value(c["value"]["tokens"].clone()).ok()))
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        let server = self.server()?;
        let tokens = serde_json::to_value(&credentials).map_err(|e| store_error(&e))?;
        let value = json!({ "type": "oauth", "methodID": self.integration, "tokens": tokens });
        server
            .store
            .put_credential(&self.integration, &self.label, &value)
            .map_err(|e| store_error(&e))?;
        server.integrations_changed();
        Ok(())
    }

    async fn clear(&self) -> Result<(), AuthError> {
        let server = self.server()?;
        let credentials = server.store.credentials().map_err(|e| store_error(&e))?;
        for c in credentials
            .iter()
            .filter(|c| c["integrationID"] == self.integration.as_str())
        {
            server
                .store
                .delete_credential(c["id"].as_str().unwrap_or_default())
                .map_err(|e| store_error(&e))?;
        }
        server.integrations_changed();
        Ok(())
    }
}

/// A sign-in method's or credential's failure, as people read it.
fn said(e: &AuthError) -> String {
    format!("The MCP server's sign-in failed: {e}")
}

impl Server {
    /// The remote MCP servers known here: the global config's, and those
    /// added while running or named by a folder's config.
    fn remote_mcp(&self) -> Vec<(String, Value)> {
        let (global, _) = self.global();
        let mut found: Vec<(String, Value)> = global
            .mcp
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        found.extend(self.mcp_specs());
        let mut seen = std::collections::HashSet::new();
        found.retain(|(_, spec)| integration_id(spec).is_some_and(|id| seen.insert(id)));
        found
    }

    /// The integrations of the remote MCP servers known here, with
    /// `connections` for each.
    pub(crate) fn mcp_integrations(&self, connections: impl Fn(&str) -> Vec<Value>) -> Vec<Value> {
        self.remote_mcp()
            .into_iter()
            .filter_map(|(name, spec)| {
                let id = integration_id(&spec)?;
                Some(json!({
                    "id": id, "name": name, "metadata": { "source": "mcp" },
                    "methods": [{ "id": id, "type": "oauth", "label": name }],
                    "connections": connections(&id),
                }))
            })
            .collect()
    }

    /// Whether integration `id` has a credential.
    pub(crate) fn has_credential(&self, id: &str) -> bool {
        self.store
            .credentials()
            .unwrap_or_default()
            .iter()
            .any(|c| c["integrationID"] == id)
    }

    /// Starts signing in to the remote MCP server integration `id` is for.
    ///
    /// # Errors
    ///
    /// When no remote server has that integration, the port the browser
    /// comes back to is taken, or the server's authorization server can't
    /// be found or won't register Harlo.
    pub(crate) async fn begin_mcp_oauth(self: &Arc<Self>, id: &str) -> Result<Value, Refusal> {
        let (name, spec) = self
            .remote_mcp()
            .into_iter()
            .find(|(_, spec)| integration_id(spec).as_deref() == Some(id))
            .ok_or_else(|| Refusal::NotFound(format!("Integration not found: {id}")))?;
        let redirect = redirect_uri(&spec);
        let back_to = reqwest::Url::parse(&redirect)
            .map_err(|e| Refusal::Bad(format!("The redirect address {redirect} is wrong: {e}")))?;
        let port = back_to.port_or_known_default().unwrap_or(PORT);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .map_err(|e| {
                Refusal::Bad(format!(
                    "Port {port} is in use ({e}): another sign-in is probably waiting there. \
                     Finish or cancel it and try again."
                ))
            })?;
        let url = spec["url"].as_str().unwrap_or_default();
        let mut manager = AuthorizationManager::new(url)
            .await
            .map_err(|e| Refusal::Bad(said(&e)))?;
        manager.set_credential_store(Kept::new(Arc::downgrade(self), id, &name));
        let found = manager
            .resolve_metadata()
            .await
            .map_err(|e| Refusal::Bad(said(&e)))?;
        manager.set_metadata(found.metadata);
        let oauth = &spec["oauth"];
        let mut request = AuthorizationRequest::new(&redirect).with_client_name("Harlo");
        if let Some(client) = oauth["clientId"].as_str() {
            request = request.with_preregistered_client(client);
            if let Some(secret) = oauth["clientSecret"].as_str() {
                request = request.with_client_secret(secret);
            }
        }
        if let Some(scope) = oauth["scope"].as_str() {
            request = request.with_scopes(scope.split_whitespace());
        }
        let session = AuthorizationSession::new(manager, request)
            .await
            .map_err(|(_, e)| Refusal::Bad(said(&e)))?;
        let address = session.get_authorization_url().to_owned();
        let state = reqwest::Url::parse(&address)
            .ok()
            .and_then(|u| {
                u.query_pairs()
                    .find(|(k, _)| k == "state")
                    .map(|(_, v)| v.into_owned())
            })
            .unwrap_or_default();
        let back = Return {
            path: back_to.path().to_owned(),
            what: name.clone(),
        };
        let server = Arc::clone(self);
        let (attempt, created) = self.track(move |taken| async move {
            let (code, _) = wait(&listener, taken, &state, &back).await?;
            session
                .handle_callback(&code, &state)
                .await
                .map_err(|e| said(&e))?;
            server.mcp_signed_in(&name);
            Ok(())
        });
        Ok(json!({
            "attemptID": attempt, "url": address, "mode": "auto", "paste": true,
            "instructions": "Sign in in the browser. If it's on another computer, paste the \
                             address its last page ended on.",
            "time": { "created": created, "expires": created + 600_000 },
        }))
    }
}
