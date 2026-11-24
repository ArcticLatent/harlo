//! Bringing sessions made on OpenCode over to Harlo's server.
//!
//! Each one is exported from OpenCode and imported whole, keeping its ID,
//! with the subagent sessions under it. A session already on Harlo's server is left
//! as it is, so importing again doubles nothing.

// Rust guideline compliant 2026-10-09

use std::path::PathBuf;

use opencode_client::{Client, read_registration, registration_path};

/// What an import did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Imported {
    /// Sessions brought over, subagents' included.
    pub copied: usize,
    /// Sessions Harlo's server had already.
    pub already: usize,
    /// What went wrong, one line per session that didn't come over.
    pub failed: Vec<String>,
}

/// Copies sessions `ids`, with the subagent sessions under them, from
/// `from` to `to`, each into the folder it was in.
pub async fn copy_sessions(from: &Client, to: &Client, ids: &[String]) -> Imported {
    let mut imported = Imported::default();
    let mut queue: Vec<String> = ids.iter().rev().cloned().collect();
    while let Some(id) = queue.pop() {
        if let Ok(children) = from.child_sessions(&id).await {
            queue.extend(children.into_iter().map(|c| c.id));
        }
        if to.session(&id).await.is_ok() {
            imported.already += 1;
            continue;
        }
        let copied = async {
            let session = from.session(&id).await?;
            let transcript = from.export_session(&id).await?;
            to.import_session(&transcript, &session.directory).await
        };
        match copied.await {
            Ok(_) => imported.copied += 1,
            Err(e) => imported.failed.push(format!("{id}: {e}")),
        }
    }
    imported
}

/// Copies sessions from OpenCode's service on this computer to Harlo's
/// server here: `ids`, or every top-level session when `None`.
///
/// # Errors
///
/// When either server can't be reached, or OpenCode's sessions can't be
/// listed.
pub async fn from_opencode(ids: Option<Vec<String>>) -> Result<Imported, String> {
    let from = opencode().await?;
    let to = crate::server::reach_local().await?;
    let ids = match ids {
        Some(ids) => ids,
        None => from
            .root_sessions()
            .await
            .map_err(|e| format!("Couldn't list OpenCode's sessions: {e}"))?
            .into_iter()
            .map(|s| s.id)
            .collect(),
    };
    Ok(copy_sessions(&from, &to, &ids).await)
}

/// A client for OpenCode's background service on this computer, started
/// once when none answers, as OpenCode's own clients do. Harlo never stops
/// it.
async fn opencode() -> Result<Client, String> {
    if let Some(client) = registered().await? {
        return Ok(client);
    }
    start_opencode().await?;
    registered()
        .await?
        .ok_or_else(|| "OpenCode's background service isn't running".to_owned())
}

/// A client for OpenCode's registered service, if it answers.
async fn registered() -> Result<Option<Client>, String> {
    let state = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map_or_else(PathBuf::new, PathBuf::from);
    let path = registration_path(state.as_deref(), &home);
    let service =
        read_registration(&path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    let Some(service) = service else {
        return Ok(None);
    };
    let client = Client::new(service.endpoint);
    Ok(client.info().await.is_ok().then_some(client))
}

/// Starts OpenCode's background service.
async fn start_opencode() -> Result<(), String> {
    let output = tokio::process::Command::new("opencode")
        .args(["service", "start"])
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                "OpenCode isn't installed (no `opencode` on PATH), so there's nothing to import"
                    .to_owned()
            }
            _ => format!("couldn't start OpenCode's background service: {e}"),
        })?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "couldn't start OpenCode's background service: {}",
            stderr.trim()
        ))
    }
}
