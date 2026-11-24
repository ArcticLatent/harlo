//! Web search: which service a session's searches go to.
//!
//! The folder's config (`websearch.provider`) names a service or `random`;
//! without one, the first search asks the person, in a form, whether to
//! allow web search (any service, at random), to choose one, or to turn it
//! off, and keeps the answer for every project. `"websearch": false` in
//! the config, or the answer to turn it off, leaves the tool out.
//!
//! At random, a session keeps the service it was given until that one is
//! limited (HTTP 429); then the search goes to another, and the limited
//! one is left alone as long as it asked, a minute unless it said.

// Rust guideline compliant 2026-10-09

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use futures_util::future::BoxFuture;
use harlo_engine::tools::Output;
use harlo_engine::web::{SERVICES, Search, Searcher};
use serde_json::{Value, json};

use crate::credentials::from_env;
use crate::server::{Server, lock};

/// The setting that keeps the person's answer.
const CHOICE: &str = "websearch:provider";

/// What the person, or the config, chose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Choice {
    /// One service, by ID.
    Service(String),
    /// Any service, at random.
    Random,
    /// No web search.
    Off,
}

impl Choice {
    /// A choice as the setting keeps it: an ID, `"random"` or `false`.
    fn of(value: &Value) -> Option<Self> {
        match value {
            Value::Bool(false) => Some(Self::Off),
            Value::String(s) if s == "random" => Some(Self::Random),
            Value::String(s) if SERVICES.iter().any(|(id, _, _)| id == s) => {
                Some(Self::Service(s.clone()))
            }
            _ => None,
        }
    }

    fn value(&self) -> Value {
        match self {
            Self::Service(id) => json!(id),
            Self::Random => json!("random"),
            Self::Off => json!(false),
        }
    }
}

/// Where random searches stand.
#[derive(Debug, Default)]
pub(crate) struct Searches {
    /// The service each session was given.
    given: HashMap<String, String>,
    /// The limited services, and until when they're left alone.
    cooling: HashMap<String, Instant>,
}

impl Searches {
    /// The service for `session`'s next try, other than those `tried`:
    /// the one it was given while it isn't limited, else another at
    /// random. None when every one is limited or tried.
    fn next(&mut self, session: &str, tried: &HashSet<String>) -> Option<String> {
        let now = Instant::now();
        self.cooling.retain(|_, until| *until > now);
        let free = |id: &str| !self.cooling.contains_key(id) && !tried.contains(id);
        if let Some(given) = self.given.get(session).filter(|g| free(g)) {
            return Some(given.clone());
        }
        let open: Vec<&str> = SERVICES
            .iter()
            .map(|(id, _, _)| *id)
            .filter(|id| free(id))
            .collect();
        if open.is_empty() {
            return None;
        }
        let pick = usize::try_from(getrandom::u32().unwrap_or(0)).unwrap_or(0) % open.len();
        let id = open[pick].to_owned();
        self.given.insert(session.to_owned(), id.clone());
        Some(id)
    }
}

/// Where a session's web searches go.
#[derive(Debug)]
pub(crate) struct SessionSearch {
    /// The server.
    pub server: Arc<Server>,
    /// The session.
    pub session: String,
    /// Its folder.
    pub directory: String,
}

impl Searcher for SessionSearch {
    fn search<'a>(&'a self, query: &'a str) -> BoxFuture<'a, Result<Output, String>> {
        Box::pin(async move {
            let choice = match self.server.search_choice(&self.directory) {
                Some(choice) => choice,
                None => {
                    self.server
                        .ask_search_choice(&self.session, &self.directory)
                        .await?
                }
            };
            match choice {
                Choice::Off => Err("Web search is turned off.".to_owned()),
                Choice::Service(id) => harlo_engine::web::search(&self.server.search(&id), query)
                    .await
                    .map_err(|f| f.text),
                Choice::Random => self.server.search_at_random(&self.session, query).await,
            }
        })
    }
}

impl Server {
    /// The choice for `directory`: its config's, else the person's answer.
    pub(crate) fn search_choice(&self, directory: &str) -> Option<Choice> {
        if let Some(said) = self.folder(directory).config.websearch {
            return Choice::of(&said);
        }
        let kept = self.store.setting(CHOICE).ok().flatten()?;
        Choice::of(&serde_json::from_str(&kept).ok()?)
    }

