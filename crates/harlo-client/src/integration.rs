//! Signing the server in to integrations, such as model providers: with a
//! key, through a browser (OAuth), by running a program, or with
//! credentials already on the server's computer. The server keeps what
//! it gets as a credential, for every client of the server.

// Rust guideline compliant 2026-10-09

use reqwest::Method;
use serde_json::{Map, Value, json};

use crate::client::Client;
use crate::error::Error;
use crate::settings::Integration;
use crate::types::{Answer, Field};

/// A way to sign in to an integration.
#[derive(Debug, Clone, PartialEq)]
pub enum SignInMethod {
    /// Paste a key, with answers to `form` when it has fields.
    Key {
        /// What the key is called, such as `Bedrock API key`.
        label: Option<String>,
        /// More it needs, such as an account ID.
        form: Vec<Field>,
    },
    /// Sign in through a browser.
    OAuth {
        /// The method's ID, sent when starting.
        id: String,
        /// Its name, such as `Login with GitHub Copilot`.
        label: String,
        /// What it asks first.
        form: Vec<Field>,
    },
    /// Run a program on the server's computer that signs in.
    Command {
        /// The method's ID, sent when starting.
        id: String,
        /// Its name.
        label: String,
        /// The program and its arguments.
        command: Vec<String>,
    },
    /// Use credentials already on the server's computer, such as another
    /// program's sign-in.
    External {
        /// The method's ID, sent when connecting.
        id: String,
        /// Its name, such as `AWS profile (SSO or named profile)`.
        label: String,
        /// What it asks first.
        form: Vec<Field>,
    },
    /// Environment variables the server reads; set where it runs.
    Env {
        /// The variables, any of which will do.
        names: Vec<String>,
    },
}

