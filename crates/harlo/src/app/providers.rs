//! Settings → Providers: the model providers the server is signed in to, signing
//! in to another (with a key, in a browser, by a program, or with what's
//! already on the server's computer), and removing a sign-in. The server keeps
//! the credentials, so a sign-in applies to every session on the server.

// Rust guideline compliant 2026-10-09

use std::time::Duration;

use futures_util::stream;
use harlo_core::approvals::Draft;
use harlo_core::server::{
    AttemptStatus, Error, Field, FieldKind, Form, Integration, OAuthAttempt, SignInMethod,
};

use crate::task::{Handle, Task};

use super::sheets::{Event as SheetEvent, Field as TextField, Load};
use super::{App, Message};

/// How often a sign-in that has started is asked how it stands.
const POLL: Duration = Duration::from_secs(2);

/// The most form fields a sign-in method shows; more are left out.
pub const FORM_FIELDS: usize = 8;

/// Something that happened in the providers part of Settings → Providers.
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderEvent {
    /// Every integration, as the server lists them, or why not.
    Loaded(Result<Vec<Integration>, String>),
    /// Open the menu of providers to sign in to, loading them again when
    /// the server listed none yet (it loads them after it starts).
    Pick,
    /// Show the page that signs in to an integration, by ID.
    Open(String),
    /// Show Models with that page, from elsewhere in Settings, such as an
    /// MCP server that signs in through an integration.
    OpenFrom(String),
    /// Back to the list.
    Back,
    /// Use the integration's sign-in method at this place in its list.
    Method(usize),
    /// Take option `.1` of the form field at `.0`.
    Choose(usize, usize),
    /// Start signing in with the chosen method.
    SignIn,
    /// Signed in, or why not.
    SignedIn(Result<(), String>),
    /// A browser sign-in started, or why not.
    OAuthStarted(Result<OAuthAttempt, String>),
    /// A sign-in program started, by attempt ID, or why not.
    CommandStarted(Result<String, String>),
    /// How the sign-in that started stands now.
    Status(AttemptStatus),
    /// Open the browser sign-in's page.
    OpenPage,
    /// Open one of the provider's own pages: where keys are made, or its
    /// prices.
    OpenLink(&'static str),
    /// Finish the browser sign-in with the code typed.
    Finish,
    /// Stop the sign-in that started.
    Cancel,
    /// Remove a stored credential, by ID: the first time asks again.
    Remove(String),
    /// A credential was removed, or why not.
    Removed(Result<(), String>),
}

/// A sign-in that has started and is waited on.
#[derive(Debug, Clone, PartialEq)]
pub enum Attempt {
    /// In a browser.
    OAuth(OAuthAttempt),
    /// By a program on the server's computer, with what it has said.
    Command {
        /// The attempt's ID.
        id: String,
        /// What the program has said so far.
        said: Option<String>,
    },
}

/// The providers part of Settings → Providers.
#[derive(Debug, Default)]
pub struct Providers {
    /// Every integration the server knows.
    pub list: Load<Vec<Integration>>,
    /// The integration whose sign-in page shows, by ID.
    pub shown: Option<String>,
    /// The sign-in method in use, by its place in the integration's list.
    pub method: usize,
    /// Answers to the method's form.
    pub answers: Draft,
    /// The key typed.
    pub key: String,
    /// The code or address typed to finish a browser sign-in.
    pub code: String,
    /// The sign-in that has started.
    pub attempt: Option<Attempt>,
    /// Whether a request is on its way.
    pub busy: bool,
    /// Why the last step failed.
    pub error: Option<String>,
    /// What the last sign-in did, such as `Signed in.`
    pub done: Option<String>,
    /// The credential whose Remove was pressed once.
    pub removing: Option<String>,
    /// Stops asking how the started sign-in stands.
    poll: Option<Handle>,
}

impl Providers {
    /// The integration with `id`, if it's loaded.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Integration> {
        match &self.list {
            Load::Loaded(list) => list.iter().find(|i| i.id == id),
            _ => None,
        }
    }

    /// The integration whose page shows.
    #[must_use]
    pub fn integration(&self) -> Option<&Integration> {
        self.shown.as_deref().and_then(|id| self.get(id))
    }

    /// The ways to sign in to the shown integration that Harlo can start,
    /// with their places in its list: every one but environment variables.
    #[must_use]
    pub fn methods(&self) -> Vec<(usize, &SignInMethod)> {
        self.integration().map_or_else(Vec::new, |integration| {
            integration
                .methods
                .iter()
                .enumerate()
                .filter(|(_, m)| !matches!(m, SignInMethod::Env { .. }))
                .collect()
        })
    }

    /// The method in use.
    #[must_use]
    pub fn chosen(&self) -> Option<&SignInMethod> {
        self.integration()?.methods.get(self.method)
    }

    /// The method's form, as the answers' form, with at most
    /// [`FORM_FIELDS`] fields.
    #[must_use]
    pub fn form(&self) -> Form {
        let fields = self.chosen().map_or(&[][..], SignInMethod::form);
        Form {
            id: String::new(),
            session_id: String::new(),
            title: String::new(),
            fields: fields.iter().take(FORM_FIELDS).cloned().collect(),
        }
    }

    /// Starts over on `method` of the shown integration.
    fn use_method(&mut self, method: usize) {
        self.stop_polling();
        self.method = method;
        self.answers = Draft::new(&self.form());
        self.key.clear();
        self.code.clear();
        self.attempt = None;
        self.error = None;
    }

    /// Stops asking how a started sign-in stands.
    fn stop_polling(&mut self) {
        if let Some(handle) = self.poll.take() {
            handle.abort();
        }
    }
}

