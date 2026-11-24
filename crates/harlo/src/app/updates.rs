//! Harlo updating itself: a look for its newest release at start and every
//! few hours, a notification when one is out, and Settings → General's
//! Update, which downloads it, checks it against the release's signed
//! manifest and installs it, then Restart.
//!
//! Only a Harlo installed from a package or an AppImage updates; one built
//! from source never looks. After an update, the server on this computer
//! still runs the old version until the user restarts it from the same
//! place.

// Rust guideline compliant 2026-10-09

use std::time::Duration;

use futures_util::StreamExt as _;
use harlo_core::download::{Download, Fetch};
use harlo_core::updates::{self, InstallError, InstallKind, Release};

use super::{App, Message};
use crate::task::{Effect, Task};

/// How long after Harlo opens it first looks for an update.
const FIRST_LOOK: Duration = Duration::from_secs(6);

/// How often it looks again.
const LOOK_EVERY: Duration = Duration::from_hours(6);

/// How much of what's new Settings shows; the rest is on the releases page.
const NOTES_SHOWN: usize = 320;

/// This Harlo's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where Harlo's own update stands.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Phase {
    /// Not looked yet.
    #[default]
    Idle,
    /// Looking.
    Checking,
    /// This is the newest version.
    Current,
    /// A newer release is out, with what's new in it, in short.
    Found {
        /// The release.
        release: Release,
        /// What's new, as plain lines; empty until read.
        notes: String,
    },
    /// Downloading it: how far, from 0 to 1.
    Downloading(f64),
    /// The package manager is installing it.
    Installing,
    /// It's installed; restarting Harlo starts it.
    Installed(String),
    /// Looking, downloading or installing failed.
    Failed(InstallError),
}

/// Harlo's own updates.
#[derive(Debug, Clone, Default)]
pub struct Updates {
    /// How this Harlo was installed: only the running app sets it, so tests
    /// and a Harlo built from source never look.
    pub kind: Option<InstallKind>,
    /// Where the update stands.
    pub phase: Phase,
    /// Whether the last look was asked for with Check now, so its result
    /// shows even when nothing is new.
    asked: bool,
    /// The version being downloaded or installed.
    downloading: Option<String>,
    /// Where its download goes.
    file: Option<std::path::PathBuf>,
    /// Whether restarting the server is under way, or why it failed.
    pub server: ServerRestart,
}

/// Restarting the server on this computer after an update.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ServerRestart {
    /// Not asked for.
    #[default]
    Idle,
    /// Stopping it and starting it again.
    Restarting,
    /// It didn't restart, and why.
    Failed(String),
}

/// Something that happened to Harlo's update.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateEvent {
    /// Look for a newer release; `asked` when the user pressed Check now.
    Check {
        /// Pressed rather than timed.
        asked: bool,
    },
    /// The release found, or why none could be read.
    Checked(Result<Release, String>),
    /// What's new up to `version`, in short.
    Notes {
        /// The release the notes are for.
        version: String,
        /// The notes; empty when they couldn't be read.
        text: String,
    },
    /// Download and install the release found.
    Install,
    /// A step of the download.
    Downloaded(Download),
    /// The package manager finished, or why it failed.
    Installed(Result<(), InstallError>),
    /// Quit and start the installed Harlo.
    Restart,
    /// Stop the server on this computer and start the new one.
    RestartServer,
    /// The server restarted, or why not.
    ServerRestarted(Result<(), String>),
    /// The notification about a new release was clicked.
    NoticeClicked,
}

impl App {
    /// Whether Harlo looks for updates: installed from a package or an
    /// AppImage, and the switch is on.
    #[must_use]
    pub fn checks_updates(&self) -> bool {
        self.updates.kind.is_some() && self.preferences.check_updates
    }

    /// A look soon after start, then every few hours, while Harlo updates.
    pub(super) fn update_timer(&self) -> Task<Message> {
        if self.updates.kind.is_none() {
            return Task::none();
        }
        let looks = futures_util::stream::unfold(FIRST_LOOK, |wait| async move {
            tokio::time::sleep(wait).await;
            Some((
                Message::Update(UpdateEvent::Check { asked: false }),
                LOOK_EVERY,
            ))
        });
        Task::stream(looks)
    }

    /// Applies something that happened to Harlo's update.
    pub(super) fn update_event(&mut self, event: UpdateEvent) -> Task<Message> {
        match event {
            UpdateEvent::Check { asked } => self.look_for_update(asked),
            UpdateEvent::Checked(found) => self.found(found),
            UpdateEvent::Notes { version, text } => {
                if let Phase::Found { release, notes } = &mut self.updates.phase
                    && release.version == version
                {
                    *notes = updates::excerpt(&text, NOTES_SHOWN);
                }
                Task::none()
            }
            UpdateEvent::Install => self.download_update(),
            UpdateEvent::Downloaded(step) => self.downloaded(step),
            UpdateEvent::Installed(result) => {
                self.updates.phase = match result {
                    Ok(()) => Phase::Installed(self.found_version().unwrap_or_default()),
                    Err(e) => Phase::Failed(e),
                };
                Task::none()
            }
            UpdateEvent::Restart => {
                if let Some(program) = updates::this_program()
                    && let Err(e) = updates::relaunch_after_exit(&program)
                {
                    self.updates.phase = Phase::Failed(InstallError {
                        message: format!("Couldn't start the new Harlo: {e}"),
                        manual: None,
                    });
                    return Task::none();
                }
                Task::effect(Effect::Quit)
            }
            UpdateEvent::RestartServer => {
                self.updates.server = ServerRestart::Restarting;
                Task::perform(harlo_core::server::restart_local(), |result| {
                    Message::Update(UpdateEvent::ServerRestarted(result))
                })
            }
            UpdateEvent::ServerRestarted(result) => {
                self.updates.server = match result {
                    Ok(()) => ServerRestart::Idle,
                    Err(why) => ServerRestart::Failed(why),
                };
                Task::none()
            }
            UpdateEvent::NoticeClicked => Task::batch([
                Task::effect(Effect::Raise),
                self.update(Message::Sheet(super::SheetEvent::Section(
                    super::Section::General,
                ))),
            ]),
        }
    }

