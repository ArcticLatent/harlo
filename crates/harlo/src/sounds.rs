//! The event sounds Settings → Notifications picks: MP3s built into Harlo,
//! decoded and played with rodio on the default output device.
//!
//! The sounds are the server's, under the MIT licence in [`LICENSE`].

// Rust guideline compliant 2026-10-09

use std::io::Cursor;

/// The licence the sounds come under, which About shows.
pub const LICENSE: &str = include_str!("../sounds/LICENSE.txt");

/// Maps each id in [`harlo_core::notifications::SOUNDS`] to its MP3.
macro_rules! sound_files {
    ($($id:literal),* $(,)?) => {
        /// The MP3 of the sound `id`, if Harlo has it.
        fn file(id: &str) -> Option<&'static [u8]> {
            match id {
                $($id => Some(include_bytes!(concat!("../sounds/", $id, ".mp3"))),)*
                _ => None,
            }
        }
    };
}

sound_files!(
    "alert-01",
    "alert-02",
    "alert-03",
    "alert-04",
    "alert-05",
    "alert-06",
    "alert-07",
    "alert-08",
    "alert-09",
    "alert-10",
    "bip-bop-01",
    "bip-bop-02",
    "bip-bop-03",
    "bip-bop-04",
    "bip-bop-05",
    "bip-bop-06",
    "bip-bop-07",
    "bip-bop-08",
    "bip-bop-09",
    "bip-bop-10",
    "staplebops-01",
    "staplebops-02",
    "staplebops-03",
    "staplebops-04",
    "staplebops-05",
    "staplebops-06",
    "staplebops-07",
    "nope-01",
    "nope-02",
    "nope-03",
    "nope-04",
    "nope-05",
    "nope-06",
    "nope-07",
    "nope-08",
    "nope-09",
    "nope-10",
    "nope-11",
    "nope-12",
    "yup-01",
    "yup-02",
    "yup-03",
    "yup-04",
    "yup-05",
    "yup-06",
);

/// Plays the sound `id` once, on a thread of its own that holds the output
/// device only while it plays.
///
/// A sound that can't play, with no output device or an unknown id, is
/// skipped: it only accompanies what the window shows.
pub fn play(id: &str) {
    let Some(mp3) = file(id) else {
        return;
    };
    // A thread that can't start leaves the sound unplayed, as above.
    let _ = std::thread::Builder::new()
        .name("harlo-sound".to_owned())
        .spawn(move || {
            let Ok(mut device) = rodio::DeviceSinkBuilder::open_default_sink() else {
                return;
            };
            device.log_on_drop(false);
            if let Ok(player) = rodio::play(device.mixer(), Cursor::new(mp3)) {
                player.sleep_until_end();
            }
        });
}

/// Whether Harlo has, and can decode, every sound Settings offers.
#[cfg(test)]
mod tests {
    #[test]
    fn every_sound_is_built_in_and_decodes() {
        for id in harlo_core::notifications::SOUNDS {
            let mp3 = super::file(id).expect(id);
            let decoded = rodio::Decoder::new(std::io::Cursor::new(mp3));
            assert!(decoded.is_ok(), "{id}");
        }
    }
}
