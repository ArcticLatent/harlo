//! One set of fonts for every code view: harlo's bundled monospace font, the
//! system's fonts for characters it lacks, and the rasterized glyphs.

// Rust guideline compliant 2026-10-09

use std::sync::{Mutex, MutexGuard, OnceLock};

use cosmic_text::{FontSystem, SwashCache};

pub struct Fonts {
    pub system: FontSystem,
    pub glyphs: SwashCache,
}

impl std::fmt::Debug for Fonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fonts").field("faces", &self.system.db().len()).finish_non_exhaustive()
    }
}

static BUNDLED: OnceLock<Vec<&'static [u8]>> = OnceLock::new();
static FONTS: OnceLock<Mutex<Fonts>> = OnceLock::new();

fn build() -> Mutex<Fonts> {
    let mut system = FontSystem::new();
    for data in BUNDLED.get().map(std::vec::Vec::as_slice).unwrap_or_default() {
        system.db_mut().load_font_data(data.to_vec());
    }
    Mutex::new(Fonts { system, glyphs: SwashCache::new() })
}

/// Starts loading the fonts in the background (the system's take a moment),
/// with harlo's own font files.
pub fn preload(bundled: Vec<&'static [u8]>) {
    let _ = BUNDLED.set(bundled);
    std::thread::spawn(|| {
        FONTS.get_or_init(build);
        // The syntaxes too (read on first use: ~20 ms on the first file).
        let _ = harlo_core::markdown::code_syntaxes();
        let _ = harlo_core::markdown::code_theme();
    });
}

/// The fonts, waiting for them if they're still loading.
pub fn lock() -> MutexGuard<'static, Fonts> {
    FONTS.get_or_init(build).lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Tests: harlo's bundled monospace font, before anything draws.
#[cfg(test)]
pub fn for_tests() {
    let _ = BUNDLED.set(vec![include_bytes!("../../harlo-app/fonts/JetBrainsMono-Regular.ttf")]);
}
