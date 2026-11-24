//! Settings → Local models: this computer's hardware, installing Ollama and
//! llama.cpp, running their servers while Harlo is open, and getting the
//! models that suit agents and fit.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use harlo_core::config::{self, LocalProvider};
use harlo_core::download::{Download, Fetch};
use harlo_core::local_models::{
    Computer, Engine, Engines, Install, Installed, ModelFiles, Release, Running, Serve, Suggestion,
    answers, base_url, ollama_models, ollama_pull, suggest,
};

use super::{App, Message};
use crate::task::{Handle, Task};

/// How long a server may take to answer after it starts; a large model
/// takes a while to load.
const START_WAIT: Duration = Duration::from_secs(180);

/// The provider ID llama.cpp's server has in OpenCode's config.
pub const LLAMACPP_PROVIDER: &str = "llamacpp";

/// Something that happened to local models.
#[derive(Debug, Clone)]
pub enum LocalEvent {
    /// Look at the computer, the engines and their servers again.
    Look,
    /// The computer's hardware was looked at, or why it couldn't be.
    Looked(Result<Box<Computer>, String>),
    /// The newest release of an engine, or why it couldn't be found.
    Latest(Engine, Result<Release, String>),
    /// Whether an engine's server answers.
    Answers(Engine, bool),
    /// Nothing answers on an engine's port: start its server.
    Free(Engine),
    /// Install or update an engine.
    Install(Engine),
    /// Stop installing it.
    StopInstall(Engine),
    /// A step of installing it.
    Installing(Engine, Install),
    /// Start an engine's server, or stop it when it runs.
    Server(Engine),
    /// The server Harlo started answered, or not in time.
    Started(Engine, bool),
    /// The models Ollama has.
    Pulled(Result<Vec<String>, String>),
    /// Get suggestion `n` for the chosen engine.
    Get(usize),
    /// Stop getting it.
    StopGet,
    /// llama.cpp's file for the model, found or not.
    Found(Result<Fetch, String>),
    /// A step of getting it.
    Getting(Download),
    /// Have llama.cpp serve suggestion `n`, restarting its server.
    Serve(usize),
    /// Add llama.cpp's server to OpenCode's config, or take it out.
    Provider(bool),
    /// OpenCode's config was changed, or why not.
    Saved(Result<String, String>),
}

/// How an engine's server stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ServerState {
    /// Not running.
    #[default]
    Stopped,
    /// Harlo started it; it hasn't answered yet.
    Starting,
    /// Harlo started it and it answers.
    Ready,
    /// It answers, but Harlo didn't start it.
    Elsewhere,
    /// It stopped, or never answered.
    Failed(String),
}

/// An engine's state.
#[derive(Debug, Default)]
pub struct EngineState {
    /// The engine as installed, by Harlo or on `PATH`.
    pub installed: Option<Installed>,
    /// Its newest release, once asked.
    pub latest: Option<Result<Release, String>>,
    /// How far installing got: the downloaded fraction, then `None` while
    /// unpacking.
    pub installing: Option<Option<f64>>,
    /// The install, which stops when dropped.
    install: Option<Handle>,
    /// Why the last install failed.
    pub error: Option<String>,
    /// Its server.
    pub server: ServerState,
    /// The server Harlo started, which stops when dropped.
    running: Option<Running>,
}

/// The model being got: which suggestion, and how far.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Getting {
    /// The suggestion.
    pub index: usize,
    /// The engine it's for.
    pub engine: Engine,
    /// How far it got, from 0 to 1.
    pub progress: f64,
}

/// Local models' state.
#[derive(Debug)]
pub struct LocalModels {
    /// Where engines are installed; `None` without a data folder.
    pub engines: Option<Engines>,
    /// Where llama.cpp's model files are kept.
    pub files: Option<ModelFiles>,
    /// The ports the servers listen on, Ollama's then llama.cpp's.
    pub ports: [u16; 2],
    /// The hardware, once looked at.
    pub computer: Option<Computer>,
    /// The suited models that fit it.
    pub suggestions: Vec<Suggestion>,
    /// Each engine, in [`Engine::ALL`]'s order.
    pub states: [EngineState; 2],
    /// The models Ollama has, by name.
    pub pulled: Vec<String>,
    /// llama.cpp's downloaded files, by repository.
    pub downloaded: HashMap<String, PathBuf>,
    /// The model being got.
    pub getting: Option<Getting>,
    /// The download or pull, which stops when dropped.
    get: Option<Handle>,
    /// Why the last download or pull failed.
    pub get_error: Option<String>,
    /// Whether OpenCode's config has llama.cpp's server.
    pub in_opencode: bool,
    /// What the last change to OpenCode's config did.
    pub done: Option<String>,
    /// Why it failed.
    pub error: Option<String>,
}