impl SignInMethod {
    /// Reads one of an integration's `methods`.
    pub(crate) fn from_json(value: &Value) -> Option<Self> {
        let text = |key: &str| value[key].as_str().map(str::to_owned);
        let form = || -> Vec<Field> {
            value["form"]
                .as_array()
                .map(|a| a.iter().filter_map(Field::from_json).collect())
                .unwrap_or_default()
        };
        Some(match value["type"].as_str()? {
            "key" => Self::Key {
                label: text("label"),
                form: form(),
            },
            "oauth" => Self::OAuth {
                id: text("id")?,
                label: text("label").unwrap_or_default(),
                form: form(),
            },
            "command" => Self::Command {
                id: text("id")?,
                label: text("label").unwrap_or_default(),
                command: value["command"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|s| s.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            "external" => Self::External {
                id: text("id")?,
                label: text("label").unwrap_or_default(),
                form: form(),
            },
            "env" => Self::Env {
                names: value["names"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|s| s.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            _ => return None,
        })
    }

    /// What the method asks before it starts; empty when nothing.
    #[must_use]
    pub fn form(&self) -> &[Field] {
        match self {
            Self::Key { form, .. } | Self::OAuth { form, .. } | Self::External { form, .. } => form,
            Self::Command { .. } | Self::Env { .. } => &[],
        }
    }
}

/// A browser sign-in that has started.
#[derive(Debug, Clone, PartialEq)]
pub struct OAuthAttempt {
    /// Its ID, for asking how it stands, finishing or cancelling it.
    pub id: String,
    /// The page to open.
    pub url: String,
    /// What to do there, in the integration's words.
    pub instructions: String,
    /// Whether the page shows a code that must be brought back; otherwise
    /// the server learns of the sign-in itself.
    pub needs_code: bool,
    /// Whether the address the browser lands on can be pasted back, for
    /// when the server can't hear from the browser itself.
    pub paste: bool,
    /// When it stops waiting, in milliseconds since the Unix epoch.
    pub expires: Option<f64>,
}

/// How a sign-in that has started stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptStatus {
    /// Still waiting, with what a program has said so far, if anything.
    Pending(Option<String>),
    /// Signed in: the credential is stored.
    Complete,
    /// It failed, and why.
    Failed(String),
}

impl AttemptStatus {
    /// Reads a status answer's `data`.
    fn from_json(value: &Value) -> Self {
        let message = value["message"].as_str().map(str::to_owned);
        match value["status"].as_str() {
            Some("complete") => Self::Complete,
            Some("failed") => Self::Failed(message.unwrap_or_else(|| "It failed.".to_owned())),
            _ => Self::Pending(message.filter(|m| !m.is_empty())),
        }
    }
}

/// Answers to a method's form, as the server takes them; `null` when there
/// are none.
fn answer_json(answers: &[(String, Answer)]) -> Value {
    if answers.is_empty() {
        return Value::Null;
    }
    let map: Map<String, Value> = answers
        .iter()
        .map(|(key, answer)| (key.clone(), answer.json()))
        .collect();
    Value::Object(map)
}

impl Client {
    /// Lists every integration the server knows, with its ways to sign in
    /// and its connections.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn integrations(&self) -> Result<Vec<Integration>, Error> {
        let v = Self::json(self.request(Method::GET, "/api/integration")).await?;
        Ok(v["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Integration::from_json)
            .collect())
    }

    /// Signs integration `id` in with `key` and the answers to its key
    /// method's form, for every client of the server.
    ///
    /// # Errors
    ///
    /// Fails when the server refuses it, such as for a missing form answer
    /// (it says which), or can't be reached.
    pub async fn connect_key(
        &self,
        id: &str,
        key: &str,
        answers: &[(String, Answer)],
    ) -> Result<(), Error> {
        let path = format!("/api/integration/{id}/connect/key");
        self.post(
            &path,
            &json!({ "key": key, "answer": answer_json(answers) }),
        )
        .await
    }

    /// Connects integration `id` with credentials already on the server's
    /// computer, by method `method` and the answers to its form.
    ///
    /// # Errors
    ///
    /// Fails when the server refuses it or can't be reached.
    pub async fn connect_external_with(
        &self,
        id: &str,
        method: &str,
        answers: &[(String, Answer)],
    ) -> Result<(), Error> {
        let path = format!("/api/integration/{id}/connect/external");
        let body = json!({ "methodID": method, "answer": answer_json(answers) });
        self.post(&path, &body).await
    }

    /// Starts a browser sign-in to integration `id` by method `method`.
    ///
    /// # Errors
    ///
    /// Fails when the server refuses it or can't be reached.
    pub async fn begin_oauth(
        &self,
        id: &str,
        method: &str,
        answers: &[(String, Answer)],
    ) -> Result<OAuthAttempt, Error> {
        let path = format!("/api/integration/{id}/connect/oauth");
        let body = json!({ "methodID": method, "answer": answer_json(answers) });
        let v = Self::json(self.request(Method::POST, &path).json(&body)).await?;
        let data = &v["data"];
        Ok(OAuthAttempt {
            id: data["attemptID"].as_str().unwrap_or_default().to_owned(),
            url: data["url"].as_str().unwrap_or_default().to_owned(),
            instructions: data["instructions"].as_str().unwrap_or_default().to_owned(),
            needs_code: data["mode"].as_str() == Some("code"),
            paste: data["paste"].as_bool().unwrap_or(false),
            expires: data["time"]["expires"].as_f64(),
        })
    }

    /// How browser sign-in `attempt` to integration `id` stands.
    ///
    /// # Errors
    ///
    /// Fails when the attempt is gone or the server can't be reached.
    pub async fn oauth_status(&self, id: &str, attempt: &str) -> Result<AttemptStatus, Error> {
        let path = format!("/api/integration/{id}/connect/oauth/{attempt}");
        let v = Self::json(self.request(Method::GET, &path)).await?;
        Ok(AttemptStatus::from_json(&v["data"]))
    }

    /// Finishes browser sign-in `attempt` with the code the page showed, or
    /// the address the browser landed on.
    ///
    /// # Errors
    ///
    /// Fails when the server refuses the code or can't be reached.
    pub async fn complete_oauth(&self, id: &str, attempt: &str, code: &str) -> Result<(), Error> {
        let path = format!("/api/integration/{id}/connect/oauth/{attempt}/complete");
        self.post(&path, &json!({ "code": code })).await
    }

    /// Cancels browser sign-in `attempt`.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn cancel_oauth(&self, id: &str, attempt: &str) -> Result<(), Error> {
        let path = format!("/api/integration/{id}/connect/oauth/{attempt}");
        Self::send(self.request(Method::DELETE, &path)).await?;
        Ok(())
    }

    /// Starts the program that signs integration `id` in, by method
    /// `method`, and returns the attempt's ID.
    ///
    /// # Errors
    ///
    /// Fails when the server refuses it or can't be reached.
    pub async fn begin_command(&self, id: &str, method: &str) -> Result<String, Error> {
        let path = format!("/api/integration/{id}/connect/command");
        let body = json!({ "methodID": method });
        let v = Self::json(self.request(Method::POST, &path).json(&body)).await?;
        Ok(v["data"]["attemptID"]
            .as_str()
            .unwrap_or_default()
            .to_owned())
    }

    /// How the sign-in program of `attempt` stands, with what it has said.
    ///
    /// # Errors
    ///
    /// Fails when the attempt is gone or the server can't be reached.
    pub async fn command_status(&self, id: &str, attempt: &str) -> Result<AttemptStatus, Error> {
        let path = format!("/api/integration/{id}/connect/command/{attempt}");
        let v = Self::json(self.request(Method::GET, &path)).await?;
        Ok(AttemptStatus::from_json(&v["data"]))
    }

    /// Stops the sign-in program of `attempt`.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached.
    pub async fn cancel_command(&self, id: &str, attempt: &str) -> Result<(), Error> {
        let path = format!("/api/integration/{id}/connect/command/{attempt}");
        Self::send(self.request(Method::DELETE, &path)).await?;
        Ok(())
    }
}
