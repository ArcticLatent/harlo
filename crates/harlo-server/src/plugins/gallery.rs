//! The plugins people have published: GitHub repositories tagged
//! `harlo-plugin`, listed when a person asks to browse them.
//!
//! Anyone can tag a repository, so nothing listed is reviewed; a plugin
//! picked from the list is added like one typed in, and what it asks for
//! waits for a yes all the same. GitHub is asked only when a person asks
//! for the list, never on the server's own, and what it answered is kept
//! a while so paging through Settings doesn't ask again.

// Rust guideline compliant 2026-10-09

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::fetch;
use crate::server::{Refusal, Server, lock};

/// The topic a repository carries to be listed.
const TOPIC: &str = "harlo-plugin";

/// Where repositories are searched, unless `HARLO_PLUGIN_GALLERY_BASE`
/// names a stand-in, for tests.
const GITHUB: &str = "https://api.github.com";

/// How long the search may take. GitHub answers in under a second; past
/// this it's taken as unreachable.
const WAIT: Duration = Duration::from_secs(15);

/// The most repositories listed: a page a person reads, most starred
/// first.
const LISTED: usize = 50;

/// How long an answer is kept. GitHub allows ten searches a minute
/// without a sign-in, shared by everyone at the same address.
const KEPT: Duration = Duration::from_mins(10);

/// The list as last answered, and when.
#[derive(Debug, Default)]
pub(crate) struct Kept(Option<(Instant, Vec<Value>)>);

/// The plugins in GitHub's answer `found`, as clients list them: those
/// whose address a plugin can be added from.
fn listed(found: &Value) -> Vec<Value> {
    found["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let src = item["html_url"].as_str()?;
            fetch::source(src).ok()?;
            Some(json!({
                "name": item["full_name"].as_str().unwrap_or(src),
                "src": src,
                "description": item["description"].as_str(),
                "stars": item["stargazers_count"].as_u64().unwrap_or_default(),
                "updated": item["pushed_at"].as_str(),
            }))
        })
        .take(LISTED)
        .collect()
}

impl Server {
    /// The plugins people have published, most starred first.
    ///
    /// # Errors
    ///
    /// When GitHub can't be reached, limits the search, or answers
    /// something else than a list.
    pub(crate) async fn plugin_gallery(&self) -> Result<Vec<Value>, Refusal> {
        if let Some((at, list)) = &lock(&self.plugins).gallery.0
            && at.elapsed() < KEPT
        {
            return Ok(list.clone());
        }
        let base = std::env::var("HARLO_PLUGIN_GALLERY_BASE")
            .ok()
            .filter(|b| !b.trim().is_empty())
            .unwrap_or_else(|| GITHUB.to_owned());
        let failed = |why: String| Refusal::Bad(format!("GitHub couldn't be asked: {why}"));
        let client = reqwest::Client::builder()
            .timeout(WAIT)
            .build()
            .map_err(|e| failed(e.to_string()))?;
        let response = client
            .get(format!(
                "{}/search/repositories",
                base.trim_end_matches('/')
            ))
            .query(&[
                ("q", format!("topic:{TOPIC} archived:false")),
                ("sort", "stars".to_owned()),
                ("order", "desc".to_owned()),
                ("per_page", LISTED.to_string()),
            ])
            // (GitHub answers no request without one.)
            .header("user-agent", "harlo")
            .header("accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| failed(e.to_string()))?;
        let status = response.status();
        if status == reqwest::StatusCode::FORBIDDEN
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        {
            return Err(Refusal::Bad(
                "GitHub is limiting searches right now. Try again in a minute.".to_owned(),
            ));
        }
        if !status.is_success() {
            return Err(failed(format!("it answered {status}.")));
        }
        let found: Value = response
            .json()
            .await
            .map_err(|e| failed(format!("its answer doesn't read: {e}")))?;
        let list = listed(&found);
        lock(&self.plugins).gallery.0 = Some((Instant::now(), list.clone()));
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_what_can_be_added_is_listed() {
        let found = json!({ "items": [
            {
                "full_name": "someone/review", "html_url": "https://github.com/someone/review",
                "description": "Reviews.", "stargazers_count": 12, "pushed_at": "2026-11-01T10:00:00Z",
            },
            { "full_name": "plain/http", "html_url": "http://example.com/plain/http" },
            { "full_name": "with/password", "html_url": "https://me:secret@example.com/x" },
            { "full_name": "no/address" },
            { "html_url": "https://github.com/bare/one" },
        ]});
        let list = listed(&found);
        assert_eq!(list.len(), 2, "{list:?}");
        assert_eq!(
            list[0],
            json!({
                "name": "someone/review", "src": "https://github.com/someone/review",
                "description": "Reviews.", "stars": 12, "updated": "2026-11-01T10:00:00Z",
            })
        );
        assert_eq!(list[1]["name"], "https://github.com/bare/one");
        assert_eq!(list[1]["stars"], 0);
        assert_eq!(
            listed(&json!({ "message": "Not Found" })),
            Vec::<Value>::new()
        );
    }
}