impl App {
    /// Applies something that happened in the providers part of Settings.
    pub(super) fn provider_event(&mut self, event: ProviderEvent) -> Task<Message> {
        if matches!(event, ProviderEvent::Open(_)) {
            // Chosen from the menu, which closes.
            self.picker = None;
        }
        let page = &mut self.sheets.providers;
        match event {
            ProviderEvent::Loaded(list) => {
                page.list = list.map_or_else(Load::Failed, Load::Loaded);
            }
            ProviderEvent::Pick => {
                let empty = !matches!(&page.list, Load::Loaded(list) if !list.is_empty());
                let reload = if empty {
                    self.load_providers()
                } else {
                    Task::none()
                };
                let open =
                    self.picker_action(Message::TogglePicker(crate::model::Picker::Provider));
                return Task::batch([reload, open]);
            }
            ProviderEvent::OpenFrom(id) => {
                self.sheets.section = super::Section::Providers;
                self.sheets.query.clear();
                return self.provider_event(ProviderEvent::Open(id));
            }
            ProviderEvent::Open(id) => {
                page.shown = Some(id);
                page.done = None;
                page.removing = None;
                // Starts on the first way Harlo can start.
                let first = page.methods().first().map_or(0, |(at, _)| *at);
                page.use_method(first);
            }
            ProviderEvent::Back => {
                page.stop_polling();
                let cancel = self.cancel_attempt();
                let page = &mut self.sheets.providers;
                page.shown = None;
                page.attempt = None;
                page.error = None;
                page.done = None;
                return cancel;
            }
            ProviderEvent::Method(method) => {
                let cancel = self.cancel_attempt();
                self.sheets.providers.use_method(method);
                return cancel;
            }
            ProviderEvent::Choose(at, option) => {
                let form = page.form();
                if let Some(field) = form.fields.get(at) {
                    choose(&mut page.answers, field, option);
                }
            }
            ProviderEvent::SignIn => return self.sign_in_provider(),
            ProviderEvent::SignedIn(result) => {
                page.busy = false;
                page.stop_polling();
                page.attempt = None;
                return self.signed_in(result);
            }
            event @ (ProviderEvent::OAuthStarted(_)
            | ProviderEvent::CommandStarted(_)
            | ProviderEvent::Status(_)
            | ProviderEvent::OpenPage
            | ProviderEvent::Finish
            | ProviderEvent::Cancel) => return self.attempt_event(event),
            ProviderEvent::OpenLink(url) => return self.open_page(url.to_owned()),
            ProviderEvent::Remove(credential) => {
                if page.removing.as_deref() != Some(credential.as_str()) {
                    page.removing = Some(credential);
                    return Task::none();
                }
                return self.remove_credential(credential);
            }
            ProviderEvent::Removed(result) => {
                page.busy = false;
                page.removing = None;
                match result {
                    Ok(()) => {
                        page.done = Some("Removed.".to_owned());
                        self.catalogs.clear();
                        return self.load_setup();
                    }
                    Err(why) => page.error = Some(why),
                }
            }
        }
        Task::none()
    }

