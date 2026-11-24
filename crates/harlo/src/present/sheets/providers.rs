//! Settings → Providers: the ones OpenCode uses and how each is signed in,
//! the best-known ones to connect, and the page that signs in to one.

// Rust guideline compliant 2026-10-09

use harlo_core::server::{Connection, FieldKind, Integration, SignInMethod};
use lucide_icons::Icon;
use slint::{ModelRc, SharedString, VecModel};

use super::{LINK, PROJECT, glyph, heading, int, live_field, note, pair};
use crate::app::{App, Field, Load, SettingChoice, SignInAttempt};
use crate::ui::SheetRow;
use crate::window;

/// The best-known providers, by OpenCode's id, in the order Popular lists
/// the ones not yet connected, each with a line on what it offers and
/// whether it's recommended.
const POPULAR: [(&str, &str, bool); 8] = [
    (
        "opencode-go",
        "A low-cost subscription to open coding models.",
        true,
    ),
    (
        "opencode",
        "Hand-picked models, Claude, GPT and Gemini among them.",
        true,
    ),
    ("anthropic", "Claude models, with an API key.", false),
    (
        "github-copilot",
        "The models of a GitHub Copilot plan.",
        false,
    ),
    (
        "openai",
        "GPT models, with a ChatGPT plan or an API key.",
        false,
    ),
    ("google", "Gemini models, quick and structured.", false),
    (
        "openrouter",
        "Most providers' models, through one account.",
        false,
    ),
    (
        "vercel",
        "Many providers' models through one gateway.",
        false,
    ),
];

/// The providers OpenCode uses and how each is signed in, the best-known
/// ones not yet connected, and a button that lists them all.
pub(super) fn provider_rows(app: &App) -> Vec<SheetRow> {
    let page = &app.sheets.providers;
    let mut rows = vec![
        heading("Connected"),
        note("Sign-ins are shared by every app on this server."),
    ];
    let integrations = match &page.list {
        Load::Loaded(list) => list.as_slice(),
        _ => &[],
    };
    let providers = match &app.sheets.setup {
        Load::Loaded(setup) => setup.providers.clone().unwrap_or_default(),
        _ => Vec::new(),
    };
    // The providers in use, then signed-in integrations that aren't (yet).
    let mut shown: Vec<(String, String, bool)> = providers
        .iter()
        .map(|p| (p.id.clone(), p.name.clone(), p.enabled))
        .collect();
    for integration in integrations {
        let signed = integration
            .connections
            .iter()
            .any(|c| c.credential.is_some());
        if signed
            && integration.id != harlo_core::claude::INTEGRATION
            && !shown.iter().any(|(id, ..)| *id == integration.id)
        {
            shown.push((integration.id.clone(), integration.name.clone(), true));
        }
    }
    shown.sort_by_key(|(_, name, _)| name.to_lowercase());
    for (id, name, enabled) in &shown {
        let found = integrations.iter().position(|i| i.id == *id);
        let connections = found.map_or(&[][..], |at| integrations[at].connections.as_slice());
        let mut value = signed_in(connections);
        if !enabled {
            value.push_str(" · turned off");
        }
        let icon = if connections.is_empty() {
            Icon::SlidersHorizontal
        } else {
            Icon::KeyRound
        };
        rows.push(match found {
            Some(at) => SheetRow {
                kind: PROJECT,
                name: name.as_str().into(),
                value: value.into(),
                glyph: glyph(icon),
                setting: window::OPEN_PROVIDER,
                choice: int(at),
                ..SheetRow::default()
            },
            None => pair(name, &value, 3),
        });
    }
    match &page.list {
        Load::Failed(why) => rows.push(pair("Couldn't list providers", why, 2)),
        Load::Idle | Load::Loading if shown.is_empty() => rows.push(note("Loading…")),
        Load::Loaded(_) if shown.is_empty() => rows.push(note("None yet.")),
        _ => {}
    }
    let popular: Vec<SheetRow> = POPULAR
        .iter()
        .filter(|(id, ..)| !shown.iter().any(|(shown, ..)| shown == id))
        .filter_map(|&(id, what, recommended)| {
            let at = integrations.iter().position(|i| i.id == id)?;
            Some(SheetRow {
                kind: LINK,
                name: integrations[at].name.as_str().into(),
                value: what.into(),
                hint: if recommended { "Recommended" } else { "" }.into(),
                action: "Connect".into(),
                setting: window::OPEN_PROVIDER,
                choice: int(at),
                ..SheetRow::default()
            })
        })
        .collect();
    if !popular.is_empty() {
        rows.push(heading("Popular"));
        rows.extend(popular);
    }
    if matches!(page.list, Load::Loaded(_)) {
        let mut more = pair("", "", 3);
        more.action = "Show more providers".into();
        more.setting = window::ADD_PROVIDER;
        rows.push(more);
    }
    rows
}

