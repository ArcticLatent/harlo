//! Integrations and credentials: the providers in the model catalog, how
//! each signs in, and the keys and sign-ins the server keeps for every
//! client. A provider with a credential, or with its key in the server's
//! environment, can be used like one in the config.

// Rust guideline compliant 2026-10-09

use std::sync::Arc;
use std::time::Duration;

use harlo_engine::catalog::{Catalog, Entry};
use harlo_engine::{ProviderSpec, ascending_id, now};
use serde_json::{Map, Value, json};

use crate::server::{Refusal, Server, lock};
use crate::store::{Error, Store};

/// The integration for a Claude plan, signed in through Claude Code.
pub(crate) const CLAUDE_AGENT: &str = "claude-agent";

/// How often the catalog is fetched again.
const REFRESH: Duration = Duration::from_hours(24);

/// A credential as stored.
fn credential(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let value: String = row.get(4)?;
    Ok(json!({
        "id": row.get::<_, String>(0)?,
        "integrationID": row.get::<_, String>(1)?,
        "label": row.get::<_, String>(2)?,
        "active": row.get::<_, bool>(3)?,
        "value": serde_json::from_str::<Value>(&value).unwrap_or(Value::Null),
    }))
}

impl Store {
    /// Makes the credentials' table.
    pub(crate) fn make_credentials(conn: &rusqlite::Connection) -> Result<(), Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS credential (
                 id TEXT PRIMARY KEY, integration TEXT NOT NULL, label TEXT NOT NULL,
                 active INTEGER NOT NULL, value TEXT NOT NULL, created INTEGER NOT NULL);",
        )
    }

    /// The credentials, oldest first.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub(crate) fn credentials(&self) -> Result<Vec<Value>, Error> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, integration, label, active, value FROM credential ORDER BY created, id",
        )?;
        stmt.query_map([], credential)?.collect()
    }

    /// Keeps `value` as integration `integration`'s credential, in place
    /// of any it had; returns its ID.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub(crate) fn put_credential(
        &self,
        integration: &str,
        label: &str,
        value: &Value,
    ) -> Result<String, Error> {
        let id = ascending_id("cred");
        let conn = self.conn();
        conn.execute(
            "DELETE FROM credential WHERE integration = ?1",
            [integration],
        )?;
        conn.execute(
            "INSERT INTO credential (id, integration, label, active, value, created)
             VALUES (?1, ?2, ?3, 1, ?4, ?5)",
            rusqlite::params![id, integration, label, value.to_string(), now()],
        )?;
        Ok(id)
    }

    /// Removes credential `id`; whether it was there.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub(crate) fn delete_credential(&self, id: &str) -> Result<bool, Error> {
        Ok(self
            .conn()
            .execute("DELETE FROM credential WHERE id = ?1", [id])?
            > 0)
    }

    /// Renames credential `id`; whether it was there.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub(crate) fn label_credential(&self, id: &str, label: &str) -> Result<bool, Error> {
        Ok(self.conn().execute(
            "UPDATE credential SET label = ?2 WHERE id = ?1",
            [id, label],
        )? > 0)
    }
}

/// An answer to a sign-in.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct KeySignIn {
    key: String,
    answer: Option<Map<String, Value>>,
    label: Option<String>,
}

/// A sign-in kept outside the server, such as a program's own.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct ExternalSignIn {
    #[serde(rename = "methodID")]
    method: String,
    label: Option<String>,
}

/// The methods `entry` signs in with.
fn methods(entry: &Entry) -> Vec<Value> {
    let (label, questions) = entry.questions();
    let mut key = json!({ "type": "key" });
    if let Some(label) = label {
        key["label"] = json!(label);
    }
    if !questions.is_empty() {
        key["form"] = questions
            .iter()
            .map(|q| {
                json!({
                    "key": q.key, "title": q.title, "required": true, "type": "string",
                    "placeholder": q.placeholder,
                })
            })
            .collect();
    }
    let mut out = vec![key];
    if !entry.env.is_empty() {
        out.push(json!({ "type": "env", "names": entry.env }));
    }
    out
}

