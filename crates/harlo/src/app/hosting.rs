//! Git hosting in Settings → Git and the setup guide: who is signed in to
//! GitHub, GitLab and Forgejo, signing in with a one-time code or a token,
//! and signing out. The tools and git keep the sign-ins (see
//! [`harlo_core::hosting`]).

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;

use harlo_core::hosting::{Account, Progress, Service, forgejo_base, normalize_host};

use crate::task::{Handle, Task};

use super::sheets::{Event as SheetEvent, Field, Load};
use super::{App, Message};

/// Something that happened to a hosting sign-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostingEvent {
    /// Who a service's tool says is signed in.
    Account(Service, Account),
    /// Sign in to GitHub or GitLab with a one-time code.
    SignIn(Service),
    /// How a sign-in is going.
    Progress(Service, Progress),
    /// Stop the sign-in going on.
    Cancel,
    /// Open the page the one-time code goes on.
    OpenPage,
    /// Sign out of a service.
    SignOut(Service),
    /// Signed out, or why not.
    SignedOut(Service, Result<(), String>),
    /// Signed in to a Forgejo host as an account, or why not.
    ForgejoSignedIn(Result<(String, String), String>),
}

/// A sign-in going on.
#[derive(Debug)]
pub struct SigningIn {
    /// The service.
    pub service: Service,
    /// The one-time code and the page to enter it on, once shown.
    pub code: Option<(String, String)>,
    /// Stops it.
    handle: Option<Handle>,
}

/// The hosting rows' state.
#[derive(Debug, Default)]
pub struct HostingState {
    /// Who is signed in to GitHub and GitLab.
    pub accounts: HashMap<Service, Load<Account>>,
    /// The sign-in going on.
    pub signing_in: Option<SigningIn>,
    /// What the last sign-in or sign-out did, by service: a message, or why
    /// it failed.
    pub outcome: Option<(Service, Result<String, String>)>,
    /// Whether a Forgejo token is being checked.
    pub checking_token: bool,
}

impl App {
    /// Applies something that happened to a hosting sign-in.
    pub(super) fn hosting_event(&mut self, event: HostingEvent) -> Task<Message> {
        let state = &mut self.sheets.hosting;
        match event {
            HostingEvent::Account(service, account) => {
                state.accounts.insert(service, Load::Loaded(account));
            }
            HostingEvent::SignIn(service) => return self.sign_in(service),
            HostingEvent::Progress(service, progress) => {
                let Some(going) = state.signing_in.as_mut().filter(|g| g.service == service) else {
                    return Task::none();
                };
                match progress {
                    Progress::Code { code, url } => going.code = Some((code, url)),
                    Progress::Done(result) => {
                        state.signing_in = None;
                        let host = self.host(service);
                        let state = &mut self.sheets.hosting;
                        if let Ok(name) = &result {
                            state
                                .accounts
                                .insert(service, Load::Loaded(Account::SignedIn(name.clone())));
                        }
                        state.outcome = Some((
                            service,
                            result.map(|name| format!("git is signed in to {host} as {name}.")),
                        ));
                    }
                }
            }
            HostingEvent::Cancel => {
                if let Some(handle) = state.signing_in.take().and_then(|g| g.handle) {
                    handle.abort();
                }
            }
            HostingEvent::OpenPage => {
                let Some((_, url)) = state.signing_in.as_ref().and_then(|g| g.code.clone()) else {
                    return Task::none();
                };
                return Task::future(async move {
                    // A page that can't be opened still shows its address.
                    let _ = open::that_detached(&url);
                })
                .discard();
            }
            HostingEvent::SignOut(service) => return self.sign_out(service),
            HostingEvent::SignedOut(service, result) => {
                state.outcome = Some((service, result.map(|()| "Signed out.".to_owned())));
                return self.load_account(service);
            }
            HostingEvent::ForgejoSignedIn(result) => {
                state.checking_token = false;
                let outcome = match result {
                    Ok((host, login)) => {
                        self.preferences.forgejo_account = Some(format!("{host} {login}"));
                        let saved = self.save_preferences();
                        self.sheets.hosting.outcome = Some((
                            Service::Forgejo,
                            Ok(format!("git is signed in to {host} as {login}.")),
                        ));
                        return saved;
                    }
                    Err(why) => Err(why),
                };
                state.outcome = Some((Service::Forgejo, outcome));
            }
        }
        Task::none()
    }

    /// Takes a hosting field: a server, or a Forgejo token to sign in with.
    pub(super) fn hosting_text(&mut self, field: Field, value: &str) -> Task<Message> {
        let value = value.trim();
        match field {
            Field::GitLabHost => {
                if self.preferences.gitlab_host == value {
                    return Task::none();
                }
                value.clone_into(&mut self.preferences.gitlab_host);
                Task::batch([self.save_preferences(), self.load_account(Service::GitLab)])
            }
            Field::ForgejoHost => {
                if self.preferences.forgejo_host == value {
                    return Task::none();
                }
                value.clone_into(&mut self.preferences.forgejo_host);
                self.save_preferences()
            }
            Field::ForgejoToken => self.forgejo_sign_in(value),
            _ => Task::none(),
        }
    }

