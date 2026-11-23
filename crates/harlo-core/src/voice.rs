//! Dictation on this computer: Whisper speech models, downloaded once, and
//! whisper.cpp to turn a recording into text. Nothing leaves the computer
//! but the model download.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

use futures_util::Stream;

pub use crate::download::Download;
use crate::download::Fetch;
use transcribe_rs::whisper_cpp::{WhisperEngine, WhisperInferenceParams};

/// Whisper's input: 16 kHz mono.
pub const SAMPLE_RATE: u32 = 16_000;

/// Where the models download from, unless a test points elsewhere.
const SOURCE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// A Whisper speech model Harlo offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Model {
    /// Quick, good for clear speech.
    #[default]
    Base,
    /// Slower, more accurate.
    Small,
}

impl Model {
    /// Every model, in the order Settings lists them.
    pub const ALL: [Self; 2] = [Self::Base, Self::Small];

    /// The name it's saved under in the preferences.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Small => "small",
        }
    }

    /// The model called `name`, if there is one.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == name)
    }

    /// Its file, here and at the source.
    #[must_use]
    pub fn file(self) -> &'static str {
        match self {
            Self::Base => "ggml-base.bin",
            Self::Small => "ggml-small.bin",
        }
    }

    /// Its size in bytes.
    #[must_use]
    pub fn size(self) -> u64 {
        match self {
            Self::Base => 147_951_465,
            Self::Small => 487_601_967,
        }
    }

    /// How Settings names it.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Base => "Base · 148 MB · quick, good for clear speech",
            Self::Small => "Small · 488 MB · slower, more accurate",
        }
    }
}

/// The languages Settings offers, by Whisper's code, "auto" first.
pub const LANGUAGE_CODES: [&str; 18] = [
    "auto", "en", "de", "es", "fr", "it", "pt", "nl", "tr", "ru", "pl", "uk", "sv", "ja", "zh",
    "ko", "ar", "hi",
];

/// The languages' names, in [`LANGUAGE_CODES`]' order.
pub const LANGUAGE_NAMES: [&str; 18] = [
    "Detect it",
    "English",
    "German",
    "Spanish",
    "French",
    "Italian",
    "Portuguese",
    "Dutch",
    "Turkish",
    "Russian",
    "Polish",
    "Ukrainian",
    "Swedish",
    "Japanese",
    "Chinese",
    "Korean",
    "Arabic",
    "Hindi",
];

/// The folder of downloaded models, and where they come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Models {
    /// The folder.
    folder: PathBuf,
    /// The address the files are under.
    source: String,
}

impl Models {
    /// The user's models, in `$XDG_DATA_HOME/harlo/whisper`, from Hugging
    /// Face. `None` when the system has no data folder.
    #[must_use]
    pub fn user() -> Option<Self> {
        let data = dirs::data_dir()?;
        Some(Self::new(data.join("harlo").join("whisper"), SOURCE))
    }

    /// Models in `folder`, downloaded from under `source`.
    #[must_use]
    pub fn new(folder: impl Into<PathBuf>, source: &str) -> Self {
        Self {
            folder: folder.into(),
            source: source.trim_end_matches('/').to_owned(),
        }
    }

    /// The folder.
    #[must_use]
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Where `model` is kept.
    #[must_use]
    pub fn path(&self, model: Model) -> PathBuf {
        self.folder.join(model.file())
    }

    /// Whether `model` is downloaded: a download only takes its name once
    /// it's whole.
    #[must_use]
    pub fn ready(&self, model: Model) -> bool {
        std::fs::metadata(self.path(model)).is_ok_and(|m| m.len() > 0)
    }

    /// Downloads `model`, telling how far it got; dropping the stream stops
    /// it, and the next download starts over.
    pub fn download(&self, model: Model) -> impl Stream<Item = Download> + Send + 'static {
        Fetch {
            url: format!("{}/{}", self.source, model.file()),
            path: self.path(model),
            size: model.size(),
            sha256: None,
        }
        .download()
    }
}

/// Turns recordings into text, keeping the last model loaded.
#[derive(Default)]
pub struct Transcriber {
    /// The loaded model and where it came from.
    loaded: Option<(PathBuf, WhisperEngine)>,
}

