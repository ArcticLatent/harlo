//! Dictation into the composer: Ctrl+Shift+D (or the microphone button)
//! starts recording, and again stops it; whisper.cpp turns it into text on
//! this computer, which goes in at the cursor. Settings → Voice downloads
//! the speech model and picks the language.

// Rust guideline compliant 2026-10-09

use std::sync::{Arc, Mutex, PoisonError};

use harlo_core::voice::{Download, Model, Models, Transcriber, to_whisper_input};

use super::sheets::{Event as SheetEvent, Section};
use super::{App, Message};
use crate::audio::Recording;
use crate::task::{Handle, Task};

/// Something that happened to dictation.
#[derive(Debug, Clone, PartialEq)]
pub enum VoiceEvent {
    /// Start recording, or stop and turn it into text.
    Toggle,
    /// Stop recording and forget it.
    Cancel,
    /// The recording was turned into text, or why not.
    Transcribed(Result<String, String>),
    /// Download the chosen speech model.
    Download,
    /// Stop downloading it.
    StopDownload,
    /// A step of the download.
    Downloading(Download),
}

/// Where dictation stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Listening {
    /// Not listening.
    #[default]
    Idle,
    /// Recording the microphone.
    Recording,
    /// Turning the recording into text.
    Transcribing,
}

/// Dictation's state.
#[derive(Debug)]
pub struct Voice {
    /// The downloaded models; `None` when the system has no data folder.
    pub models: Option<Models>,
    /// Where dictation stands.
    pub listening: Listening,
    /// The recording under way.
    recording: Option<Recording>,
    /// Why the last dictation failed.
    pub error: Option<String>,
    /// How far the model's download got, while it runs.
    pub progress: Option<f64>,
    /// The download, which stops when dropped.
    download: Option<Handle>,
    /// Why the last download failed.
    pub download_error: Option<String>,
    /// whisper.cpp, keeping the last model loaded.
    transcriber: Arc<Mutex<Transcriber>>,
    /// For tests: what the microphone "hears", so nothing is recorded and
    /// whisper.cpp doesn't run.
    pub heard: Option<String>,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            models: Models::user(),
            listening: Listening::Idle,
            recording: None,
            error: None,
            progress: None,
            download: None,
            download_error: None,
            transcriber: Arc::default(),
            heard: None,
        }
    }
}

impl App {
    /// The speech model dictation uses.
    #[must_use]
    pub fn voice_model(&self) -> Model {
        Model::named(&self.preferences.voice_model).unwrap_or_default()
    }

    /// Whether the chosen speech model is downloaded, so dictation works.
    #[must_use]
    pub fn voice_ready(&self) -> bool {
        let model = self.voice_model();
        self.voice.models.as_ref().is_some_and(|m| m.ready(model))
    }

    /// Handles `event` on dictation.
    pub(super) fn voice_event(&mut self, event: VoiceEvent) -> Task<Message> {
        match event {
            VoiceEvent::Toggle => match self.voice.listening {
                Listening::Idle => self.start_listening(),
                Listening::Recording => self.stop_listening(),
                Listening::Transcribing => Task::none(),
            },
            VoiceEvent::Cancel => {
                self.voice.recording = None;
                self.voice.listening = Listening::Idle;
                Task::none()
            }
            VoiceEvent::Transcribed(result) => {
                self.voice.listening = Listening::Idle;
                match result {
                    Ok(text) if text.trim().is_empty() => {
                        self.voice.error = Some("No speech heard.".to_owned());
                    }
                    Ok(text) => self.insert_spoken(&text),
                    Err(why) => self.voice.error = Some(why),
                }
                Task::none()
            }
            VoiceEvent::Download => self.download_voice(),
            VoiceEvent::StopDownload => {
                self.voice.download = None;
                self.voice.progress = None;
                Task::none()
            }
            VoiceEvent::Downloading(Download::Progress(fraction)) => {
                if self.voice.download.is_some() {
                    self.voice.progress = Some(fraction);
                }
                Task::none()
            }
            VoiceEvent::Downloading(Download::Done(result)) => {
                self.voice.download = None;
                self.voice.progress = None;
                self.voice.download_error = result.err();
                Task::none()
            }
        }
    }

    /// Starts recording, or sends to Settings → Voice when there's no model.
    fn start_listening(&mut self) -> Task<Message> {
        self.voice.error = None;
        if !self.voice_ready() {
            self.voice.error =
                Some("Download a speech model in Settings → Voice first.".to_owned());
            return self.update(Message::Sheet(SheetEvent::Open(Section::Voice)));
        }
        if self.voice.heard.is_none() {
            match Recording::start() {
                Ok(recording) => self.voice.recording = Some(recording),
                Err(why) => {
                    self.voice.error = Some(why);
                    return Task::none();
                }
            }
        }
        self.voice.listening = Listening::Recording;
        Task::none()
    }

    /// Stops recording and turns it into text, off the UI's thread.
    fn stop_listening(&mut self) -> Task<Message> {
        self.voice.listening = Listening::Transcribing;
        if let Some(heard) = self.voice.heard.clone() {
            return Task::perform(async move { Ok(heard) }, |result| {
                Message::Voice(VoiceEvent::Transcribed(result))
            });
        }
        let (Some(recording), Some(models)) = (self.voice.recording.take(), &self.voice.models)
        else {
            self.voice.listening = Listening::Idle;
            return Task::none();
        };
        let path = models.path(self.voice_model());
        let language = self.preferences.voice_language.clone();
        let transcriber = Arc::clone(&self.voice.transcriber);
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let heard = recording.finish()?;
                    let input = to_whisper_input(&heard.samples, heard.rate, heard.channels);
                    transcriber
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .transcribe(&path, &input, &language)
                })
                .await
                .map_err(|e| format!("Transcription stopped: {e}"))?
            },
            |result| Message::Voice(VoiceEvent::Transcribed(result)),
        )
    }

    /// Puts `text` in the composer at the cursor, spaced from what's there.
    fn insert_spoken(&mut self, text: &str) {
        let before = &self.composer.as_str()[..self.composer.offset()];
        let spaced = if before.is_empty() || before.ends_with(char::is_whitespace) {
            text.to_owned()
        } else {
            format!(" {text}")
        };
        self.composer.paste(&spaced);
    }

    /// Downloads the chosen speech model.
    fn download_voice(&mut self) -> Task<Message> {
        let Some(models) = &self.voice.models else {
            return Task::none();
        };
        if self.voice.download.is_some() {
            return Task::none();
        }
        let (task, handle) = Task::run(models.download(self.voice_model()), |step| {
            Message::Voice(VoiceEvent::Downloading(step))
        })
        .abortable();
        self.voice.download = Some(handle.abort_on_drop());
        self.voice.progress = Some(0.0);
        self.voice.download_error = None;
        task
    }
}
