//! `Dictation` (QML singleton): records the microphone and transcribes it
//! on this computer with the built-in whisper.cpp; the text goes into the
//! composer that asked. Also downloads the speech models.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// "idle", "recording" or "transcribing".
        #[qproperty(QString, state)]
        /// Why the last attempt failed ("" when it didn't).
        #[qproperty(QString, error)]
        /// The model being downloaded ("" when none) and how far (0–1).
        #[qproperty(QString, downloading)]
        #[qproperty(f64, download_progress, cxx_name = "downloadProgress")]
        #[qproperty(QString, download_error, cxx_name = "downloadError")]
        /// Bumped when a model finishes downloading (views re-check).
        #[qproperty(i32, models_revision, cxx_name = "modelsRevision")]
        type Dictation = super::DictationRust;
    }

    impl cxx_qt::Threading for Dictation {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// `[{ name, label, size, ready }]` as JSON.
        #[qinvokable]
        fn models_json(self: &Dictation) -> QString;
        #[qinvokable]
        fn model_ready(self: &Dictation, name: &QString) -> bool;
        /// Where models are kept.
        #[qinvokable]
        fn models_dir(self: &Dictation) -> QString;
        /// Starts recording for `target` (a session key).
        #[qinvokable]
        fn start(self: Pin<&mut Dictation>, target: &QString);
        /// Stops recording and transcribes with `model`; `transcribed`
        /// follows.
        #[qinvokable]
        fn stop(self: Pin<&mut Dictation>, model: &QString, language: &QString);
        /// Stops recording and drops it.
        #[qinvokable]
        fn cancel(self: Pin<&mut Dictation>);
        /// The microphone's loudness just now (0–1), while recording.
        #[qinvokable]
        fn level(self: &Dictation) -> f64;
        #[qinvokable]
        fn download(self: Pin<&mut Dictation>, name: &QString);
        #[qinvokable]
        fn cancel_download(self: Pin<&mut Dictation>);

        #[qsignal]
        fn transcribed(self: Pin<&mut Dictation>, text: &QString, target: &QString);
    }
}

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use harlo_core::dictation;

use qobject::Dictation;

pub struct DictationRust {
    state: QString,
    error: QString,
    downloading: QString,
    download_progress: f64,
    download_error: QString,
    models_revision: i32,
    target: String,
    cancel_download: Arc<AtomicBool>,
}

impl Default for DictationRust {
    fn default() -> Self {
        Self {
            state: QString::from("idle"),
            error: QString::default(),
            downloading: QString::default(),
            download_progress: 0.0,
            download_error: QString::default(),
            models_revision: 0,
            target: String::new(),
            cancel_download: Arc::default(),
        }
    }
}

/// Tests: `HARLO_TEST_DICTATION` is what the microphone "hears"; nothing is
/// recorded and whisper doesn't run.
fn test_speech() -> Option<String> {
    std::env::var("HARLO_TEST_DICTATION").ok()
}

impl Dictation {
    pub fn models_json(&self) -> QString {
        let list: Vec<serde_json::Value> = dictation::MODELS
            .iter()
            .map(|(name, _, size, label)| serde_json::json!({ "name": name, "label": label, "size": size, "ready": dictation::model_ready(name) }))
            .collect();
        QString::from(&serde_json::Value::from(list).to_string())
    }

    pub fn model_ready(&self, name: &QString) -> bool {
        dictation::model_ready(&name.to_string())
    }

    pub fn models_dir(&self) -> QString {
        QString::from(&dictation::models_dir().display().to_string())
    }

    pub fn start(mut self: Pin<&mut Self>, target: &QString) {
        if self.state.to_string() != "idle" {
            return;
        }
        self.as_mut().set_error(QString::default());
        if test_speech().is_none() {
            let error = crate::audio::audio_start();
            if !error.is_empty() {
                self.as_mut().set_error(error);
                return;
            }
        }
        self.as_mut().rust_mut().target = target.to_string();
        self.as_mut().set_state(QString::from("recording"));
    }

    pub fn stop(mut self: Pin<&mut Self>, model: &QString, language: &QString) {
        if self.state.to_string() != "recording" {
            return;
        }
        let (mut rate, mut channels) = (0, 0);
        let samples = if test_speech().is_some() { Vec::new() } else { crate::audio::audio_stop(&mut rate, &mut channels) };
        let (target, model, language) = (self.target.clone(), model.to_string(), language.to_string());
        self.as_mut().set_state(QString::from("transcribing"));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let result = match test_speech() {
                Some(text) if dictation::model_ready(&model) => {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    Ok(text)
                }
                Some(_) => Err("The speech model isn't downloaded yet.".to_owned()),
                None => {
                    let input = dictation::to_whisper_input(&samples, rate.max(0) as u32, channels.max(1) as u16);
                    dictation::transcribe(&model, &input, &language).map_err(|e| format!("{e:#}"))
                }
            };
            let _ = thread.queue(move |mut this| {
                // The text (or error) first: views finish up on "idle".
                match result {
                    Ok(text) if !text.is_empty() => this.as_mut().transcribed(&QString::from(&text), &QString::from(&target)),
                    Ok(_) => this.as_mut().set_error(QString::from("No speech heard.")),
                    Err(e) => this.as_mut().set_error(QString::from(&e)),
                }
                this.as_mut().set_state(QString::from("idle"));
            });
        });
    }

    pub fn cancel(mut self: Pin<&mut Self>) {
        if self.state.to_string() != "recording" {
            return;
        }
        if test_speech().is_none() {
            crate::audio::audio_cancel();
        }
        self.as_mut().set_state(QString::from("idle"));
    }

    pub fn level(&self) -> f64 {
        if self.state.to_string() != "recording" {
            0.0
        } else if test_speech().is_some() {
            0.5
        } else {
            crate::audio::audio_level() as f64
        }
    }

    pub fn download(mut self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        if !self.downloading.is_empty() || dictation::model_url(&name).is_none() {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.as_mut().rust_mut().cancel_download = cancel.clone();
        self.as_mut().set_download_error(QString::default());
        self.as_mut().set_download_progress(0.0);
        self.as_mut().set_downloading(QString::from(&name));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let progress_thread = thread.clone();
            let result = dictation::download(&name, &cancel, |fraction| {
                let _ = progress_thread.queue(move |mut this| this.as_mut().set_download_progress(fraction));
            });
            let cancelled = cancel.load(Ordering::Relaxed);
            let _ = thread.queue(move |mut this| {
                match result {
                    Ok(()) => {
                        let next = this.models_revision + 1;
                        this.as_mut().set_models_revision(next);
                    }
                    Err(_) if cancelled => {}
                    Err(e) => this.as_mut().set_download_error(QString::from(&format!("Download failed: {e:#}"))),
                }
                this.as_mut().set_downloading(QString::default());
            });
        });
    }

    pub fn cancel_download(self: Pin<&mut Self>) {
        self.cancel_download.store(true, Ordering::Relaxed);
    }
}