    /// Where session `session`'s searches go; none when web search is off.
    pub(crate) fn searcher(
        self: &Arc<Self>,
        session: &str,
        directory: &str,
    ) -> Option<Arc<dyn Searcher>> {
        if self.search_choice(directory) == Some(Choice::Off) {
            return None;
        }
        Some(Arc::new(SessionSearch {
            server: Arc::clone(self),
            session: session.to_owned(),
            directory: directory.to_owned(),
        }))
    }

    /// Service `id`, with its key when it has one: signed in, or in its
    /// environment variable.
    fn search(&self, id: &str) -> Search {
        let env = SERVICES
            .iter()
            .find(|(s, _, _)| *s == id)
            .map(|(_, _, env)| (*env).to_owned());
        let key = self
            .store
            .credentials()
            .unwrap_or_default()
            .iter()
            .find(|c| c["integrationID"] == id && c["value"]["type"] == "key")
            .and_then(|c| c["value"]["key"].as_str().map(str::to_owned))
            .or_else(|| from_env(&env.into_iter().collect::<Vec<_>>()).map(|(_, v)| v));
        Search {
            service: id.to_owned(),
            key,
            base: std::env::var("HARLO_WEBSEARCH_BASE")
                .ok()
                .filter(|b| !b.is_empty()),
        }
    }

    /// Searches `query` at random for `session`, moving on from limited
    /// services.
    async fn search_at_random(&self, session: &str, query: &str) -> Result<Output, String> {
        let mut tried = HashSet::new();
        let mut last = None;
        loop {
            let Some(id) = lock(&self.live).searches.next(session, &tried) else {
                return Err(last.unwrap_or_else(|| {
                    "Every search service is limited for now; try again in a minute.".to_owned()
                }));
            };
            tried.insert(id.clone());
            match harlo_engine::web::search(&self.search(&id), query).await {
                Ok(found) => return Ok(found),
                Err(failure) => {
                    let Some(wait) = failure.limited else {
                        return Err(failure.text);
                    };
                    lock(&self.live)
                        .searches
                        .cooling
                        .insert(id, Instant::now() + wait);
                    last = Some(failure.text);
                }
            }
        }
    }

    /// Asks the person in `session` whether to search the web, and with
    /// which service; keeps the answer.
    async fn ask_search_choice(&self, session: &str, directory: &str) -> Result<Choice, String> {
        let names: Vec<&str> = SERVICES.iter().map(|(_, name, _)| *name).collect();
        let first = self
            .ask_one(
                session,
                directory,
                "Web Search",
                "choice",
                "Allow Harlo to search the web \
                for up-to-date information?",
                &[
                    ("allow", &format!("Allow search via {}", names.join(", "))),
                    ("choose", "Choose another provider"),
                    ("disable", "Disable web search"),
                ],
            )
            .await?;
        let choice = match first.as_str() {
            "allow" => Choice::Random,
            "disable" => Choice::Off,
            _ => {
                let services: Vec<(&str, &str)> =
                    SERVICES.iter().map(|(id, name, _)| (*id, *name)).collect();
                let id = self
                    .ask_one(
                        session,
                        directory,
                        "Choose a web search provider",
                        "provider",
                        "Choose a provider for web search.",
                        &services,
                    )
                    .await?;
                Choice::of(&json!(id)).ok_or_else(|| format!("There's no search service {id}."))?
            }
        };
        self.store
            .put_setting(CHOICE, &choice.value().to_string())
            .map_err(|e| format!("Couldn't keep the choice: {e}"))?;
        Ok(choice)
    }

    /// Asks one question with `options` (value, label) in a form titled
    /// `title`, and waits for the value chosen.
    async fn ask_one(
        &self,
        session: &str,
        directory: &str,
        title: &str,
        key: &str,
        question: &str,
        options: &[(&str, &str)],
    ) -> Result<String, String> {
        let id = harlo_engine::ascending_id("frm");
        let options: Vec<Value> = options
            .iter()
            .map(|(value, label)| json!({ "value": value, "label": label }))
            .collect();
        let form = json!({
            "id": id, "sessionID": session, "title": title,
            "metadata": { "kind": "websearch.provider" },
            "fields": [{ "key": key, "description": question, "required": true,
                         "type": "string", "options": options, "custom": false }],
        });
        let answer = self
            .ask_form(session, directory, &id, form)
            .ok_or_else(|| "The session stopped.".to_owned())?
            .await
            .ok()
            .flatten()
            .ok_or_else(|| "The user dismissed the question about web search.".to_owned())?;
        answer
            .into_iter()
            .next()
            .and_then(|values| values.into_iter().next())
            .ok_or_else(|| "The user didn't answer the question about web search.".to_owned())
    }
}
