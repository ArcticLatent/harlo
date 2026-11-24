//! Bringing sessions made on OpenCode over to Harlo's server.
//!
//! Each one is exported from OpenCode and imported whole, keeping its ID,
//! with the subagent sessions under it. A session already on Harlo's server is left
//! as it is, so importing again doubles nothing.

// Rust guideline compliant 2026-10-09

use opencode_client::Client;

use crate::layout::Backend;

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
    let from = crate::server::reach_here(Backend::OpenCode).await?;
    let to = crate::server::reach_here(Backend::Harlo).await?;
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