impl std::fmt::Debug for Transcriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transcriber")
            .field("loaded", &self.loaded.as_ref().map(|(path, _)| path))
            .finish()
    }
}

impl Transcriber {
    /// Nothing loaded yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The text spoken in `samples` (16 kHz mono), with the model at
    /// `model`; `language` is Whisper's code, or "auto".
    ///
    /// Runs whisper.cpp on this thread, which takes a while: call it off
    /// the UI's.
    ///
    /// # Errors
    ///
    /// When too little was recorded, the model can't be loaded, or
    /// transcription fails.
    pub fn transcribe(
        &mut self,
        model: &Path,
        samples: &[f32],
        language: &str,
    ) -> Result<String, String> {
        if samples.len() < SAMPLE_RATE as usize / 4 {
            return Err("Nothing was recorded. Check the microphone.".to_owned());
        }
        let engine = match &mut self.loaded {
            Some((path, engine)) if path == model => engine,
            held => {
                // The old model goes before the new one loads.
                *held = None;
                let engine = WhisperEngine::load(model)
                    .map_err(|e| format!("Couldn't load the speech model: {e}"))?;
                &mut held.insert((model.to_owned(), engine)).1
            }
        };
        let language = language.trim();
        let params = WhisperInferenceParams {
            language: (!language.is_empty() && language != "auto").then(|| language.to_owned()),
            n_threads: std::thread::available_parallelism()
                .map_or(4, |n| n.get().min(8))
                .try_into()
                .unwrap_or(4),
            ..WhisperInferenceParams::default()
        };
        let result = engine
            .transcribe_with(samples, &params)
            .map_err(|e| format!("Transcription failed: {e}"))?;
        Ok(transcript_text(&result.text))
    }
}

/// Interleaved `samples` at `rate` with `channels` as 16 kHz mono: the
/// channels averaged, the rate changed by linear interpolation, which is
/// fine for speech.
#[must_use]
pub fn to_whisper_input(samples: &[f32], rate: u32, channels: u16) -> Vec<f32> {
    let channels = usize::from(channels.max(1));
    #[expect(clippy::cast_precision_loss, reason = "a channel count is tiny")]
    let mono: Vec<f32> = samples
        .chunks(channels)
        .map(|c| c.iter().sum::<f32>() / c.len() as f32)
        .collect();
    if rate == SAMPLE_RATE || rate == 0 || mono.is_empty() {
        return mono;
    }
    let step = f64::from(rate) / f64::from(SAMPLE_RATE);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "sample positions: positive, far below 2^52, rounded down on purpose"
    )]
    let resampled = {
        let len = (mono.len() as f64 / step).floor() as usize;
        (0..len)
            .map(|i| {
                let at = i as f64 * step;
                let (j, frac) = (at.floor() as usize, (at - at.floor()) as f32);
                let a = mono[j];
                let b = mono.get(j + 1).copied().unwrap_or(a);
                a + (b - a) * frac
            })
            .collect()
    };
    resampled
}

/// Whisper's words, without its markers for silence and sounds
/// (`[BLANK_AUDIO]`, `(music)`), in one line.
#[must_use]
pub fn transcript_text(raw: &str) -> String {
    const SOUNDS: [&str; 7] = [
        "music",
        "silence",
        "applause",
        "laughter",
        "noise",
        "inaudible",
        "blank_audio",
    ];
    let mut kept = String::new();
    for line in raw.lines() {
        let mut rest = line.trim();
        while let Some(start) = rest.find(['[', '(']) {
            let close = if rest[start..].starts_with('[') {
                ']'
            } else {
                ')'
            };
            let Some(len) = rest[start..].find(close) else {
                break;
            };
            let inner = &rest[start + 1..start + len];
            kept.push_str(&rest[..start]);
            // Markers are in capitals or a sound; real parentheses stay.
            let marker = inner
                .chars()
                .all(|c| c.is_ascii_uppercase() || c == '_' || c == ' ')
                || SOUNDS.contains(&inner.to_ascii_lowercase().as_str());
            if !marker {
                kept.push_str(&rest[start..=start + len]);
            }
            rest = &rest[start + len + 1..];
        }
        kept.push_str(rest);
        kept.push(' ');
    }
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}