/// How a provider is signed in, in words, from its connections.
fn signed_in(connections: &[Connection]) -> String {
    if connections.iter().any(|c| c.needs_sign_in.is_some()) {
        return "Needs signing in again".to_owned();
    }
    let ways: Vec<String> = connections.iter().map(connection_words).collect();
    if ways.is_empty() {
        "No sign-in needed".to_owned()
    } else {
        ways.join(" · ")
    }
}

/// One connection, in words.
fn connection_words(connection: &Connection) -> String {
    match connection.method.as_deref() {
        _ if connection.credential.is_none() => format!("From {}", connection.label),
        Some("key") => "Signed in with a key".to_owned(),
        Some("external") => "Signed in with the server's own credentials".to_owned(),
        _ => "Signed in".to_owned(),
    }
}

/// A provider's sign-in page: how it's signed in now, the ways to sign in,
/// the chosen way's form, and a sign-in that has started.
pub(super) fn sign_in_rows(app: &App) -> Vec<SheetRow> {
    let page = &app.sheets.providers;
    let Some(integration) = page.integration() else {
        let mut back = pair("Provider", "No longer listed by the server", 3);
        back.action = "All providers".into();
        back.setting = window::ALL_PROVIDERS;
        return vec![back];
    };
    let mut back = pair(&integration.name, "Signs in on the OpenCode server", 3);
    back.action = "All providers".into();
    back.setting = window::ALL_PROVIDERS;
    let mut rows = vec![back];
    if let Some(done) = &page.done {
        rows.push(pair("", done, 1));
    }
    if let Some(why) = &page.error {
        rows.push(pair("That didn't work", why, 2));
    }
    connection_rows(app, integration, &mut rows);

    let methods = page.methods();
    if methods.len() > 1 {
        rows.push(heading("Ways to sign in"));
        for (at, method) in &methods {
            let chosen = *at == page.method;
            let words = method_words(method);
            let mut row = if chosen {
                pair(&method_label(method), &format!("{words} · in use"), 1)
            } else {
                pair(&method_label(method), words, 3)
            };
            if !chosen {
                row.action = "Use this".into();
                row.setting = window::USE_SIGN_IN_METHOD;
                row.choice = int(*at);
            }
            rows.push(row);
        }
    }
    if let Some(method) = page.chosen().filter(|_| !methods.is_empty()) {
        rows.push(heading(if methods.len() > 1 {
            "Sign in"
        } else {
            match method {
                SignInMethod::Key { .. } => "Sign in with a key",
                _ => "Sign in",
            }
        }));
        method_rows(app, method, &mut rows);
    }
    let names: Vec<&str> = integration
        .methods
        .iter()
        .filter_map(|m| match m {
            SignInMethod::Env { names } => Some(names.iter().map(String::as_str)),
            _ => None,
        })
        .flatten()
        .collect();
    if !names.is_empty() {
        rows.push(note(&format!(
            "Or set {} where the OpenCode server runs.",
            names.join(" or ")
        )));
    }
    rows
}