    /// The host git signs in to for `service`: the one set, or its own.
    #[must_use]
    pub fn host(&self, service: Service) -> String {
        let typed = match service {
            Service::GitHub => "",
            Service::GitLab => &self.preferences.gitlab_host,
            Service::Forgejo => &self.preferences.forgejo_host,
        };
        let host = normalize_host(typed);
        if host.is_empty() {
            service.default_host().to_owned()
        } else {
            host
        }
    }

    /// The Forgejo account signed in here for the Forgejo host set now.
    #[must_use]
    pub fn forgejo_account(&self) -> Option<&str> {
        let host = self.host(Service::Forgejo);
        let (saved, account) = self
            .preferences
            .forgejo_account
            .as_deref()?
            .split_once(' ')?;
        (saved == host).then_some(account)
    }

    /// Asks the tools who is signed in to GitHub and GitLab.
    pub(super) fn load_accounts(&mut self) -> Task<Message> {
        Task::batch([
            self.load_account(Service::GitHub),
            self.load_account(Service::GitLab),
        ])
    }

    /// Asks `service`'s tool who is signed in.
    fn load_account(&mut self, service: Service) -> Task<Message> {
        let Some(hosting) = self.hosting.clone() else {
            return Task::none();
        };
        if service == Service::Forgejo {
            return Task::none();
        }
        if self.sheets.hosting.accounts.get(&service) == Some(&Load::Loading) {
            return Task::none();
        }
        let host = self.host(service);
        self.sheets.hosting.accounts.insert(service, Load::Loading);
        Task::perform(
            async move { hosting.account(service, &host).await },
            move |account| hosting_message(HostingEvent::Account(service, account)),
        )
    }

    /// Starts signing in to GitHub or GitLab with a one-time code.
    fn sign_in(&mut self, service: Service) -> Task<Message> {
        let Some(hosting) = self.hosting.clone() else {
            return Task::none();
        };
        if self.sheets.hosting.signing_in.is_some() {
            return Task::none();
        }
        let host = self.host(service);
        let (task, handle) = Task::run(hosting.sign_in(service, &host), move |progress| {
            hosting_message(HostingEvent::Progress(service, progress))
        })
        .abortable();
        self.sheets.hosting.outcome = None;
        self.sheets.hosting.signing_in = Some(SigningIn {
            service,
            code: None,
            handle: Some(handle),
        });
        task
    }

    /// Signs out of `service`.
    fn sign_out(&mut self, service: Service) -> Task<Message> {
        let Some(hosting) = self.hosting.clone() else {
            return Task::none();
        };
        self.sheets.hosting.outcome = None;
        if service == Service::Forgejo {
            let Some(account) = self.forgejo_account().map(str::to_owned) else {
                return Task::none();
            };
            let base = forgejo_base(&forgejo_typed(&self.preferences));
            self.preferences.forgejo_account = None;
            let saved = self.save_preferences();
            let forget = Task::perform(
                async move { hosting.forgejo_sign_out(&base, &account).await },
                |result| hosting_message(HostingEvent::SignedOut(Service::Forgejo, result)),
            );
            return Task::batch([saved, forget]);
        }
        let host = self.host(service);
        let account = match self.sheets.hosting.accounts.get(&service) {
            Some(Load::Loaded(Account::SignedIn(name))) => name.clone(),
            _ => String::new(),
        };
        Task::perform(
            async move { hosting.sign_out(service, &host, &account).await },
            move |result| hosting_message(HostingEvent::SignedOut(service, result)),
        )
    }

    /// Checks a Forgejo token with the server, then has git keep it.
    fn forgejo_sign_in(&mut self, token: &str) -> Task<Message> {
        let Some(hosting) = self.hosting.clone() else {
            return Task::none();
        };
        if token.is_empty() || self.sheets.hosting.checking_token {
            return Task::none();
        }
        let host = self.host(Service::Forgejo);
        let base = forgejo_base(&forgejo_typed(&self.preferences));
        let token = token.to_owned();
        self.sheets.hosting.checking_token = true;
        self.sheets.hosting.outcome = None;
        Task::perform(
            async move {
                hosting
                    .forgejo_sign_in(&base, &token)
                    .await
                    .map(|login| (host, login))
            },
            |result| hosting_message(HostingEvent::ForgejoSignedIn(result)),
        )
    }
}

/// The Forgejo server as typed, `http://` kept, or Codeberg.
fn forgejo_typed(preferences: &harlo_core::layout::Preferences) -> String {
    if normalize_host(&preferences.forgejo_host).is_empty() {
        Service::Forgejo.default_host().to_owned()
    } else {
        preferences.forgejo_host.clone()
    }
}

/// `event` as the app's message.
fn hosting_message(event: HostingEvent) -> Message {
    Message::Sheet(SheetEvent::Hosting(event))
}