impl Default for LocalModels {
    fn default() -> Self {
        Self {
            engines: Engines::user(),
            files: ModelFiles::user(),
            ports: Engine::ALL.map(Engine::port),
            computer: None,
            suggestions: Vec::new(),
            states: [EngineState::default(), EngineState::default()],
            pulled: Vec::new(),
            downloaded: HashMap::new(),
            getting: None,
            get: None,
            get_error: None,
            in_opencode: false,
            done: None,
            error: None,
        }
    }
}

/// `engine`'s place in [`Engine::ALL`].
fn slot(engine: Engine) -> usize {
    match engine {
        Engine::Ollama => 0,
        Engine::LlamaCpp => 1,
    }
}

impl LocalModels {
    /// `engine`'s state.
    #[must_use]
    pub fn state(&self, engine: Engine) -> &EngineState {
        &self.states[slot(engine)]
    }

    /// `engine`'s state, to change.
    fn state_mut(&mut self, engine: Engine) -> &mut EngineState {
        &mut self.states[slot(engine)]
    }

    /// `engine`'s server address.
    #[must_use]
    pub fn base(&self, engine: Engine) -> String {
        base_url(self.ports[slot(engine)])
    }

    /// Whether `engine`'s server answers, started by Harlo or not.
    #[must_use]
    pub fn serving(&self, engine: Engine) -> bool {
        matches!(
            self.state(engine).server,
            ServerState::Ready | ServerState::Elsewhere
        )
    }

    /// Whether Ollama has `suggestion`.
    #[must_use]
    pub fn pulled(&self, suggestion: &Suggestion) -> bool {
        let tag = suggestion.ollama_tag();
        self.pulled
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&tag))
    }
}

impl App {
    /// The engine Settings gets models for.
    #[must_use]
    pub fn local_engine(&self) -> Engine {
        Engine::ALL
            .into_iter()
            .find(|e| e.id() == self.preferences.local_engine)
            .unwrap_or(Engine::Ollama)
    }

    /// Whether local models serve this server: only a server on this
    /// computer reaches them.
    #[must_use]
    pub fn local_models_apply(&self) -> bool {
        self.location.is_here()
    }

    /// The suggestion llama.cpp serves, if it's one of them.
    #[must_use]
    pub fn served(&self) -> Option<&Suggestion> {
        let path = PathBuf::from(&self.preferences.llamacpp_model);
        let local = &self.local;
        local
            .suggestions
            .iter()
            .find(|s| local.downloaded.get(&s.repo) == Some(&path))
    }

    /// Starts the servers left running when Harlo last closed.
    pub(super) fn start_local_servers(&mut self) -> Task<Message> {
        if !self.local_models_apply() {
            return Task::none();
        }
        let engines: Vec<Engine> = Engine::ALL
            .into_iter()
            .filter(|e| self.preferences.local_servers.iter().any(|id| id == e.id()))
            .collect();
        let local = &mut self.local;
        if let Some(home) = &local.engines {
            for engine in &engines {
                local.states[slot(*engine)].installed = home.installed(*engine);
            }
        }
        if let Some(files) = &local.files {
            local.downloaded = downloaded_files(files, &local.suggestions);
        }
        Task::batch(engines.into_iter().map(|e| self.check_then_start(e)))
    }