/// The connection credential `credential` makes, as clients see it.
fn connection(credential: &Value) -> Value {
    json!({
        "type": "credential", "id": credential["id"], "label": credential["label"],
        "method": credential["value"]["type"],
    })
}

/// The first of `names` set in the server's environment, with its value.
fn from_env(names: &[String]) -> Option<(String, String)> {
    names.iter().find_map(|n| {
        std::env::var(n)
            .ok()
            .filter(|v| !v.is_empty())
            .map(|v| (n.clone(), v))
    })
}

impl Server {
    /// The model catalog as last read.
    pub(crate) fn catalog(&self) -> Arc<Catalog> {
        Arc::clone(&lock(&self.catalog))
    }

    /// Where the fetched catalog is kept.
    fn catalog_file(&self) -> std::path::PathBuf {
        self.snapshots.with_file_name("models.json")
    }

    /// Reads the fetched catalog, when there's one, over the bundled one;
    /// then fetches it again every day.
    pub fn start_catalog(self: &Arc<Self>) {
        if let Ok(text) = std::fs::read_to_string(self.catalog_file())
            && let Ok(fetched) = Catalog::parse(&text)
        {
            *lock(&self.catalog) = Arc::new(fetched);
        }
        let server = Arc::clone(self);
        tokio::spawn(async move {
            let mut every = tokio::time::interval(REFRESH);
            loop {
                every.tick().await;
                server.fetch_catalog().await;
            }
        });
    }

    /// Fetches the catalog and keeps it, when it reads.
    async fn fetch_catalog(&self) {
        let got = async {
            let response = reqwest::Client::new()
                .get(harlo_engine::catalog::SOURCE)
                .timeout(Duration::from_mins(1))
                .send()
                .await
                .ok()?
                .error_for_status()
                .ok()?;
            response.text().await.ok()
        }
        .await;
        let Some(text) = got else {
            tracing::info!(name: "catalog.fetch.failed", "keeping the model catalog as it was");
            return;
        };
        if let Ok(fetched) = Catalog::parse(&text) {
            let _ = std::fs::write(self.catalog_file(), &text);
            *lock(&self.catalog) = Arc::new(fetched);
        }
    }

    /// Integration `id`, as clients see it.
    ///
    /// # Errors
    ///
    /// When there's no such integration, or the store fails.
    pub(crate) fn integration(&self, id: &str) -> Result<Value, Refusal> {
        self.integrations()?
            .into_iter()
            .find(|i| i["id"] == id)
            .ok_or_else(|| Refusal::NotFound(format!("Integration not found: {id}")))
    }

    /// Every integration, as clients see them: the catalog's providers and
    /// Claude Agent, with how each signs in and its connections.
    ///
    /// # Errors
    ///
    /// When the store fails.
    pub(crate) fn integrations(&self) -> Result<Vec<Value>, Refusal> {
        let credentials = self.store.credentials()?;
        let connections = |id: &str, env: &[String]| -> Vec<Value> {
            let mut out: Vec<Value> = credentials
                .iter()
                .filter(|c| c["integrationID"] == id)
                .map(connection)
                .collect();
            if let Some((name, _)) = from_env(env) {
                out.push(json!({ "type": "env", "name": name }));
            }
            out
        };
        let mut list: Vec<Value> = self
            .catalog()
            .entries
            .iter()
            .map(|e| {
                let mut methods = methods(e);
                if e.id == "openai" {
                    methods.push(json!({
                        "id": crate::oauth::CHATGPT, "type": "oauth",
                        "label": crate::oauth::CHATGPT_LABEL,
                    }));
                }
                json!({
                    "id": e.id, "name": e.name, "methods": methods,
                    "connections": connections(&e.id, &e.env),
                })
            })
            .collect();
        list.push(json!({
            "id": CLAUDE_AGENT, "name": "Claude Agent",
            "methods": [{
                "id": "claude-login", "type": "external",
                "label": "Claude Code login (claude auth login)",
            }],
            "connections": connections(CLAUDE_AGENT, &[]),
        }));
        list.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        Ok(list)
    }

