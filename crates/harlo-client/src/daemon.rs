//! What Harlo's own server adds to the API: phone access hosted by the
//! server, so phones keep working while no desktop is open, and notes kept
//! between sessions.
//!
//! OpenCode doesn't have these routes; [`Client::hosts_phone`] and
//! [`Client::keeps_notes`] say whether the server does.

// Rust guideline compliant 2026-10-09

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Client, Error};

/// How the desktop wants the server's phone access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneWanted {
    /// Whether phone access is on.
    pub on: bool,
    /// Whether phones reach the server through the relay too.
    pub relay: bool,
    /// Whether notifications are held back.
    pub paused: bool,
    /// The port phones connect to directly; 0 picks a free one.
    pub port: u16,
    /// This computer's name, as phones show it.
    pub name: String,
    /// Project folders phones don't list.
    pub hidden: Vec<String>,
    /// The theme phones draw in, as the phone app reads it (see
    /// `harlo_core::phone::theme`); null for Harlo's own.
    #[serde(default)]
    pub theme: Value,
}

/// An avatar picked on a phone, numbered so the desktop takes each pick
/// once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarPick {
    /// Counts the picks since phone access started.
    pub serial: u64,
    /// The avatar's name, such as `scarf`.
    pub name: String,
}

/// A paired phone, as the server lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneDevice {
    /// The server's ID for it.
    pub id: String,
    /// The name it gave when it paired.
    pub name: String,
    /// When it was last connected, in seconds since the Unix epoch.
    pub last_seen: i64,
    /// Whether it takes notifications.
    pub push: bool,
}

/// Where the server's connection to the relay is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "detail", rename_all = "camelCase")]
pub enum PhoneRelay {
    /// Not on the relay.
    Off,
    /// Reaching it.
    Connecting,
    /// Connected, at this relay ID.
    Online(String),
    /// The last attempt failed, and why; the server tries again.
    Failed(String),
}

/// The server's phone access as it stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneState {
    /// Bumped on every change; pass it to [`Client::phone_state`] to wait
    /// for the next.
    pub version: u64,
    /// What was last asked for.
    pub wanted: Option<PhoneWanted>,
    /// The port phones connect to directly, while listening.
    pub port: Option<u16>,
    /// Why it couldn't start.
    pub error: Option<String>,
    /// The relay connection.
    pub relay: PhoneRelay,
    /// The paired phones.
    pub devices: Vec<PhoneDevice>,
    /// The IDs of the phones connected now.
    pub connected: Vec<String>,
    /// The avatar last picked on a phone, if one was.
    #[serde(default)]
    pub avatar_pick: Option<AvatarPick>,
}

/// Who a note is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteScope {
    /// Every session of the project, in any of its worktrees.
    Project,
    /// Every session of the user's.
    User,
}

/// Where a note came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteSource {
    /// The session.
    pub session: String,
    /// The message that kept it.
    pub message: String,
    /// The session's title then, if it had one.
    #[serde(default)]
    pub title: Option<String>,
}

/// A decision kept between sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// Its ID.
    pub id: String,
    /// Who it's for.
    pub scope: NoteScope,
    /// What was decided.
    pub text: String,
    /// Where it came from.
    pub source: NoteSource,
    /// When it was kept, in Unix milliseconds.
    pub created: i64,
    /// The note that replaced it, if one did.
    #[serde(rename = "supersededBy", default)]
    pub superseded_by: Option<String>,
}

impl Client {
    /// Whether the server keeps notes between sessions (Harlo's own
    /// server).
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached or rejects the password.
    pub async fn keeps_notes(&self) -> Result<bool, Error> {
        let info = Self::json(self.request(Method::GET, "/api/info")).await?;
        Ok(info["capabilities"]["memory"].as_bool() == Some(true))
    }

