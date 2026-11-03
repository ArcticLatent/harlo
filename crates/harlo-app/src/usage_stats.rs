//! `UsageStats`: usage report across all sessions for the Stats view.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        /// Report as JSON (see `harlo_core::stats::report`).
        #[qproperty(QString, json)]
        #[qproperty(bool, loading)]
        /// Local time of the last refresh, e.g. "12:08 PM".
        #[qproperty(QString, updated)]
        type UsageStats = super::UsageStatsRust;
    }

    impl cxx_qt::Threading for UsageStats {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// Recomputes for the last `days` days (0 = all time), limited to
        /// sessions inside `roots` (empty = every folder).
        #[qinvokable]
        fn refresh(self: Pin<&mut UsageStats>, days: i32, roots: &QStringList);
    }
}

use std::path::PathBuf;
use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};

use qobject::UsageStats;

#[derive(Default)]
pub struct UsageStatsRust {
    json: QString,
    loading: bool,
    updated: QString,
    generation: u64,
}

impl UsageStats {
    pub fn refresh(mut self: Pin<&mut Self>, days: i32, roots: &QStringList) {
        self.as_mut().rust_mut().generation += 1;
        let generation = self.generation;
        let roots: Vec<PathBuf> = roots.iter().map(|r| PathBuf::from(r.to_string())).collect();
        self.as_mut().set_loading(true);
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let days = days.max(0) as u32;
            let mut files = harlo_core::stats::all_sessions();
            // This PC's at once; with other machines' once they're read.
            let show = |files: &[std::sync::Arc<harlo_core::stats::FileStats>], done: bool| {
                let report = harlo_core::stats::report(files, days, &roots).to_string();
                let updated = chrono::Local::now().format("%-I:%M %p").to_string();
                let _ = thread.queue(move |mut this| {
                    if this.generation != generation {
                        return;
                    }
                    this.as_mut().set_json(QString::from(&report));
                    this.as_mut().set_updated(QString::from(&updated));
                    if done {
                        this.as_mut().set_loading(false);
                    }
                });
            };
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(harlo_core::stats::remote_sessions(days));
            });
            // (Read already: one report, not one that jumps a moment later.)
            let remote = match rx.recv_timeout(std::time::Duration::from_millis(300)) {
                Ok(r) => r,
                Err(_) => {
                    show(&files, false);
                    rx.recv().unwrap_or_default()
                }
            };
            files.extend(remote);
            show(&files, true);
        });
    }
}
