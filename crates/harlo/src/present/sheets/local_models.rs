//! Settings → Local models: this computer, the two engines with their
//! servers, and the models that fit.

// Rust guideline compliant 2026-10-09

use harlo_core::local_models::{Engine, Suggestion, newer};

use super::{choice, heading, int, note, pair};
use crate::app::{App, ServerState, SettingChoice};
use crate::ui::SheetRow;
use crate::window;

/// The section's rows.
pub(super) fn local_rows(app: &App) -> Vec<SheetRow> {
    let local = &app.local;
    let mut rows = vec![heading("This computer")];
    if !app.local_models_apply() {
        rows.push(note(
            "Harlo is following a server on another computer, which can't reach models run \
             here. Follow this computer's server to use them.",
        ));
        return rows;
    }
    match &local.computer {
        Some(computer) => {
            rows.push(pair("Graphics", &computer.gpu_words(), 3));
            rows.push(pair("Memory", &format!("{:.0} GB", computer.ram_gb), 3));
        }
        None => rows.push(pair("Graphics", "Looking…", 3)),
    }
    let Some(engines) = &local.engines else {
        rows.push(note(
            "This system has no folder for app data, so nothing can be installed.",
        ));
        return rows;
    };
    if let Some(why) = &local.error {
        rows.push(pair("Not done", why, 2));
    }
    rows.push(note(&format!(
        "Engines and models go in {}; their servers run while Harlo is open and listen only \
         on this computer.",
        engines
            .folder()
            .parent()
            .unwrap_or(engines.folder())
            .display()
    )));

    rows.push(heading("Ollama"));
    rows.push(note(
        "Gets models by name and serves all of them. OpenCode finds it on this computer by \
         itself, and its models show in the model menu.",
    ));
    engine_rows(app, Engine::Ollama, &mut rows);

    rows.push(heading("llama.cpp"));
    rows.push(note(
        "Serves one model file, the quickest way to run it on this computer. OpenCode uses it \
         once it's added to OpenCode's config.",
    ));
    engine_rows(app, Engine::LlamaCpp, &mut rows);
    provider_rows(app, &mut rows);

    model_rows(app, &mut rows);
    rows
}

/// `engine`'s install and server rows.
fn engine_rows(app: &App, engine: Engine, rows: &mut Vec<SheetRow>) {
    let state = app.local.state(engine);
    let item = int(Engine::ALL
        .iter()
        .position(|e| *e == engine)
        .unwrap_or_default());
    let mut row = match (&state.installing, &state.installed) {
        (Some(Some(fraction)), _) => {
            let mut row = pair(
                "Program",
                &format!("Downloading… {}%", percent(*fraction)),
                3,
            );
            row.action = "Stop".into();
            row.setting = window::STOP_ENGINE_INSTALL;
            row
        }
        (Some(None), _) => pair("Program", "Unpacking…", 3),
        (None, Some(installed)) if !installed.own => {
            pair("Program", "Outside Harlo, on this computer", 1)
        }
        (None, Some(installed)) => {
            let update = match &state.latest {
                Some(Ok(latest)) if newer(&latest.version, &installed.version) => {
                    Some(latest.version.clone())
                }
                _ => None,
            };
            match update {
                Some(version) => {
                    let mut row = pair(
                        "Program",
                        &format!("{}, and {version} is out", installed.version),
                        4,
                    );
                    row.action = "Update".into();
                    row.setting = window::INSTALL_ENGINE;
                    row
                }
                None => pair("Program", &installed.version, 1),
            }
        }
        (None, None) => {
            let mut row = pair("Program", "Not installed", 3);
            match &state.latest {
                Some(Ok(latest)) => {
                    let size: u64 = latest.assets.iter().map(|a| a.size).sum();
                    row.action = format!("Install {} MB", size.div_ceil(1_000_000)).into();
                    row.setting = window::INSTALL_ENGINE;
                }
                Some(Err(_)) => {}
                None => row.value = "Looking for the newest release…".into(),
            }
            row
        }
    };
    row.choice = item;
    rows.push(row);
    if let Some(Err(why)) = &state.latest {
        rows.push(pair("No release", why, 2));
    }
    if let Some(why) = &state.error {
        rows.push(pair("Couldn't install", why, 2));
    }

    if state.installed.is_some() || state.server == ServerState::Elsewhere {
        server_rows(app, engine, item, rows);
    }
}