    /// Handles `event` on local models.
    pub(super) fn local_event(&mut self, event: LocalEvent) -> Task<Message> {
        match event {
            LocalEvent::Look => self.look(),
            LocalEvent::Looked(Err(why)) => {
                self.local.error = Some(why);
                Task::none()
            }
            LocalEvent::Looked(Ok(computer)) => {
                self.local.suggestions = suggest(&computer.specs);
                self.local.computer = Some(*computer);
                if let Some(files) = &self.local.files {
                    self.local.downloaded = downloaded_files(files, &self.local.suggestions);
                }
                self.fetch_latest()
            }
            LocalEvent::Latest(engine, release) => {
                self.local.state_mut(engine).latest = Some(release);
                Task::none()
            }
            LocalEvent::Answers(engine, answering) => {
                let state = self.local.state_mut(engine);
                if state.running.is_none() {
                    state.server = if answering {
                        ServerState::Elsewhere
                    } else {
                        ServerState::Stopped
                    };
                }
                self.after_server(engine)
            }
            LocalEvent::Install(engine) => self.install(engine),
            LocalEvent::StopInstall(engine) => {
                let state = self.local.state_mut(engine);
                state.install = None;
                state.installing = None;
                Task::none()
            }
            LocalEvent::Installing(engine, step) => self.installing(engine, step),
            LocalEvent::Server(engine) => {
                if self.local.state(engine).running.is_some() {
                    let state = self.local.state_mut(engine);
                    state.running = None;
                    state.server = ServerState::Stopped;
                    self.preferences
                        .local_servers
                        .retain(|id| id != engine.id());
                    self.save_preferences()
                } else {
                    self.check_then_start(engine)
                }
            }
            LocalEvent::Free(engine) => self.start_server(engine),
            LocalEvent::Started(engine, answered) => self.server_started(engine, answered),
            LocalEvent::Pulled(models) => {
                // An Ollama that stopped answering lists nothing.
                self.local.pulled = models.unwrap_or_default();
                Task::none()
            }
            LocalEvent::Get(index) => self.get(index),
            LocalEvent::StopGet => {
                self.local.get = None;
                self.local.getting = None;
                Task::none()
            }
            LocalEvent::Found(Ok(fetch)) => {
                if self.local.getting.is_none() {
                    return Task::none();
                }
                let (task, handle) = Task::run(fetch.download(), |step| {
                    Message::Local(LocalEvent::Getting(step))
                })
                .abortable();
                self.local.get = Some(handle.abort_on_drop());
                task
            }
            LocalEvent::Found(Err(why)) => {
                self.local.getting = None;
                self.local.get_error = Some(why);
                Task::none()
            }
            LocalEvent::Getting(step) => self.getting(step),
            LocalEvent::Serve(index) => self.serve(index),
            LocalEvent::Provider(add) => self.change_provider(add),
            LocalEvent::Saved(result) => {
                match result {
                    Ok(done) => {
                        self.local.done = Some(done);
                        self.local.error = None;
                    }
                    Err(why) => {
                        self.local.done = None;
                        self.local.error = Some(why);
                    }
                }
                self.local.in_opencode = self.global_config().is_some_and(|folder| {
                    config::has_provider(folder, LLAMACPP_PROVIDER).unwrap_or(false)
                });
                Task::none()
            }
        }
    }

    /// Looks at the hardware once, then at what's installed, running and
    /// newest each time Settings shows the section.
    pub(super) fn look(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        if self.local.computer.is_none() {
            tasks.push(Task::perform(
                async {
                    tokio::task::spawn_blocking(Computer::detect)
                        .await
                        .map(Box::new)
                        .map_err(|e| format!("Couldn't look at this computer: {e}"))
                },
                |computer| Message::Local(LocalEvent::Looked(computer)),
            ));
        } else {
            tasks.push(self.fetch_latest());
        }
        self.local.in_opencode = self
            .global_config()
            .is_some_and(|folder| config::has_provider(folder, LLAMACPP_PROVIDER).unwrap_or(false));
        if let Some(files) = &self.local.files {
            self.local.downloaded = downloaded_files(files, &self.local.suggestions);
        }
        for engine in Engine::ALL {
            if let Some(engines) = &self.local.engines {
                let installed = engines.installed(engine);
                self.local.state_mut(engine).installed = installed;
            }
            let port = self.local.ports[slot(engine)];
            tasks.push(Task::perform(answers(engine, port), move |up| {
                Message::Local(LocalEvent::Answers(engine, up))
            }));
        }
        Task::batch(tasks)
    }

    /// Asks GitHub for both engines' newest releases, for this computer's
    /// build.
    fn fetch_latest(&self) -> Task<Message> {
        let (Some(engines), Some(computer)) = (&self.local.engines, &self.local.computer) else {
            return Task::none();
        };
        let build = computer.build;
        Task::batch(Engine::ALL.map(|engine| {
            let engines = engines.clone();
            Task::perform(
                async move { engines.latest(engine, build).await },
                move |release| Message::Local(LocalEvent::Latest(engine, release)),
            )
        }))
    }

    /// Starts installing `engine`'s newest release.
    fn install(&mut self, engine: Engine) -> Task<Message> {
        let Some(engines) = &self.local.engines else {
            return Task::none();
        };
        let state = &self.local.states[slot(engine)];
        if state.install.is_some() {
            return Task::none();
        }
        let Some(Ok(release)) = state.latest.clone() else {
            let state = self.local.state_mut(engine);
            state.error = Some(format!(
                "Couldn't find {}'s newest release on GitHub.",
                engine.name()
            ));
            return Task::none();
        };
        let (task, handle) = Task::run(engines.install(engine, release), move |step| {
            Message::Local(LocalEvent::Installing(engine, step))
        })
        .abortable();
        let state = self.local.state_mut(engine);
        state.install = Some(handle.abort_on_drop());
        state.installing = Some(Some(0.0));
        state.error = None;
        task
    }

