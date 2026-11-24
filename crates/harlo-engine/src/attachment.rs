//! Files and pictures sent with a prompt, kept with it as the server keeps
//! them: their bytes, type, name and where they came from.

// Rust guideline compliant 2026-10-09

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A file sent with a prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    /// Its bytes, in base64.
    pub data: String,
    /// Its type, such as `image/png` or `text/plain`.
    pub mime: String,
    /// Where it came from: `{"type":"inline"}`, or `{"type":"uri","uri":…}`.
    pub source: Value,
    /// The name shown for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The text in the prompt that refers to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mention: Option<Value>,
}

impl File {
    /// A file of type `mime` holding `bytes`.
    #[must_use]
    pub fn new(bytes: &[u8], mime: &str, source: Value, name: Option<String>) -> Self {
        Self {
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
            mime: mime.to_owned(),
            source,
            name,
            mention: None,
        }
    }

    /// What the model is sent for it: a picture as an image, text as
    /// text under its name; none for other kinds.
    #[must_use]
    pub fn content(&self) -> Option<Value> {
        if self.mime.starts_with("image/") {
            let url = format!("data:{};base64,{}", self.mime, self.data);
            return Some(json!({ "type": "image_url", "image_url": { "url": url } }));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&self.data)
            .ok()?;
        let text = String::from_utf8(bytes).ok()?;
        let name = self.name.as_deref().unwrap_or("file");
        Some(json!({ "type": "text", "text": format!("\n\nAttached file: {name}\n\n{text}") }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_go_as_images_and_text_as_text() {
        let picture = File::new(&[1, 2], "image/png", json!({"type":"inline"}), None);
        assert_eq!(picture.content().expect("image")["type"], "image_url");
        let text = File::new(
            b"alpha\n",
            "text/plain",
            json!({"type":"inline"}),
            Some("a.txt".into()),
        );
        assert_eq!(
            text.content().expect("text")["text"],
            "\n\nAttached file: a.txt\n\nalpha\n"
        );
        let binary = File::new(&[0xff, 0xfe], "application/octet-stream", json!({}), None);
        assert_eq!(binary.content(), None);
    }
}