    /// Applies something about a sign-in that starts and is waited on.
    fn attempt_event(&mut self, event: ProviderEvent) -> Task<Message> {
        let page = &mut self.sheets.providers;
        match event {
            ProviderEvent::OAuthStarted(Ok(attempt)) => {
                page.busy = false;
                page.attempt = Some(Attempt::OAuth(attempt.clone()));
                let Some((client, id)) = self.client.clone().zip(page.shown.clone()) else {
                    return Task::none();
                };
                // The page opens at once; the address stays to open again.
                let open = self.open_page(attempt.url.clone());
                let poll = self.poll(move || {
                    let (client, id, attempt) = (client.clone(), id.clone(), attempt.id.clone());
                    async move { client.oauth_status(&id, &attempt).await }
                });
                return Task::batch([open, poll]);
            }
            ProviderEvent::CommandStarted(Ok(attempt)) => {
                page.busy = false;
                page.attempt = Some(Attempt::Command {
                    id: attempt.clone(),
                    said: None,
                });
                let Some((client, id)) = self.client.clone().zip(page.shown.clone()) else {
                    return Task::none();
                };
                return self.poll(move || {
                    let (client, id, attempt) = (client.clone(), id.clone(), attempt.clone());
                    async move { client.command_status(&id, &attempt).await }
                });
            }
            ProviderEvent::OAuthStarted(Err(why)) | ProviderEvent::CommandStarted(Err(why)) => {
                page.busy = false;
                page.error = Some(why);
            }
            ProviderEvent::Status(AttemptStatus::Pending(said)) => {
                page.busy = false;
                if let Some(Attempt::Command { said: was, .. }) = &mut page.attempt {
                    *was = said;
                }
            }
            ProviderEvent::Status(AttemptStatus::Complete) => {
                page.stop_polling();
                page.attempt = None;
                return self.signed_in(Ok(()));
            }
            ProviderEvent::Status(AttemptStatus::Failed(why)) => {
                page.stop_polling();
                page.attempt = None;
                page.error = Some(why);
            }
            ProviderEvent::OpenPage => {
                if let Some(Attempt::OAuth(attempt)) = &page.attempt {
                    let url = attempt.url.clone();
                    return self.open_page(url);
                }
            }
            ProviderEvent::Finish => return self.finish_oauth(),
            ProviderEvent::Cancel => {
                page.stop_polling();
                let cancel = self.cancel_attempt();
                let page = &mut self.sheets.providers;
                page.attempt = None;
                page.busy = false;
                return cancel;
            }
            _ => {}
        }
        Task::none()
    }

    /// Takes the text of a sign-in field, as it's typed.
    pub(super) fn provider_text(&mut self, field: TextField, value: &str) -> Task<Message> {
        let page = &mut self.sheets.providers;
        match field {
            TextField::SignInKey => value.clone_into(&mut page.key),
            TextField::SignInCode => value.clone_into(&mut page.code),
            TextField::SignInAnswer(at) => {
                let form = page.form();
                if let Some(field) = form.fields.get(usize::from(at)) {
                    page.answers.type_text(field, value.to_owned());
                }
            }
            _ => {}
        }
        Task::none()
    }