/// How the provider is signed in now, each sign-in with Remove.
fn connection_rows(app: &App, integration: &Integration, rows: &mut Vec<SheetRow>) {
    let page = &app.sheets.providers;
    if integration.connections.is_empty() {
        return;
    }
    rows.push(heading("Signed in"));
    for (at, connection) in integration.connections.iter().enumerate() {
        let words = match &connection.needs_sign_in {
            Some(why) if why.is_empty() => "Needs signing in again".to_owned(),
            Some(why) => format!("Needs signing in again: {why}"),
            None => connection_words(connection),
        };
        let tone = if connection.needs_sign_in.is_some() {
            2
        } else {
            0
        };
        let Some(credential) = &connection.credential else {
            rows.push(pair(&connection.label, &words, tone));
            continue;
        };
        let asked = page.removing.as_deref() == Some(credential.as_str());
        let mut row = if asked {
            pair(
                &connection.label,
                "Remove it for every app using this server?",
                4,
            )
        } else {
            pair(&connection.label, &words, tone)
        };
        row.action = if asked { "Remove for good" } else { "Remove" }.into();
        row.setting = window::REMOVE_CREDENTIAL;
        row.risky = true;
        row.choice = int(at);
        rows.push(row);
    }
}

/// The chosen method's form, its key or started sign-in, and its button.
fn method_rows(app: &App, method: &SignInMethod, rows: &mut Vec<SheetRow>) {
    let page = &app.sheets.providers;
    rows.extend(form_rows(app));
    match method {
        SignInMethod::Key { label, .. } => {
            let mut key = live_field(
                Field::SignInKey,
                label.as_deref().unwrap_or("API key"),
                &page.key,
                "Paste the key",
            );
            key.on = true;
            rows.push(key);
        }
        SignInMethod::Command { command, .. } => {
            rows.push(pair("Runs", &command.join(" "), 3));
        }
        SignInMethod::OAuth { .. } | SignInMethod::External { .. } | SignInMethod::Env { .. } => {}
    }
    match &page.attempt {
        Some(SignInAttempt::OAuth(attempt)) => {
            if !attempt.instructions.is_empty() {
                rows.push(note(&attempt.instructions));
            }
            let mut url = pair("Page", &attempt.url, 0);
            url.action = "Open again".into();
            url.setting = window::OPEN_PROVIDER_PAGE;
            rows.push(url);
            if attempt.needs_code || attempt.paste {
                let (name, hint) = if attempt.needs_code {
                    ("Code", "Paste the code the page shows")
                } else {
                    (
                        "Address",
                        "If nothing happens, paste the address the browser landed on",
                    )
                };
                rows.push(live_field(Field::SignInCode, name, &page.code, hint));
                let mut finish = pair("", "", 3);
                finish.action = if page.busy { "Finishing…" } else { "Finish" }.into();
                finish.setting = window::FINISH_SIGN_IN;
                rows.push(finish);
            }
            rows.push(waiting_row());
        }
        Some(SignInAttempt::Command { said, .. }) => {
            if let Some(said) = said {
                rows.push(note(said));
            }
            rows.push(waiting_row());
        }
        None => {
            let mut go = pair("", "", 3);
            go.action = if page.busy {
                "Signing in…"
            } else {
                match method {
                    SignInMethod::Key { .. } | SignInMethod::External { .. } => "Sign in",
                    SignInMethod::OAuth { .. } => "Open the sign-in page",
                    SignInMethod::Command { .. } => "Run it",
                    SignInMethod::Env { .. } => "",
                }
            }
            .into();
            go.setting = window::SIGN_IN_PROVIDER;
            rows.push(go);
        }
    }
}