/// `engine`'s server: how it stands, with Start or Stop.
fn server_rows(app: &App, engine: Engine, item: i32, rows: &mut Vec<SheetRow>) {
    let state = app.local.state(engine);
    let port = app.local.base(engine);
    let address = port.trim_start_matches("http://");
    let mut server = match &state.server {
        ServerState::Stopped => {
            let mut row = pair("Server", "Stopped", 3);
            row.action = "Start".into();
            row
        }
        ServerState::Starting => pair("Server", "Starting…", 3),
        ServerState::Ready => {
            let mut row = pair("Server", &format!("Running at {address}"), 1);
            row.action = "Stop".into();
            row
        }
        ServerState::Elsewhere => pair(
            "Server",
            &format!("Running at {address}, started outside Harlo"),
            1,
        ),
        ServerState::Failed(why) => {
            let mut row = pair("Server", why, 2);
            row.action = "Start".into();
            row
        }
    };
    if !server.action.is_empty() {
        server.setting = window::ENGINE_SERVER;
        server.choice = item;
    }
    if engine == Engine::LlamaCpp {
        if let Some(served) = app.served() {
            rows.push(pair("Serves", &served.title, 3));
        } else if state.server == ServerState::Stopped {
            server.value = "Stopped; get a model below and press Serve".into();
            server.action = "".into();
        }
    }
    rows.push(server);
}

/// Whether OpenCode's config has llama.cpp's server, with Add or Remove.
fn provider_rows(app: &App, rows: &mut Vec<SheetRow>) {
    let local = &app.local;
    if app.global_config().is_none() || app.served().is_none() {
        return;
    }
    let mut row = if local.in_opencode {
        pair("In OpenCode", "Added", 1)
    } else {
        pair("In OpenCode", "Not added", 3)
    };
    row.action = if local.in_opencode {
        "Take out"
    } else {
        "Add to OpenCode"
    }
    .into();
    row.setting = window::LOCAL_PROVIDER;
    row.choice = i32::from(!local.in_opencode);
    rows.push(row);
    if let Some(done) = &local.done {
        rows.push(pair("", done, 1));
    }
    rows.push(note(
        "This changes OpenCode's config, for every app using this server.",
    ));
}

/// The models that fit, for the chosen engine.
fn model_rows(app: &App, rows: &mut Vec<SheetRow>) {
    let local = &app.local;
    rows.push(heading("Models that fit"));
    rows.push(choice(
        app,
        SettingChoice::LocalEngine,
        "Get them for",
        "The engine that downloads and serves them",
    ));
    if local.computer.is_none() {
        return;
    }
    if local.suggestions.is_empty() {
        rows.push(note(
            "None of the models suited to agents fit this computer's memory.",
        ));
        return;
    }
    rows.push(note(
        "Models suited to agents' tools that fit this computer, the most capable first, each \
         at the largest quantization that fits. Tight ones leave little room for context.",
    ));
    let engine = app.local_engine();
    for (index, suggestion) in local.suggestions.iter().enumerate() {
        rows.push(suggestion_row(app, engine, index, suggestion));
    }
    if let Some(why) = &local.get_error {
        rows.push(pair("Not downloaded", why, 2));
    }
    rows.push(note(
        "Ready models show in the model menu, under the engine's name.",
    ));
}

/// A suggested model, with Get, its progress, or Serve.
fn suggestion_row(app: &App, engine: Engine, index: usize, s: &Suggestion) -> SheetRow {
    let local = &app.local;
    let about = format!("{} · {:.1} GB · {}", s.quant, s.memory_gb, s.fit.words());
    let mut row = pair(&s.title, &about, 3);
    row.choice = int(index);
    if let Some(getting) = local
        .getting
        .filter(|g| g.index == index && g.engine == engine)
    {
        row.value = format!("{about} · Downloading… {}%", percent(getting.progress)).into();
        row.action = "Stop".into();
        row.setting = window::STOP_LOCAL_MODEL;
        return row;
    }
    let ready = match engine {
        Engine::Ollama => local.pulled(s),
        Engine::LlamaCpp => local.downloaded.contains_key(&s.repo),
    };
    if !ready {
        row.action = "Get".into();
        row.setting = window::GET_LOCAL_MODEL;
        return row;
    }
    row.tone = 1;
    if engine == Engine::LlamaCpp {
        if app.served().is_some_and(|served| served.repo == s.repo) {
            row.value = format!("{about} · Served").into();
        } else {
            row.value = format!("{about} · Downloaded").into();
            row.action = "Serve".into();
            row.setting = window::SERVE_LOCAL_MODEL;
        }
    } else {
        row.value = format!("{about} · Ready").into();
    }
    row
}

/// A fraction from 0 to 1 as a whole percentage.
fn percent(fraction: f64) -> u32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a fraction from 0 to 1 as a whole percentage"
    )]
    let percent = (fraction.clamp(0.0, 1.0) * 100.0).round() as u32;
    percent
}
