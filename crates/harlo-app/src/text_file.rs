//! `TextFile`: one file loaded for the editor tab.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, path)]
        /// loading | text | binary | large | error | "" (nothing loaded)
        #[qproperty(QString, state)]
        /// The file's text as last loaded or saved.
        #[qproperty(QString, text)]
        #[qproperty(QString, message)]
        #[qproperty(bool, error)]
        /// A save is under way (a remote file's goes over SSH).
        #[qproperty(bool, saving)]
        type TextFile = super::TextFileRust;
    }

    unsafe extern "RustQt" {
        /// `path` was saved (at once here; a remote file once it's there).
        #[qsignal]
        #[cxx_name = "savedTo"]
        fn saved_to(self: Pin<&mut TextFile>, path: &QString);
    }

    impl cxx_qt::Threading for TextFile {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// Loads `path` in the background; `state` says how it went.
        #[qinvokable]
        fn load(self: Pin<&mut TextFile>, path: &QString);
        /// Saves `text` to `path` atomically; `savedTo` says when it's done.
        /// Returns false if it failed at once (see `message`).
        #[qinvokable]
        fn save(self: Pin<&mut TextFile>, text: &QString) -> bool;
        /// Compares the file on disk with what was loaded or saved:
        /// "same", "changed" or "deleted". A cheap stat; poll it. (A remote
        /// file's is asked for over SSH in the background: the answer is the
        /// last one known, a poll or two late.)
        #[qinvokable]
        fn disk_state(self: &TextFile) -> QString;
        /// Accepts the current file on disk as known (after "keep mine"),
        /// so `disk_state` reports "same" until it changes again.
        #[qinvokable]
        fn acknowledge_disk(self: Pin<&mut TextFile>);
    }
}

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use harlo_core::files::{self, FileContent};

use qobject::TextFile;

#[derive(Default)]
pub struct TextFileRust {
    path: QString,
    state: QString,
    text: QString,
    message: QString,
    error: bool,
    saving: bool,
    generation: u64,
    /// The file's stamp (modification time and size) as last loaded or saved.
    disk: Option<String>,
    /// A remote file's stamp now: the last one asked for, and whether a
    /// fresh one is being asked for.
    remote_now: Arc<Mutex<(Option<Option<String>>, bool)>>,
}

/// A file's modification time and size, here or (over SSH) on its host;
/// None when it isn't there.
fn stamp(path: &Path) -> Option<String> {
    if let Some((name, f)) = harlo_core::remote::split(path) {
        let host = harlo_core::remote::host(&name)?;
        let q = harlo_core::remote::quote(&f);
        let out = harlo_core::remote::run(&host, &format!("stat -c '%y:%s' -- {q} 2>/dev/null || stat -f '%Fm:%z' -- {q} 2>/dev/null"), Duration::from_secs(15)).ok()?;
        let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        return (!text.is_empty()).then_some(text);
    }
    let m = std::fs::metadata(path).ok()?;
    let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    Some(format!("{t}:{}", m.len()))
}

impl TextFile {
    pub fn load(mut self: Pin<&mut Self>, path: &QString) {
        self.as_mut().rust_mut().generation += 1;
        let generation = self.generation;
        self.as_mut().set_path(path.clone());
        self.as_mut().set_state(QString::from("loading"));
        self.as_mut().set_message(QString::default());
        self.as_mut().set_error(false);
        let full = PathBuf::from(path.to_string());
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            // Stat before reading: a write landing mid-read then shows up as a change.
            let disk = stamp(&full);
            let result = files::read_text(&full);
            let _ = thread.queue(move |mut this| {
                if this.generation != generation {
                    return;
                }
                let (state, text, msg) = match result {
                    Ok(FileContent::Text(t)) => ("text", t, String::new()),
                    Ok(FileContent::Binary) => (
                        "binary",
                        String::new(),
                        format!("{} contains binary data (a zero byte), so it can't be edited here; open it in another app.", full.display()),
                    ),
                    Ok(FileContent::TooLarge(n)) => ("large", String::new(), format!("This file is {} MB; too large to edit here.", n / (1024 * 1024))),
                    Err(e) => ("error", String::new(), format!("Could not open {}: {e}", full.display())),
                };
                *this.remote_now.lock().unwrap() = (Some(disk.clone()), false);
                this.as_mut().rust_mut().disk = disk;
                this.as_mut().set_text(QString::from(&text));
                this.as_mut().set_message(QString::from(&msg));
                this.as_mut().set_error(state == "error");
                this.as_mut().set_state(QString::from(state));
            });
        });
    }

    pub fn save(mut self: Pin<&mut Self>, text: &QString) -> bool {
        let path = PathBuf::from(self.path.to_string());
        let body = text.to_string();
        // A remote file: written over SSH off the UI thread.
        if harlo_core::remote::is_remote(&path) {
            if self.saving {
                return false;
            }
            self.as_mut().set_saving(true);
            let (thread, generation) = (self.qt_thread(), self.generation);
            crate::runtime().spawn_blocking(move || {
                let result = files::write_text(&path, &body).map(|()| stamp(&path));
                let _ = thread.queue(move |mut this| {
                    this.as_mut().set_saving(false);
                    if this.generation == generation {
                        this.as_mut().saved(&path, &body, result);
                    }
                });
            });
            return true;
        }
        let result = files::write_text(&path, &body).map(|()| stamp(&path));
        let ok = result.is_ok();
        self.saved(&path, &body, result);
        ok
    }

    /// A save's outcome: the new stamp and text, or why it failed.
    fn saved(mut self: Pin<&mut Self>, path: &Path, body: &str, result: std::io::Result<Option<String>>) {
        match result {
            Ok(disk) => {
                *self.remote_now.lock().unwrap() = (Some(disk.clone()), false);
                self.as_mut().rust_mut().disk = disk;
                self.as_mut().set_text(QString::from(body));
                self.as_mut().set_error(false);
                self.as_mut().set_message(QString::from(&format!("Saved {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())));
                let p = QString::from(&path.to_string_lossy().into_owned());
                self.saved_to(&p);
            }
            Err(e) => {
                self.as_mut().set_error(true);
                self.set_message(QString::from(&format!("Could not save: {e}")));
            }
        }
    }
}

impl TextFile {
    pub fn disk_state(&self) -> QString {
        let path = PathBuf::from(self.path.to_string());
        let now = if harlo_core::remote::is_remote(&path) {
            // The last answer from the host; a fresh one for the next poll.
            let mut cell = self.remote_now.lock().unwrap();
            if !cell.1 && !self.saving {
                cell.1 = true;
                let cell = Arc::clone(&self.remote_now);
                crate::runtime().spawn_blocking(move || {
                    let now = stamp(&path);
                    *cell.lock().unwrap() = (Some(now), false);
                });
            }
            match &cell.0 {
                Some(now) => now.clone(),
                None => return QString::from("same"),
            }
        } else {
            stamp(&path)
        };
        let state = match (now, &self.disk) {
            (None, Some(_)) => "deleted",
            (Some(now), Some(known)) if now != *known => "changed",
            _ => "same",
        };
        QString::from(state)
    }

    pub fn acknowledge_disk(mut self: Pin<&mut Self>) {
        let path = PathBuf::from(self.path.to_string());
        let now = if harlo_core::remote::is_remote(&path) { self.remote_now.lock().unwrap().0.clone().flatten() } else { stamp(&path) };
        self.as_mut().rust_mut().disk = now;
    }
}