/// The row that waits on a started sign-in, with Cancel.
fn waiting_row() -> SheetRow {
    let mut row = pair("Waiting for the sign-in…", "", 3);
    row.action = "Cancel".into();
    row.setting = window::CANCEL_PROVIDER_SIGN_IN;
    row
}

/// The chosen method's form: a choice for a few options or yes or no, a
/// text field otherwise, each with its description.
fn form_rows(app: &App) -> Vec<SheetRow> {
    let page = &app.sheets.providers;
    let form = page.form();
    let mut rows = Vec::new();
    for (at, field) in form.fields.iter().enumerate() {
        if !page.answers.asks(&form, field) {
            continue;
        }
        let title = field.title.as_deref().unwrap_or(&field.key);
        let index = u8::try_from(at).unwrap_or_default();
        let choose = |options: Vec<SharedString>, chosen: Option<usize>| SheetRow {
            kind: 4,
            name: title.into(),
            setting: chosen.map_or(-1, int),
            options: ModelRc::new(VecModel::from(options)),
            choice: int(SettingChoice::ALL
                .iter()
                .position(|c| *c == SettingChoice::SignInAnswer(index))
                .unwrap_or_default()),
            ..SheetRow::default()
        };
        match &field.kind {
            FieldKind::Text {
                options,
                custom: false,
                ..
            } if !options.is_empty() && options.len() <= 4 => {
                let chosen = options
                    .iter()
                    .position(|o| page.answers.is_chosen(&field.key, &o.value));
                rows.push(choose(
                    options.iter().map(|o| o.label.as_str().into()).collect(),
                    chosen,
                ));
            }
            FieldKind::Boolean { .. } => {
                let chosen = page.answers.flag(&field.key).map(|yes| usize::from(!yes));
                rows.push(choose(vec!["Yes".into(), "No".into()], chosen));
            }
            FieldKind::Text {
                options,
                placeholder,
                ..
            } => {
                let typed = page.answers.typed(&field.key);
                let value = if typed.is_empty() {
                    options
                        .iter()
                        .find(|o| page.answers.is_chosen(&field.key, &o.value))
                        .map_or("", |o| o.value.as_str())
                } else {
                    typed
                };
                rows.push(live_field(
                    Field::SignInAnswer(index),
                    title,
                    value,
                    placeholder.as_deref().unwrap_or_default(),
                ));
                if !options.is_empty() {
                    let some: Vec<&str> = options.iter().map(|o| o.value.as_str()).collect();
                    rows.push(note(&format!("For example {}", some.join(", "))));
                }
            }
            FieldKind::Number { .. } => rows.push(live_field(
                Field::SignInAnswer(index),
                title,
                page.answers.typed(&field.key),
                "",
            )),
            FieldKind::Multiselect { .. } | FieldKind::External { .. } => {
                rows.push(pair(title, "Answer it with OpenCode's own sign-in", 3));
                continue;
            }
        }
        if let Some(about) = field.description.as_deref().filter(|d| !d.is_empty()) {
            rows.push(note(about));
        }
    }
    rows
}

/// A method's name, or a plain one when it has none.
fn method_label(method: &SignInMethod) -> String {
    match method {
        SignInMethod::Key { label, .. } => label.clone().unwrap_or_else(|| "API key".to_owned()),
        SignInMethod::OAuth { label, .. }
        | SignInMethod::Command { label, .. }
        | SignInMethod::External { label, .. } => label.clone(),
        SignInMethod::Env { .. } => "Environment variable".to_owned(),
    }
}

/// What a method does, in a few words.
fn method_words(method: &SignInMethod) -> &'static str {
    match method {
        SignInMethod::Key { .. } => "Paste a key",
        SignInMethod::OAuth { .. } => "In the browser",
        SignInMethod::Command { .. } => "A program on the server's computer",
        SignInMethod::External { .. } => "What's already on the server's computer",
        SignInMethod::Env { .. } => "Set where the server runs",
    }
}
