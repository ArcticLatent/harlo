//! Other computers, whose shared the server service Harlo follows over SSH:
//! adding one in Settings → Server follows it, and the location picker
//! offers it from then on.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;

use harlo_core::server::Location;
use harlo_core::ssh;

use super::{App, Message};
use crate::task::Task;

/// Something that happened to the other computers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComputerEvent {
    /// Forget computer `n`, and stop following it.
    Remove(usize),
    /// The hosts the SSH config names, read.
    Found(Vec<String>),
    /// Add the SSH config's host `n`, and follow it.
    Follow(usize),
    /// Bring every session over from OpenCode's service on this computer.
    ImportOpenCode,
    /// What bringing them over did.
    Imported(Result<harlo_core::import::Imported, String>),
}

impl App {
    /// Adds `target`, what follows `ssh` to reach a computer, and follows
    /// it.
    pub(super) fn add_computer(&mut self, target: &str) -> Task<Message> {
        let target = target.trim();
        self.sheets.computer_error = None;
        if let Err(why) = ssh::words(target) {
            self.sheets.computer_error = Some(why);
            return Task::none();
        }
        if !self.preferences.computers.iter().any(|t| t == target) {
            self.preferences.computers.push(target.to_owned());
        }
        let location = Location::Ssh {
            target: target.to_owned(),
        };
        if !self.locations.contains(&location) {
            self.locations.push(location.clone());
        }
        Task::batch([self.save_preferences(), self.pick_location(location)])
    }

    /// Reads the hosts the SSH config names, for Settings → Server to
    /// offer.
    pub(super) fn load_ssh_hosts(&self) -> Task<Message> {
        let Some(path) = self.ssh_config.clone() else {
            return Task::none();
        };
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || ssh::config_hosts(&path))
                    .await
                    .unwrap_or_default()
            },
            |hosts| Message::Sheet(super::SheetEvent::Computer(ComputerEvent::Found(hosts))),
        )
    }

    /// Handles `event` on the other computers.
    pub(super) fn computer_event(&mut self, event: ComputerEvent) -> Task<Message> {
        match event {
            ComputerEvent::Found(hosts) => {
                self.sheets.ssh_hosts = hosts;
                Task::none()
            }
            ComputerEvent::Follow(index) => match self.sheets.ssh_hosts.get(index).cloned() {
                Some(host) => self.add_computer(&host),
                None => Task::none(),
            },
            ComputerEvent::ImportOpenCode => {
                if self.sheets.import.as_deref() == Some(IMPORTING) {
                    return Task::none();
                }
                self.sheets.import = Some(IMPORTING.to_owned());
                Task::perform(harlo_core::import::from_opencode(None), |done| {
                    Message::Sheet(super::SheetEvent::Computer(ComputerEvent::Imported(done)))
                })
            }
            ComputerEvent::Imported(done) => {
                // Harlo's server announces each session that came over, so
                // the lists take them in as they come.
                self.sheets.import = Some(imported_text(&done));
                Task::none()
            }
            ComputerEvent::Remove(index) => {
                if index >= self.preferences.computers.len() {
                    return Task::none();
                }
                let target = self.preferences.computers.remove(index);
                let gone = Location::Ssh { target };
                self.locations.retain(|l| *l != gone);
                let mut tasks = vec![self.save_preferences()];
                // Back to the first in the picker: Harlo's server on this
                // computer, or the server Harlo was started with.
                if self.location == gone {
                    let first = self.locations.first().cloned().unwrap_or(Location::Local);
                    tasks.push(self.pick_location(first));
                }
                Task::batch(tasks)
            }
        }
    }
}

/// What Settings → Server says while sessions come over from OpenCode.
pub const IMPORTING: &str = "Bringing sessions over…";

/// What an import from OpenCode did, in a line.
#[must_use]
pub fn imported_text(done: &Result<harlo_core::import::Imported, String>) -> String {
    match done {
        Err(why) => format!("Not imported: {why}"),
        Ok(imported) => {
            let mut said = match imported.copied {
                0 => "Nothing new to bring over".to_owned(),
                1 => "Brought 1 session over".to_owned(),
                n => format!("Brought {n} sessions over"),
            };
            if imported.already > 0 {
                let _ = write!(said, "; {} already here", imported.already);
            }
            if let Some(first) = imported.failed.first() {
                let _ = write!(
                    said,
                    "; {} didn't come over ({first})",
                    imported.failed.len()
                );
            }
            said.push('.');
            said
        }
    }
}