    /// Whether notes are on.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached or doesn't keep notes.
    pub async fn notes_on(&self) -> Result<bool, Error> {
        let v = Self::json(self.request(Method::GET, "/harlo/memory")).await?;
        Ok(v["enabled"].as_bool() == Some(true))
    }

    /// Turns notes on or off, for every session on the server.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached or doesn't keep notes.
    pub async fn set_notes_on(&self, on: bool) -> Result<(), Error> {
        let request = self
            .request(Method::PUT, "/harlo/memory")
            .json(&serde_json::json!({ "enabled": on }));
        Self::json(request).await.map(drop)
    }

    /// The user's notes and project `project`'s, oldest first, replaced
    /// ones included.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached or doesn't keep notes.
    pub async fn notes(&self, project: Option<&str>) -> Result<Vec<Note>, Error> {
        let mut request = self.request(Method::GET, "/harlo/notes");
        if let Some(project) = project {
            request = request.query(&[("projectID", project)]);
        }
        let v = Self::json(request).await?;
        Ok(serde_json::from_value(v["data"].clone())?)
    }

    /// Corrects note `id` to say `text`.
    ///
    /// # Errors
    ///
    /// Fails when there's no such note or the text is empty.
    pub async fn correct_note(&self, id: &str, text: &str) -> Result<(), Error> {
        let request = self
            .request(Method::PATCH, &format!("/harlo/notes/{id}"))
            .json(&serde_json::json!({ "text": text }));
        Self::json(request).await.map(drop)
    }

    /// Forgets note `id`.
    ///
    /// # Errors
    ///
    /// Fails when there's no such note.
    pub async fn forget_note(&self, id: &str) -> Result<(), Error> {
        Self::send(self.request(Method::DELETE, &format!("/harlo/notes/{id}")))
            .await
            .map(drop)
    }

    /// Whether the server hosts phone access itself (Harlo's own server).
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached or rejects the password.
    pub async fn hosts_phone(&self) -> Result<bool, Error> {
        let info = Self::json(self.request(Method::GET, "/api/info")).await?;
        Ok(info["capabilities"]["phone"].as_bool() == Some(true))
    }

    /// The server's phone access; with `after`, once its version is past
    /// that (or after a while, unchanged).
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached or doesn't host phone access.
    pub async fn phone_state(&self, after: Option<u64>) -> Result<PhoneState, Error> {
        let mut request = self.request(Method::GET, "/harlo/phone");
        if let Some(after) = after {
            request = request.query(&[("after", after)]);
        }
        Ok(serde_json::from_value(Self::json(request).await?)?)
    }

    /// Asks the server for phone access as `wanted` says.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached or doesn't host phone access.
    pub async fn want_phone(&self, wanted: &PhoneWanted) -> Result<PhoneState, Error> {
        let request = self.request(Method::PUT, "/harlo/phone").json(wanted);
        Ok(serde_json::from_value(Self::json(request).await?)?)
    }

    /// A new pairing link, for the QR code.
    ///
    /// # Errors
    ///
    /// Fails when phone access isn't running.
    pub async fn pair_phone(&self) -> Result<String, Error> {
        let v: Value = Self::json(self.request(Method::POST, "/harlo/phone/pair")).await?;
        v["link"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::other("the server gave no pairing link"))
    }

    /// Forgets the paired phone with ID `id`.
    ///
    /// # Errors
    ///
    /// Fails when phone access isn't running or the list can't be saved.
    pub async fn remove_phone(&self, id: &str) -> Result<(), Error> {
        let path = format!("/harlo/phone/device/{id}");
        Self::send(self.request(Method::DELETE, &path))
            .await
            .map(drop)
    }

    /// Sends a test notification: how many phones got it.
    ///
    /// # Errors
    ///
    /// Fails when none could be sent, with why.
    pub async fn test_phone(&self) -> Result<usize, Error> {
        let v: Value = Self::json(self.request(Method::POST, "/harlo/phone/test")).await?;
        Ok(v["sent"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(0))
    }
}
