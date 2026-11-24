//! Where an OpenCode server lives and how to find the shared one.

// Rust guideline compliant 2026-10-09

use std::fmt::{self, Debug, Formatter};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::Error;

/// An OpenCode server's address and password.
///
/// The password is sent as HTTP Basic auth with the fixed user `opencode`.
/// `Debug` never shows it.
#[derive(Clone, PartialEq, Eq)]
pub struct Endpoint {
    url: String,
    password: Option<String>,
}

impl Endpoint {
    /// Creates an endpoint for `url` (such as `http://127.0.0.1:49374`).
    #[must_use]
    pub fn new(url: impl Into<String>, password: Option<String>) -> Self {
        let mut url = url.into();
        while url.ends_with('/') {
            url.pop();
        }
        Self { url, password }
    }

    /// Returns the server's base URL, without a trailing slash.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The same server, with its password, reached at `url` instead: the
    /// local end of a tunnel to it.
    #[must_use]
    pub fn at(&self, url: impl Into<String>) -> Self {
        Self::new(url, self.password.clone())
    }

    pub(crate) fn password(&self) -> Option<&str> {
        self.password.as_deref()
    }
}

impl Debug for Endpoint {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("url", &self.url)
            .field("password", &self.password.as_ref().map(|_| "…"))
            .finish()
    }
}

/// The background service's registration, as `harlo-server` writes it.
#[derive(Debug, Deserialize)]
struct Registration {
    url: String,
    password: Option<String>,
    version: Option<String>,
    pid: Option<u32>,
}

/// The background service found on this computer.
#[derive(Debug, Clone)]
pub struct SharedService {
    /// How to reach it.
    pub endpoint: Endpoint,
    /// The server version that registered it, when recorded.
    pub version: Option<String>,
    /// Its process, when recorded.
    pub pid: Option<u32>,
}

/// Returns the path of the shared service's registration file.
///
/// OpenCode records the running background service in
/// `$XDG_STATE_HOME/opencode/service.json`, falling back to
/// `~/.local/state/opencode/service.json`.
#[must_use]
pub fn registration_path(xdg_state_home: Option<&Path>, home: &Path) -> PathBuf {
    xdg_state_home
        .map_or_else(|| home.join(".local").join("state"), Path::to_path_buf)
        .join("opencode")
        .join("service.json")
}

/// Reads the shared service's registration from `path`.
///
/// Returns `Ok(None)` when no service has registered. The registration says
/// where a service was started; whether it still answers is checked by
/// calling the server (for example [`crate::Client::info`]).
///
/// # Errors
///
/// Returns an error when the file exists but can't be read or parsed.
pub fn read_registration(path: &Path) -> Result<Option<SharedService>, Error> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    parse_registration(&text).map(Some)
}

/// Reads a shared service's registration from `text`, the contents of its
/// `service.json`, such as one read on another computer.
///
/// # Errors
///
/// Returns an error when `text` isn't a registration.
pub fn parse_registration(text: &str) -> Result<SharedService, Error> {
    let registration: Registration = serde_json::from_str(text)?;
    Ok(SharedService {
        endpoint: Endpoint::new(registration.url, registration.password),
        version: registration.version,
        pid: registration.pid,
    })
}
