//! Saved approvals: what a person allowed "always", kept per project so
//! every session there runs it without asking from then on.

// Rust guideline compliant 2026-10-09

use harlo_engine::{Effect, Rule, ascending_id, now};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::server::{Refusal, Server};
use crate::store::{Error, Store};

/// A saved approval's row, in OpenCode's shape.
fn approval(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": row.get::<_, String>(0)?,
        "projectID": row.get::<_, String>(1)?,
        "action": row.get::<_, String>(2)?,
        "resource": row.get::<_, String>(3)?,
        "time": { "created": row.get::<_, i64>(4)?, "updated": row.get::<_, i64>(5)? },
    }))
}

const COLUMNS: &str = "id, project, action, resource, created, updated";

impl Store {
    /// Makes the saved approvals' table.
    pub(crate) fn make_approvals(conn: &rusqlite::Connection) -> Result<(), Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS approval (
                 id TEXT PRIMARY KEY, project TEXT NOT NULL, action TEXT NOT NULL,
                 resource TEXT NOT NULL, created INTEGER NOT NULL, updated INTEGER NOT NULL,
                 UNIQUE (project, action, resource));",
        )
    }

    /// Saves an approval of `action` on `resource` in project `project`;
    /// saving one already there marks it updated.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub fn save_approval(&self, project: &str, action: &str, resource: &str) -> Result<(), Error> {
        let at = now();
        self.conn().execute(
            "INSERT INTO approval (id, project, action, resource, created, updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT (project, action, resource) DO UPDATE SET updated = excluded.updated",
            params![ascending_id("psv"), project, action, resource, at],
        )?;
        Ok(())
    }

    /// The saved approvals, of project `project` or of every project.
    ///
    /// # Errors
    ///
    /// Fails when the read fails.
    pub fn approvals(&self, project: Option<&str>) -> Result<Vec<Value>, Error> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM approval WHERE ?1 IS NULL OR project = ?1 ORDER BY id"
        ))?;
        stmt.query_map([project], approval)?.collect()
    }

    /// Saved approval `id`, after changing its action or resource.
    ///
    /// # Errors
    ///
    /// Fails when the write fails; `None` when there's no such approval.
    pub fn change_approval(
        &self,
        id: &str,
        action: Option<&str>,
        resource: Option<&str>,
    ) -> Result<Option<Value>, Error> {
        let conn = self.conn();
        conn.execute(
            "UPDATE approval SET action = coalesce(?2, action),
             resource = coalesce(?3, resource), updated = ?4 WHERE id = ?1",
            params![id, action, resource, now()],
        )?;
        conn.query_row(
            &format!("SELECT {COLUMNS} FROM approval WHERE id = ?1"),
            [id],
            approval,
        )
        .optional()
    }

    /// Removes saved approval `id`, if it's there.
    ///
    /// # Errors
    ///
    /// Fails when the write fails.
    pub fn delete_approval(&self, id: &str) -> Result<(), Error> {
        self.conn()
            .execute("DELETE FROM approval WHERE id = ?1", [id])?;
        Ok(())
    }
}

impl Server {
    /// Saves what a person allowed "always" in session `id`: each pattern
    /// in the request's `save`, for its project.
    pub(crate) fn save_approvals(&self, id: &str, request: &Value) -> Result<(), Refusal> {
        let session = self.session(id)?;
        let action = request["action"].as_str().unwrap_or_default();
        for pattern in request["save"].as_array().into_iter().flatten() {
            if let Some(resource) = pattern.as_str() {
                self.store
                    .save_approval(&session.project_id, action, resource)?;
            }
        }
        Ok(())
    }

    /// Project `project`'s saved approvals, as rules that allow.
    pub(crate) fn saved_rules(&self, project: &str) -> Vec<Rule> {
        self.store
            .approvals(Some(project))
            .unwrap_or_default()
            .iter()
            .map(|a| Rule {
                action: a["action"].as_str().unwrap_or_default().to_owned(),
                resource: a["resource"].as_str().unwrap_or_default().to_owned(),
                effect: Effect::Allow,
            })
            .collect()
    }
}
