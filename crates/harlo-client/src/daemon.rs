//! What Harlo's own server adds to the API: phone access hosted by the
//! server, so phones keep working while no desktop is open, notes kept
//! between sessions, and plugins.
//!
//! A server from before 2.0 doesn't have these routes;
//! [`Client::hosts_phone`] and [`Client::keeps_notes`] say whether the
//! server does.

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

/// Where a plugin stands on the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginState {
    /// In use.
    Active,
    /// Turned off.
    Off,
    /// What it asks for hasn't been allowed.
    Waiting,
    /// Listed, but not fetched on the server's computer.
    Missing,
    /// Unusable; [`Plugin::error`] says why.
    Failed,
}

/// A plugin someone published, as the server lists them from GitHub.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PublishedPlugin {
    /// Its repository, as owner and name.
    pub name: String,
    /// The address it's added from.
    pub src: String,
    /// What it's for, in its publisher's words.
    #[serde(default)]
    pub description: Option<String>,
    /// How many people starred it.
    #[serde(default)]
    pub stars: u64,
    /// When it last changed, as GitHub writes it.
    #[serde(default)]
    pub updated: Option<String>,
}

/// Something a person fills in for a plugin, such as a token.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PluginSetting {
    /// The name the plugin's code reads it by.
    pub key: String,
    /// What a person is asked for.
    pub label: String,
    /// Whether it's a secret, which the server never gives back.
    #[serde(default)]
    pub secret: bool,
    /// Whether it's filled in.
    #[serde(default)]
    pub set: bool,
    /// What it's filled in with, unless it's a secret.
    #[serde(default)]
    pub value: Option<String>,
}

/// A plugin added to the server: a package that brings skills, commands,
/// agents, MCP servers and tools of its own.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plugin {
    /// Its name.
    pub name: String,
    /// Where it comes from: an address or a folder.
    pub src: String,
    /// The tag or branch it follows; none follows the default branch.
    #[serde(default)]
    pub version: Option<String>,
    /// The commit it's kept at.
    pub commit: String,
    /// Whether it's turned on.
    pub enabled: bool,
    /// Where it stands.
    pub state: PluginState,
    /// Why it can't be used, when it can't.
    #[serde(default)]
    pub error: Option<String>,
    /// What it's for, in a line.
    #[serde(default)]
    pub description: Option<String>,
    /// The version it gives itself.
    #[serde(default)]
    pub release: Option<String>,
    /// What it asks for that waits for a yes, a line each.
    #[serde(default)]
    pub asks: Vec<String>,
    /// How many skills it brings.
    #[serde(default)]
    pub skills: usize,
    /// How many commands it brings.
    #[serde(default)]
    pub commands: usize,
    /// How many agents it brings.
    #[serde(default)]
    pub agents: usize,
    /// The MCP servers it brings, by name.
    #[serde(default)]
    pub mcp: Vec<String>,
    /// The tools its own code gives agents, by name.
    #[serde(default)]
    pub tools: Vec<String>,
    /// What its code runs for with no one calling, such as `turn.ended`.
    #[serde(default)]
    pub events: Vec<String>,
    /// What a person fills in for it.
    #[serde(default)]
    pub settings: Vec<PluginSetting>,
    /// Why its code last failed when it ran with no one calling.
    #[serde(default)]
    pub event_error: Option<String>,
}

impl Client {
    /// The plugins added to the server.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached or has no plugins (a server
    /// from before they existed).
    pub async fn plugins(&self) -> Result<Vec<Plugin>, Error> {
        let v = Self::json(self.request(Method::GET, "/harlo/plugins")).await?;
        Ok(serde_json::from_value(v["data"].clone())?)
    }

    /// Adds the plugin at `src` (an `https://` address, or a folder on the
    /// server's computer) at `version` (a tag, branch or commit), for
    /// every client of the server. What it asks for still waits for
    /// [`Self::approve_plugin`].
    ///
    /// # Errors
    ///
    /// Fails when it can't be fetched, isn't a plugin, or one of its name
    /// is there already.
    pub async fn add_plugin(&self, src: &str, version: Option<&str>) -> Result<Plugin, Error> {
        let request = self
            .request(Method::POST, "/harlo/plugins")
            .json(&serde_json::json!({ "src": src, "version": version }));
        Self::plugin(request).await
    }

    /// The plugins people have published, most starred first. The server
    /// asks GitHub for them.
    ///
    /// # Errors
    ///
    /// Fails when the server can't be reached, or GitHub can't.
    pub async fn published_plugins(&self) -> Result<Vec<PublishedPlugin>, Error> {
        let v = Self::json(self.request(Method::GET, "/harlo/plugin-gallery")).await?;
        Ok(serde_json::from_value(v["data"].clone())?)
    }

    /// Allows what plugin `name` asks for now.
    ///
    /// # Errors
    ///
    /// Fails when there's no such plugin.
    pub async fn approve_plugin(&self, name: &str) -> Result<Plugin, Error> {
        Self::plugin(self.request(Method::POST, &format!("/harlo/plugins/{name}/approve"))).await
    }

    /// Moves plugin `name` to the newest commit of what it follows.
    ///
    /// # Errors
    ///
    /// Fails when there's no such plugin or the newer one can't be
    /// fetched.
    pub async fn update_plugin(&self, name: &str) -> Result<Plugin, Error> {
        Self::plugin(self.request(Method::POST, &format!("/harlo/plugins/{name}/update"))).await
    }

    /// Fetches plugin `name` where it's listed but missing.
    ///
    /// # Errors
    ///
    /// Fails when there's no such plugin or it can't be fetched.
    pub async fn install_plugin(&self, name: &str) -> Result<Plugin, Error> {
        Self::plugin(self.request(Method::POST, &format!("/harlo/plugins/{name}/install"))).await
    }

    /// Turns plugin `name` on or off.
    ///
    /// # Errors
    ///
    /// Fails when there's no such plugin.
    pub async fn set_plugin_enabled(&self, name: &str, enabled: bool) -> Result<Plugin, Error> {
        let request = self
            .request(Method::PATCH, &format!("/harlo/plugins/{name}"))
            .json(&serde_json::json!({ "enabled": enabled }));
        Self::plugin(request).await
    }

    /// Fills in setting `key` of plugin `name` with `value`, or clears it.
    ///
    /// # Errors
    ///
    /// Fails when there's no such plugin or it names no such setting.
    pub async fn fill_plugin(
        &self,
        name: &str,
        key: &str,
        value: Option<&str>,
    ) -> Result<Plugin, Error> {
        let request = self
            .request(Method::PUT, &format!("/harlo/plugins/{name}/settings"))
            .json(&serde_json::json!({ key: value }));
        Self::plugin(request).await
    }

    /// Removes plugin `name` from the server.
    ///
    /// # Errors
    ///
    /// Fails when there's no such plugin.
    pub async fn remove_plugin(&self, name: &str) -> Result<(), Error> {
        Self::send(self.request(Method::DELETE, &format!("/harlo/plugins/{name}")))
            .await
            .map(drop)
    }

    /// The plugin a plugin request answers with.
    async fn plugin(request: reqwest::RequestBuilder) -> Result<Plugin, Error> {
        let v = Self::json(request).await?;
        Ok(serde_json::from_value(v["data"].clone())?)
    }

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
