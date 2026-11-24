//! Other computers, whose shared OpenCode service Harlo follows over SSH:
//! adding one in Settings → Server follows it, and the location picker
//! offers it from then on.

// Rust guideline compliant 2026-10-09

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
            ComputerEvent::Remove(index) => {
                if index >= self.preferences.computers.len() {
                    return Task::none();
                }
                let target = self.preferences.computers.remove(index);
                let location = Location::Ssh { target };
                self.locations.retain(|l| *l != location);
                let mut tasks = vec![self.save_preferences()];
                // Back to the first in the picker: this computer's service,
                // or the server Harlo was started with.
                if self.location == location {
                    let first = self.locations.first().cloned().unwrap_or(Location::Shared);
                    tasks.push(self.pick_location(first));
                }
                Task::batch(tasks)
            }
        }
    }
}
