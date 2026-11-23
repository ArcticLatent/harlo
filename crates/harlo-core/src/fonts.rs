//! The font families installed on this computer, for choosing the
//! interface's and the code's font in Settings → Appearance.

// Rust guideline compliant 2026-10-09

use std::collections::BTreeMap;

/// The font families this computer has, by name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemFonts {
    /// Every family, sorted by name, ignoring case.
    pub all: Vec<String>,
    /// The families whose faces are monospaced, for code, sorted the same.
    pub mono: Vec<String>,
}

impl SystemFonts {
    /// Reads the families installed on this computer.
    ///
    /// It looks where fontconfig says fonts are, reading only each face's
    /// header, so it takes a moment on a computer with many fonts: call it
    /// off the window's thread.
    #[must_use]
    pub fn load() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        Self::from_faces(db.faces().filter_map(|face| {
            let (name, _) = face.families.first()?;
            Some((name.as_str(), face.monospaced))
        }))
    }

    /// The families of `faces`, each a family's name and whether that face
    /// is monospaced. A family counts as monospaced when all its faces are;
    /// hidden and emoji families are left out.
    pub fn from_faces<'a>(faces: impl IntoIterator<Item = (&'a str, bool)>) -> Self {
        // By lower-case name, keeping the first spelling met.
        let mut families: BTreeMap<String, (String, bool)> = BTreeMap::new();
        for (name, monospaced) in faces {
            let name = name.trim();
            // Hidden families and emoji, which can't set text on their own.
            if name.is_empty() || name.starts_with('.') || name.to_lowercase().contains("emoji") {
                continue;
            }
            families
                .entry(name.to_lowercase())
                .and_modify(|(_, mono)| *mono &= monospaced)
                .or_insert_with(|| (name.to_owned(), monospaced));
        }
        let mono = families
            .values()
            .filter(|(_, mono)| *mono)
            .map(|(name, _)| name.clone())
            .collect();
        let all = families.into_values().map(|(name, _)| name).collect();
        Self { all, mono }
    }
}

#[cfg(test)]
mod tests {
    use super::SystemFonts;

    #[test]
    fn families_are_sorted_deduplicated_and_monospaced_only_when_every_face_is() {
        let fonts = SystemFonts::from_faces([
            ("Noto Sans", false),
            ("DejaVu Sans Mono", true),
            ("dejavu sans mono", true),
            ("Mixed", true),
            ("Mixed", false),
            ("", false),
            (".Hidden", false),
            ("Noto Color Emoji", true),
            ("Fira Code", true),
        ]);
        assert_eq!(
            fonts.all,
            ["DejaVu Sans Mono", "Fira Code", "Mixed", "Noto Sans"]
        );
        assert_eq!(fonts.mono, ["DejaVu Sans Mono", "Fira Code"]);
    }
}
