//! Saved approvals: what "Allow always" saved for a project, so the server
//! allows it there without asking. A configured `deny` still wins.

// Rust guideline compliant 2026-10-09

use reqwest::Method;
use serde_json::Value;

use crate::client::Client;
use crate::error::Error;

/// An approval the server saved when someone chose "Allow always".
#[derive(Debug, Clone, PartialEq)]
pub struct SavedApproval {
    /// Its ID, for removing it.
    pub id: String,
    /// The ID of the project it applies in.
    pub project_id: String,
    /// What it allows, such as `shell` or `edit`.
    pub action: String,
    /// The pattern it allows that on, such as `git status *`.
    pub resource: String,
    /// When it was last saved, in milliseconds since the Unix epoch.
    pub updated: f64,
}

impl SavedApproval {
    /// Reads one from the server's answer.
    fn from_json(value: &Value) -> Option<Self> {
        let text = |key: &str| value[key].as_str().map(str::to_owned);
        Some(Self {
            id: text("id")?,
            project_id: text("projectID").unwrap_or_default(),
            action: text("action").unwrap_or_default(),
            resource: text("resource").unwrap_or_default(),
            updated: value["time"]["updated"].as_f64().unwrap_or_default(),
        })
    }
}

impl Client {
    /// Lists every saved approval, in every project.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn saved_approvals(&self) -> Result<Vec<SavedApproval>, Error> {
        let v = Self::json(self.request(Method::GET, "/api/permission/saved")).await?;
        Ok(v["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(SavedApproval::from_json)
            .collect())
    }

    /// Removes saved approval `id`, so the server asks again, for every client
    /// of the server.
    ///
    /// # Errors
    ///
    /// Fails when the approval is gone or the server can't be reached.
    pub async fn remove_saved_approval(&self, id: &str) -> Result<(), Error> {
        let path = format!("/api/permission/saved/{id}");
        Self::send(self.request(Method::DELETE, &path)).await?;
        Ok(())
    }
}