    /// Signs integration `id` in with a key and the answers its form needs.
    ///
    /// # Errors
    ///
    /// When there's no such integration, a required answer is missing, or
    /// the store fails.
    pub(crate) fn connect_key(&self, id: &str, sign_in: KeySignIn) -> Result<(), Refusal> {
        let catalog = self.catalog();
        let entry = catalog
            .get(id)
            .ok_or_else(|| Refusal::NotFound(format!("Integration not found: {id}")))?;
        let answers = sign_in.answer.unwrap_or_default();
        for question in entry.questions().1 {
            if answers
                .get(question.key)
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(Refusal::Bad(format!(
                    "Missing required form field: {}",
                    question.key
                )));
            }
        }
        let value = json!({ "type": "key", "key": sign_in.key, "metadata": answers });
        let label = sign_in.label.unwrap_or_else(|| entry.name.clone());
        self.store.put_credential(id, &label, &value)?;
        self.integrations_changed();
        Ok(())
    }

    /// Connects integration `id` through a sign-in kept outside the
    /// server.
    ///
    /// # Errors
    ///
    /// When there's no such integration or method, or the store fails.
    pub(crate) fn connect_external(
        &self,
        id: &str,
        sign_in: ExternalSignIn,
    ) -> Result<(), Refusal> {
        if id != CLAUDE_AGENT {
            return Err(Refusal::NotFound(format!("Integration not found: {id}")));
        }
        if sign_in.method != "claude-login" {
            return Err(Refusal::NotFound(format!(
                "Integration method not found: {}",
                sign_in.method
            )));
        }
        let value = json!({ "type": "external", "methodID": sign_in.method });
        let label = sign_in.label.unwrap_or_else(|| "Claude Agent".to_owned());
        self.store.put_credential(id, &label, &value)?;
        self.integrations_changed();
        Ok(())
    }

    /// Removes credential `id`.
    ///
    /// # Errors
    ///
    /// When there's no such credential, or the store fails.
    pub(crate) fn remove_credential(&self, id: &str) -> Result<(), Refusal> {
        if !self.store.delete_credential(id)? {
            return Err(Refusal::NotFound(format!("There's no credential {id}.")));
        }
        self.integrations_changed();
        Ok(())
    }

    /// Renames credential `id`.
    ///
    /// # Errors
    ///
    /// When there's no such credential, or the store fails.
    pub(crate) fn label_credential(&self, id: &str, label: &str) -> Result<(), Refusal> {
        if !self.store.label_credential(id, label)? {
            return Err(Refusal::NotFound(format!("There's no credential {id}.")));
        }
        self.integrations_changed();
        Ok(())
    }

    /// Tells clients the integrations and providers changed.
    pub(crate) fn integrations_changed(&self) {
        self.emit("integration.updated", json!({}), None, None);
        self.emit("provider.updated", json!({}), None, None);
    }

    /// The catalog providers signed in with a credential or a key in the
    /// environment, that `configured` doesn't already have.
    pub(crate) fn signed_in(&self, configured: &[ProviderSpec]) -> Vec<ProviderSpec> {
        let catalog = self.catalog();
        let credentials = self.store.credentials().unwrap_or_default();
        catalog
            .entries
            .iter()
            .filter(|e| !configured.iter().any(|p| p.id == e.id))
            .filter_map(|e| {
                let kept = credentials.iter().find(|c| {
                    c["integrationID"] == e.id
                        && matches!(c["value"]["type"].as_str(), Some("key" | "oauth"))
                });
                if let Some(c) = kept {
                    let value = &c["value"];
                    let key = value["key"].as_str().or_else(|| value["access"].as_str());
                    e.provider(
                        key.map(str::to_owned),
                        value["metadata"].as_object().unwrap_or(&Map::new()),
                    )
                } else {
                    let (_, key) = from_env(&e.env)?;
                    e.provider(Some(key), &Map::new())
                }
            })
            .collect()
    }
}