    /// Takes a step of installing `engine`.
    fn installing(&mut self, engine: Engine, step: Install) -> Task<Message> {
        let state = self.local.state_mut(engine);
        if state.install.is_none() {
            return Task::none();
        }
        match step {
            Install::Downloading(fraction) => state.installing = Some(Some(fraction)),
            Install::Unpacking => state.installing = Some(None),
            Install::Done(result) => {
                state.install = None;
                state.installing = None;
                match result {
                    Ok(installed) => {
                        state.installed = Some(installed);
                        // A running server keeps the old program until it's
                        // started again.
                        if state.running.is_some() {
                            state.running = None;
                            state.server = ServerState::Stopped;
                            return self.check_then_start(engine);
                        }
                    }
                    Err(why) => state.error = Some(why),
                }
            }
        }
        Task::none()
    }

    /// Starts `engine`'s server unless one already answers on its port.
    fn check_then_start(&mut self, engine: Engine) -> Task<Message> {
        let port = self.local.ports[slot(engine)];
        Task::perform(answers(engine, port), move |up| {
            Message::Local(if up {
                LocalEvent::Answers(engine, true)
            } else {
                LocalEvent::Free(engine)
            })
        })
    }

    /// What follows a server starting to answer or stopping: Ollama's
    /// models.
    fn after_server(&mut self, engine: Engine) -> Task<Message> {
        if engine != Engine::Ollama {
            return Task::none();
        }
        if !self.local.serving(Engine::Ollama) {
            self.local.pulled.clear();
            return Task::none();
        }
        let base = self.local.base(Engine::Ollama);
        Task::perform(async move { ollama_models(&base).await }, |models| {
            Message::Local(LocalEvent::Pulled(models))
        })
    }