    /// The version of the release found or being installed.
    fn found_version(&self) -> Option<String> {
        match &self.updates.phase {
            Phase::Found { release, .. } => Some(release.version.clone()),
            _ => self.updates.downloading.clone(),
        }
    }

    /// Looks for a newer release, unless something's under way.
    fn look_for_update(&mut self, asked: bool) -> Task<Message> {
        if self.updates.kind.is_none() || !asked && !self.preferences.check_updates {
            return Task::none();
        }
        if matches!(
            self.updates.phase,
            Phase::Checking | Phase::Downloading(_) | Phase::Installing | Phase::Installed(_)
        ) {
            return Task::none();
        }
        self.updates.asked = asked;
        self.updates.phase = Phase::Checking;
        Task::perform(updates::latest(), |found| {
            Message::Update(UpdateEvent::Checked(found))
        })
    }

    /// Takes what a look found: a newer release, with its notes to read and
    /// a notification the first time it's seen; or nothing new.
    fn found(&mut self, found: Result<Release, String>) -> Task<Message> {
        let asked = std::mem::take(&mut self.updates.asked);
        let release = match found {
            Ok(release) => release,
            Err(message) => {
                // A failed timed look stays quiet: the next one tries again.
                self.updates.phase = if asked {
                    Phase::Failed(InstallError {
                        message,
                        manual: None,
                    })
                } else {
                    Phase::Idle
                };
                return Task::none();
            }
        };
        if !updates::newer(&release.version, VERSION) {
            self.updates.phase = Phase::Current;
            return Task::none();
        }
        let version = release.version.clone();
        self.updates.phase = Phase::Found {
            release,
            notes: String::new(),
        };
        let notes = {
            let version = version.clone();
            Task::perform(
                async move {
                    let text = updates::changes(VERSION, &version)
                        .await
                        .unwrap_or_default();
                    (version, text)
                },
                |(version, text)| Message::Update(UpdateEvent::Notes { version, text }),
            )
        };
        if self.preferences.update_announced == version {
            return notes;
        }
        self.preferences.update_announced.clone_from(&version);
        Task::batch([notes, self.save_preferences(), self.announce(&version)])
    }

    /// The notification that a new Harlo is out; a click opens Settings.
    fn announce(&self, version: &str) -> Task<Message> {
        if !self.notices.enabled {
            return Task::none();
        }
        let title = format!("Harlo {version} is out");
        let body = "Update it from Settings → General.".to_owned();
        let clicked = async move { super::notices::show(&title, &body).await };
        Task::stream(
            futures_util::stream::once(clicked).filter_map(|clicked| async move {
                clicked.then_some(Message::Update(UpdateEvent::NoticeClicked))
            }),
        )
    }

    /// Downloads the release found for how this Harlo was installed.
    fn download_update(&mut self) -> Task<Message> {
        let (Some(kind), Phase::Found { release, .. }) = (&self.updates.kind, &self.updates.phase)
        else {
            return Task::none();
        };
        let Some(asset) = release.asset(kind).cloned() else {
            self.updates.phase = Phase::Failed(InstallError {
                message: "This release has no file for this kind of install.".to_owned(),
                manual: None,
            });
            return Task::none();
        };
        // Only the file name: the manifest is signed, but a name is all it needs.
        let Some(name) = std::path::Path::new(&asset.name).file_name() else {
            self.updates.phase = Phase::Failed(InstallError {
                message: "The release's file has no name.".to_owned(),
                manual: None,
            });
            return Task::none();
        };
        let file = updates::folder().join(name);
        self.updates.downloading = Some(release.version.clone());
        self.updates.file = Some(file.clone());
        self.updates.phase = Phase::Downloading(0.0);
        let steps = Fetch {
            url: asset.download_url,
            path: file,
            size: 0,
            sha256: Some(asset.sha256),
        }
        .download();
        Task::stream(steps.map(|step| Message::Update(UpdateEvent::Downloaded(step))))
    }

    /// Takes a step of the download; once it's whole and checked, installs it.
    fn downloaded(&mut self, step: Download) -> Task<Message> {
        match step {
            Download::Progress(fraction) => {
                if matches!(self.updates.phase, Phase::Downloading(_)) {
                    self.updates.phase = Phase::Downloading(fraction);
                }
                Task::none()
            }
            Download::Done(Err(message)) => {
                self.updates.phase = Phase::Failed(InstallError {
                    message,
                    manual: None,
                });
                Task::none()
            }
            Download::Done(Ok(())) => {
                let (Some(kind), Some(file)) =
                    (self.updates.kind.clone(), self.updates.file.clone())
                else {
                    return Task::none();
                };
                self.updates.phase = Phase::Installing;
                Task::perform(
                    async move { updates::install(&kind, &file).await },
                    |result| Message::Update(UpdateEvent::Installed(result)),
                )
            }
        }
    }

    /// The version of the server on this computer, when it's older than
    /// this Harlo: an update installed while it ran.
    #[must_use]
    pub fn server_behind(&self) -> Option<String> {
        if !self.location.is_local() {
            return None;
        }
        let super::Status::Connected { version, .. } = &self.status else {
            return None;
        };
        updates::newer(VERSION, version).then(|| version.clone())
    }
}
