//! A prompt to send: its text, how it's delivered, and what it brings along.

// Rust guideline compliant 2026-10-09

use serde_json::{Value, json};

use crate::types::Delivery;

/// Where some text sits in the prompt, so OpenCode can tie an attachment to it.
///
/// Offsets count UTF-16 code units, as OpenCode's own clients do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    /// Offset of the mention's first unit.
    pub start: usize,
    /// Offset just past its last unit.
    pub end: usize,
    /// The mention as typed, such as `@src/main.rs`.
    pub text: String,
}

impl Mention {
    /// Finds `needle` in `text` and returns its place, or `None` when it isn't there.
    #[must_use]
    pub fn find(text: &str, needle: &str) -> Option<Self> {
        let at = text.find(needle)?;
        let start = text[..at].encode_utf16().count();
        Some(Self {
            start,
            end: start + needle.encode_utf16().count(),
            text: needle.to_owned(),
        })
    }

    fn json(&self) -> Value {
        json!({ "start": self.start, "end": self.end, "text": self.text })
    }
}

/// A file, image or resource sent with a prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    /// Where OpenCode reads it: `file:///…`, an MCP URI, or `data:` with the bytes inline.
    pub uri: String,
    /// The name shown for it.
    pub name: Option<String>,
    /// The text in the prompt that refers to it.
    pub mention: Option<Mention>,
}

impl Attachment {
    /// A file on the server's machine, by absolute path.
    #[must_use]
    pub fn file(path: &str) -> Self {
        Self {
            uri: format!("file://{path}"),
            name: None,
            mention: None,
        }
    }

    /// Bytes sent with the prompt, such as a pasted image.
    #[must_use]
    pub fn inline(mime: &str, bytes: &[u8], name: &str) -> Self {
        use base64::Engine as _;
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        Self {
            uri: format!("data:{mime};base64,{data}"),
            name: Some(name.to_owned()),
            mention: None,
        }
    }

    /// Ties the attachment to the text that mentions it.
    #[must_use]
    pub fn mentioned(mut self, mention: Option<Mention>) -> Self {
        self.mention = mention;
        self
    }

    fn json(&self) -> Value {
        let mut out = json!({ "uri": self.uri });
        if self.uri.starts_with("data:") {
            out["source"] = json!({ "type": "inline" });
        }
        if let Some(name) = &self.name {
            out["name"] = json!(name);
        }
        if let Some(mention) = &self.mention {
            out["mention"] = mention.json();
        }
        out
    }
}

/// A prompt for [`Client::prompt`](crate::Client::prompt).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    /// Client-made message ID (see [`crate::message_id`]); resending it is safe.
    pub id: String,
    /// What the user wrote.
    pub text: String,
    /// How it reaches a busy session.
    pub delivery: Delivery,
    /// Files, images and resources sent along.
    pub files: Vec<Attachment>,
    /// Skills to load first, by ID.
    pub skills: Vec<String>,
    /// Subagents the prompt names, by ID.
    pub agents: Vec<String>,
}

impl Prompt {
    /// A plain prompt with a fresh message ID.
    #[must_use]
    pub fn new(text: impl Into<String>, delivery: Delivery) -> Self {
        Self {
            id: crate::message_id(),
            text: text.into(),
            delivery,
            files: Vec::new(),
            skills: Vec::new(),
            agents: Vec::new(),
        }
    }

    pub(crate) fn json(&self) -> Value {
        let mut body = json!({
            "id": self.id,
            "text": self.text,
            "delivery": self.delivery.as_str(),
        });
        if !self.files.is_empty() {
            body["files"] = self.files.iter().map(Attachment::json).collect();
        }
        if !self.skills.is_empty() {
            body["skills"] = self.skills.iter().map(|id| json!({ "id": id })).collect();
        }
        if !self.agents.is_empty() {
            body["agents"] = self
                .agents
                .iter()
                .map(|name| json!({ "name": name }))
                .collect();
        }
        body
    }
}
