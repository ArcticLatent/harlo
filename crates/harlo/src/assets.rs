//! Images bundled with the app, loaded once.

// Rust guideline compliant 2026-10-09

use iced::widget::{image, svg};

/// Handles for the bundled images. Creating a handle hashes its bytes, so
/// views reuse these instead of making new ones.
#[derive(Debug, Clone)]
pub struct Assets {
    /// Harlo's logo mark.
    pub logo: svg::Handle,
    /// The default avatar.
    pub avatar: image::Handle,
    providers: Vec<(&'static str, svg::Handle)>,
}

impl Assets {
    /// Loads every bundled image.
    #[must_use]
    pub fn load() -> Self {
        let provider = |id, bytes: &'static [u8]| (id, svg::Handle::from_memory(bytes));
        Self {
            logo: svg::Handle::from_memory(
                include_bytes!("../assets/images/logo-mark.svg").as_slice(),
            ),
            avatar: image::Handle::from_bytes(
                include_bytes!("../assets/images/avatar-penguin.png").as_slice(),
            ),
            providers: vec![
                provider(
                    "anthropic",
                    include_bytes!("../assets/images/providers/anthropic.svg"),
                ),
                provider(
                    "openai",
                    include_bytes!("../assets/images/providers/openai.svg"),
                ),
                provider(
                    "zai",
                    include_bytes!("../assets/images/providers/zai-coding-plan.svg"),
                ),
            ],
        }
    }

    /// The logo of a model's maker, by OpenCode provider ID, with the logo's key.
    ///
    /// Providers that serve one maker's models under several IDs, such as
    /// `zai` and `zai-coding-plan`, share a logo.
    #[must_use]
    pub fn provider(&self, id: &str) -> Option<(&'static str, &svg::Handle)> {
        self.providers
            .iter()
            .find(|(key, _)| id == *key || id.starts_with(&format!("{key}-")))
            .map(|(key, handle)| (*key, handle))
    }
}