    /// Runs `engine`'s server, and waits for it to answer.
    pub(super) fn start_server(&mut self, engine: Engine) -> Task<Message> {
        if self.local.state(engine).running.is_some() {
            return Task::none();
        }
        let Some(home) = &self.local.engines else {
            return Task::none();
        };
        let logs = home
            .folder()
            .parent()
            .map_or_else(|| home.folder().join("logs"), |data| data.join("logs"));
        let Some(installed) = self.local.state(engine).installed.clone() else {
            self.local.state_mut(engine).server =
                ServerState::Failed(format!("Install {} first.", engine.name()));
            return Task::none();
        };
        let context = self.served().map_or(8_192, |s| s.context);
        let model = (engine == Engine::LlamaCpp)
            .then(|| PathBuf::from(&self.preferences.llamacpp_model))
            .filter(|p| p.is_file());
        if engine == Engine::LlamaCpp && model.is_none() {
            self.local.state_mut(engine).server =
                ServerState::Failed("Get a model below and press Serve.".to_owned());
            return Task::none();
        }
        let port = self.local.ports[slot(engine)];
        let serve = Serve {
            engine,
            program: installed.program,
            port,
            model,
            context,
            models: None,
            log: logs.join(format!("{}.log", engine.id())),
        };
        let state = self.local.state_mut(engine);
        match serve.start() {
            Ok(running) => {
                state.running = Some(running);
                state.server = ServerState::Starting;
            }
            Err(why) => {
                state.server = ServerState::Failed(why);
                return Task::none();
            }
        }
        if !self
            .preferences
            .local_servers
            .iter()
            .any(|id| id == engine.id())
        {
            self.preferences.local_servers.push(engine.id().to_owned());
        }
        let wait = Task::perform(
            async move {
                let started = Instant::now();
                while started.elapsed() < START_WAIT {
                    if answers(engine, port).await {
                        return true;
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
                false
            },
            move |up| Message::Local(LocalEvent::Started(engine, up)),
        );
        Task::batch([wait, self.save_preferences()])
    }

    /// The server Harlo started answered, or didn't in time.
    fn server_started(&mut self, engine: Engine, answered: bool) -> Task<Message> {
        let state = self.local.state_mut(engine);
        if state.running.is_none() {
            // Stopped meanwhile.
            return Task::none();
        }
        if answered {
            state.server = ServerState::Ready;
        } else {
            let ended = state.running.as_mut().and_then(Running::ended);
            state.running = None;
            state.server = ServerState::Failed(match ended {
                Some(how) => format!("It stopped ({how}); its log is in Harlo's logs folder."),
                None => "It didn't answer in time; its log is in Harlo's logs folder.".to_owned(),
            });
        }
        self.after_server(engine)
    }

    /// Gets suggestion `index` for the chosen engine.
    fn get(&mut self, index: usize) -> Task<Message> {
        if self.local.getting.is_some() {
            return Task::none();
        }
        let Some(suggestion) = self.local.suggestions.get(index).cloned() else {
            return Task::none();
        };
        let engine = self.local_engine();
        self.local.get_error = None;
        match engine {
            Engine::Ollama => {
                if !self.local.serving(Engine::Ollama) {
                    self.local.get_error = Some("Start Ollama's server first.".to_owned());
                    return Task::none();
                }
                let base = self.local.base(Engine::Ollama);
                let (task, handle) =
                    Task::run(ollama_pull(&base, &suggestion.ollama_tag()), |step| {
                        Message::Local(LocalEvent::Getting(step))
                    })
                    .abortable();
                self.local.get = Some(handle.abort_on_drop());
                self.local.getting = Some(Getting {
                    index,
                    engine,
                    progress: 0.0,
                });
                task
            }
            Engine::LlamaCpp => {
                let Some(files) = self.local.files.clone() else {
                    return Task::none();
                };
                self.local.getting = Some(Getting {
                    index,
                    engine,
                    progress: 0.0,
                });
                Task::perform(async move { files.find(&suggestion).await }, |found| {
                    Message::Local(LocalEvent::Found(found))
                })
            }
        }
    }

    /// Takes a step of getting a model.
    fn getting(&mut self, step: Download) -> Task<Message> {
        let Some(getting) = &mut self.local.getting else {
            return Task::none();
        };
        match step {
            Download::Progress(fraction) => {
                getting.progress = fraction;
                Task::none()
            }
            Download::Done(result) => {
                let engine = getting.engine;
                self.local.getting = None;
                self.local.get = None;
                self.local.get_error = result.err();
                if let Some(files) = &self.local.files {
                    self.local.downloaded = downloaded_files(files, &self.local.suggestions);
                }
                self.after_server(engine)
            }
        }
    }

    /// Has llama.cpp serve suggestion `index`: remembers its file, starts
    /// the server again, and names it in OpenCode's config if it's there.
    fn serve(&mut self, index: usize) -> Task<Message> {
        let Some(suggestion) = self.local.suggestions.get(index) else {
            return Task::none();
        };
        let Some(path) = self.local.downloaded.get(&suggestion.repo) else {
            return Task::none();
        };
        self.preferences.llamacpp_model = path.display().to_string();
        let mut tasks = vec![self.save_preferences()];
        let state = self.local.state_mut(Engine::LlamaCpp);
        state.running = None;
        state.server = ServerState::Stopped;
        tasks.push(self.start_server(Engine::LlamaCpp));
        if self.local.in_opencode {
            tasks.push(self.change_provider(true));
        }
        Task::batch(tasks)
    }

    /// Adds llama.cpp's server, with the model it serves, to OpenCode's
    /// config; or takes it out.
    fn change_provider(&mut self, add: bool) -> Task<Message> {
        let Some(folder) = self.global_config().map(ToOwned::to_owned) else {
            return Task::none();
        };
        if !add {
            return Task::perform(
                config::remove_provider_later(folder, LLAMACPP_PROVIDER.to_owned()),
                |result| {
                    Message::Local(LocalEvent::Saved(
                        result
                            .map(|()| "Taken out of OpenCode's config.".to_owned())
                            .map_err(|e| e.to_string()),
                    ))
                },
            );
        }
        let Some(served) = self.served().cloned() else {
            self.local.error = Some("Choose the model llama.cpp serves first.".to_owned());
            return Task::none();
        };
        let provider = LocalProvider {
            id: LLAMACPP_PROVIDER.to_owned(),
            name: "llama.cpp on this computer".to_owned(),
            base_url: format!("{}/v1", self.local.base(Engine::LlamaCpp)),
            models: vec![(served.id(), served.title.clone(), served.context)],
        };
        Task::perform(
            config::set_provider_later(folder, provider),
            move |result| {
                Message::Local(LocalEvent::Saved(
                    result
                        .map(|()| format!("{} is in OpenCode's model menu.", served.title))
                        .map_err(|e| e.to_string()),
                ))
            },
        )
    }
}

/// llama.cpp's files on disk for `suggestions`, by repository.
fn downloaded_files(files: &ModelFiles, suggestions: &[Suggestion]) -> HashMap<String, PathBuf> {
    suggestions
        .iter()
        .filter_map(|s| Some((s.repo.clone(), files.downloaded(s)?)))
        .collect()
}
