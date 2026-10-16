//! Microphone recording for dictation (Qt Multimedia, `cpp/harlo_audio.h`).
//! Called on the Qt thread.

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("harlo_audio.h");

        /// Starts recording from the default microphone; an error, or "".
        #[namespace = "harlo"]
        fn audio_start() -> QString;
        /// Stops; the samples (interleaved floats), with their rate and
        /// channel count.
        #[namespace = "harlo"]
        fn audio_stop(rate: &mut i32, channels: &mut i32) -> Vec<f32>;
        /// Stops and drops the recording.
        #[namespace = "harlo"]
        fn audio_cancel();
        /// The latest loudness (0–1).
        #[namespace = "harlo"]
        fn audio_level() -> f32;
    }
}

pub use ffi::{audio_cancel, audio_level, audio_start, audio_stop};