    /// Lists every integration the server knows.
    pub(super) fn load_providers(&mut self) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        if !matches!(self.sheets.providers.list, Load::Loaded(_)) {
            self.sheets.providers.list = Load::Loading;
        }
        Task::perform(
            async move { client.integrations().await.map_err(|e| said(&e)) },
            |list| provider_message(ProviderEvent::Loaded(list)),
        )
    }

    /// Signs in with the chosen method: at once for a key or what's on the
    /// server's computer; a browser sign-in or a program starts and is
    /// waited on.
    fn sign_in_provider(&mut self) -> Task<Message> {
        let page = &mut self.sheets.providers;
        let (Some(client), Some(id), Some(method)) = (
            self.client.clone(),
            page.shown.clone(),
            page.chosen().cloned(),
        ) else {
            return Task::none();
        };
        if page.busy || page.attempt.is_some() {
            return Task::none();
        }
        let answers = match page.answers.answers(&page.form()) {
            Ok(answers) => answers,
            Err(problems) => {
                let form = page.form();
                page.error = problems.first().map(|problem| {
                    let title = form
                        .fields
                        .iter()
                        .find(|f| f.key == problem.key)
                        .and_then(|f| f.title.clone())
                        .unwrap_or_else(|| problem.key.clone());
                    format!("{title}: {}", problem.text)
                });
                return Task::none();
            }
        };
        let key = page.key.trim().to_owned();
        if matches!(method, SignInMethod::Key { .. }) && key.is_empty() {
            page.error = Some("Paste the key first.".to_owned());
            return Task::none();
        }
        page.busy = true;
        page.error = None;
        page.done = None;
        match method {
            SignInMethod::Key { .. } => Task::perform(
                async move {
                    client
                        .connect_key(&id, &key, &answers)
                        .await
                        .map_err(|e| said(&e))
                },
                |result| provider_message(ProviderEvent::SignedIn(result)),
            ),
            SignInMethod::External { id: method, .. } => Task::perform(
                async move {
                    client
                        .connect_external_with(&id, &method, &answers)
                        .await
                        .map_err(|e| said(&e))
                },
                |result| provider_message(ProviderEvent::SignedIn(result)),
            ),
            SignInMethod::OAuth { id: method, .. } => Task::perform(
                async move {
                    client
                        .begin_oauth(&id, &method, &answers)
                        .await
                        .map_err(|e| said(&e))
                },
                |started| provider_message(ProviderEvent::OAuthStarted(started)),
            ),
            SignInMethod::Command { id: method, .. } => Task::perform(
                async move {
                    client
                        .begin_command(&id, &method)
                        .await
                        .map_err(|e| said(&e))
                },
                |started| provider_message(ProviderEvent::CommandStarted(started)),
            ),
            SignInMethod::Env { .. } => {
                self.sheets.providers.busy = false;
                Task::none()
            }
        }
    }

    /// Notes a finished sign-in and reloads what the server has.
    fn signed_in(&mut self, result: Result<(), String>) -> Task<Message> {
        let page = &mut self.sheets.providers;
        match result {
            Ok(()) => {
                page.key.clear();
                page.code.clear();
                page.error = None;
                page.done = Some("Signed in. Its models are in the model menus.".to_owned());
                // Reloaded for the current folder once this update ends.
                self.catalogs.clear();
                self.load_setup()
            }
            Err(why) => {
                page.error = Some(why);
                Task::none()
            }
        }
    }

    /// Asks how the started sign-in stands every [`POLL`], until it's done
    /// or fails.
    fn poll<F, Fut>(&mut self, status: F) -> Task<Message>
    where
        F: Fn() -> Fut + Send + 'static,
        Fut: Future<Output = Result<AttemptStatus, Error>> + Send + 'static,
    {
        let statuses = stream::unfold((status, false), |(status, done)| async move {
            if done {
                return None;
            }
            tokio::time::sleep(POLL).await;
            let now = status()
                .await
                .unwrap_or_else(|e| AttemptStatus::Failed(said(&e)));
            let done = !matches!(now, AttemptStatus::Pending(_));
            Some((provider_message(ProviderEvent::Status(now)), (status, done)))
        });
        let (task, handle) = Task::stream(statuses).abortable();
        let page = &mut self.sheets.providers;
        page.stop_polling();
        page.poll = Some(handle);
        task
    }

    /// Finishes the browser sign-in with the code or address typed.
    fn finish_oauth(&mut self) -> Task<Message> {
        let page = &mut self.sheets.providers;
        let code = page.code.trim().to_owned();
        if code.is_empty() {
            page.error = Some("Paste what the page gave you first.".to_owned());
            return Task::none();
        }
        let (Some(client), Some(id), Some(Attempt::OAuth(attempt))) = (
            self.client.clone(),
            page.shown.clone(),
            page.attempt.clone(),
        ) else {
            return Task::none();
        };
        page.busy = true;
        page.error = None;
        let needs_code = attempt.needs_code;
        Task::perform(
            async move {
                client
                    .complete_oauth(&id, &attempt.id, &code)
                    .await
                    .map_err(|e| said(&e))
            },
            move |result| match result {
                // An automatic sign-in ends through its status.
                Ok(()) if !needs_code => {
                    provider_message(ProviderEvent::Status(AttemptStatus::Pending(None)))
                }
                result => provider_message(ProviderEvent::SignedIn(result)),
            },
        )
    }

    /// Opens `url` in the browser, where the running app does.
    fn open_page(&self, url: String) -> Task<Message> {
        if !self.opens_pages {
            return Task::none();
        }
        Task::future(async move {
            // A page that can't be opened still shows its address.
            let _ = open::that_detached(&url);
        })
        .discard()
    }

    /// Tells the server to drop the sign-in that started, if one did.
    fn cancel_attempt(&self) -> Task<Message> {
        let page = &self.sheets.providers;
        let (Some(client), Some(id), Some(attempt)) = (
            self.client.clone(),
            page.shown.clone(),
            page.attempt.clone(),
        ) else {
            return Task::none();
        };
        Task::future(async move {
            let _ = match attempt {
                Attempt::OAuth(oauth) => client.cancel_oauth(&id, &oauth.id).await,
                Attempt::Command { id: attempt, .. } => client.cancel_command(&id, &attempt).await,
            };
        })
        .discard()
    }

    /// Removes stored credential `credential`.
    fn remove_credential(&mut self, credential: String) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        let page = &mut self.sheets.providers;
        page.busy = true;
        page.error = None;
        page.done = None;
        Task::perform(
            async move {
                client
                    .remove_credential(&credential)
                    .await
                    .map_err(|e| said(&e))
            },
            |result| provider_message(ProviderEvent::Removed(result)),
        )
    }
}

/// Takes option `option` of `field` into `answers`: one of its choices, or
/// yes (0) or no (1).
fn choose(answers: &mut Draft, field: &Field, option: usize) {
    match &field.kind {
        FieldKind::Text { options, .. } => {
            if let Some(choice) = options.get(option) {
                answers.choose(field, &choice.value);
            }
        }
        FieldKind::Boolean { .. } => answers.set_flag(field, option == 0),
        FieldKind::Number { .. } | FieldKind::Multiselect { .. } | FieldKind::External { .. } => {}
    }
}

/// What the server said about a refused step, or the whole error when it
/// said nothing.
fn said(error: &Error) -> String {
    error
        .server_message()
        .map_or_else(|| error.to_string(), ToOwned::to_owned)
}

/// `event` as the app's message.
pub(crate) fn provider_message(event: ProviderEvent) -> Message {
    Message::Sheet(SheetEvent::Provider(event))
}
