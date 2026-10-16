//! `Mcp` (QML singleton): Settings → MCP. Lists, saves and removes the MCP
//! servers pi and omp read (globally or in a project, here or on a remote
//! host), and tests them for a status (the harnesses don't report one).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Bumped when servers are saved or a test finishes.
        #[qproperty(i32, revision)]
        type Mcp = super::McpRust;
    }

    impl cxx_qt::Threading for Mcp {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// A scope ("global" or a project's path): `{ servers: [{ name,
        /// harnesses, differs, form }], errors, files: { omp, pi } }`.
        #[qinvokable]
        fn list_json(self: &Mcp, scope: &QString) -> QString;
        /// Saves a server (`form` JSON) for `harnesses` (JSON array of
        /// "omp" / "pi"); renames from `old_name`. An error, or "".
        #[qinvokable]
        fn save(self: Pin<&mut Mcp>, scope: &QString, old_name: &QString, form: &QString, harnesses: &QString) -> QString;
        #[qinvokable]
        fn remove(self: Pin<&mut Mcp>, scope: &QString, name: &QString) -> QString;
        /// Turns a server on or off (tests it when turned on). An error, or "".
        #[qinvokable]
        fn set_enabled(self: Pin<&mut Mcp>, scope: &QString, name: &QString, on: bool) -> QString;
        /// Starts testing a server (`statusJson` follows).
        #[qinvokable]
        fn test(self: Pin<&mut Mcp>, scope: &QString, name: &QString);
        /// Tests every enabled server of a scope.
        #[qinvokable]
        fn test_all(self: Pin<&mut Mcp>, scope: &QString);
        /// `{ name: { state, error, server, tools: [{ name, description }] } }`;
        /// state "testing", "connected", "failed" or "needs_auth".
        #[qinvokable]
        fn status_json(self: &Mcp, scope: &QString) -> QString;
        /// `{ forms: [...], error }` from a pasted snippet.
        #[qinvokable]
        fn parse_import(self: &Mcp, text: &QString) -> QString;
        /// A pasted command line as words (JSON array).
        #[qinvokable]
        fn split_command(self: &Mcp, line: &QString) -> QString;
        /// What to run in a terminal to sign in to a server with a harness.
        #[qinvokable]
        fn sign_in_command(self: &Mcp, harness: &QString, name: &QString) -> QString;
        /// The agent is installed (its path in Settings → Providers, else
        /// found): new servers are for the installed ones.
        #[qinvokable]
        fn installed(self: &Mcp, harness: &QString) -> bool;
        /// The agent is installed where a scope's servers run (here, or
        /// the host of a `host:` / remote project scope).
        #[qinvokable]
        fn installed_on(self: &Mcp, harness: &QString, scope: &QString) -> bool;
    }
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use harlo_core::mcp::{self, Form, Scope};
use harlo_core::mcp_probe::{self, Probe};
use harlo_harness::HarnessKind;
use serde_json::{Value, json};

use qobject::Mcp;

#[derive(Default)]
pub struct McpRust {
    revision: i32,
    /// Test results by (scope, server).
    status: HashMap<(String, String), Probe>,
}

fn form_json(f: &Form) -> Value {
    json!({
        "name": f.name, "enabled": f.enabled, "transport": f.transport, "command": f.command, "url": f.url,
        "headers": f.headers, "env": f.env, "cwd": f.cwd, "timeoutSecs": f.timeout_secs,
        "exposure": f.exposure, "description": f.description,
    })
}

fn pairs(v: &Value) -> Vec<(String, String)> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p[0].as_str()?.to_owned(), p[1].as_str().unwrap_or_default().to_owned())))
        .collect()
}

fn form_from(v: &Value) -> Form {
    Form {
        name: v["name"].as_str().unwrap_or_default().trim().to_owned(),
        enabled: v["enabled"].as_bool().unwrap_or(true),
        transport: v["transport"].as_str().unwrap_or("stdio").to_owned(),
        command: v["command"].as_array().into_iter().flatten().filter_map(|w| w.as_str()).map(str::to_owned).filter(|w| !w.is_empty()).collect(),
        url: v["url"].as_str().unwrap_or_default().trim().to_owned(),
        headers: pairs(&v["headers"]),
        env: pairs(&v["env"]),
        cwd: v["cwd"].as_str().unwrap_or_default().to_owned(),
        timeout_secs: v["timeoutSecs"].as_f64().filter(|t| *t > 0.0),
        exposure: v["exposure"].as_str().unwrap_or_default().to_owned(),
        description: v["description"].as_str().unwrap_or_default().to_owned(),
    }
}

fn kind_name(k: HarnessKind) -> &'static str {
    k.as_str()
}

/// An agent's program: the path set in Settings → Providers (if it's
/// there), else the installed one.
fn program(kind: HarnessKind) -> Option<PathBuf> {
    let set = harlo_core::db::Db::open(&harlo_core::db::default_path()).ok().and_then(|d| d.get(&format!("settings.{}_path", kind.as_str())).ok().flatten()).unwrap_or_default();
    if set.trim().is_empty() { harlo_harness::resolve_program(kind) } else { Some(PathBuf::from(set.trim())).filter(|p| p.is_file()) }
}

fn probe_json(p: &Probe) -> Value {
    json!({
        "state": p.state, "error": p.error, "server": p.server,
        "tools": p.tools.iter().map(|(n, d)| json!({ "name": n, "description": d })).collect::<Vec<_>>(),
    })
}

impl Mcp {
    fn bump(mut self: Pin<&mut Self>) {
        let next = self.revision + 1;
        self.as_mut().set_revision(next);
    }

    pub fn list_json(&self, scope: &QString) -> QString {
        let scope = Scope::parse(&scope.to_string());
        let listing = mcp::load(&scope);
        let servers: Vec<Value> = listing
            .servers
            .iter()
            .map(|s| json!({ "name": s.form.name, "harnesses": s.harnesses.iter().map(|k| kind_name(*k)).collect::<Vec<_>>(), "differs": s.differs, "form": form_json(&s.form) }))
            .collect();
        let files: serde_json::Map<String, Value> = mcp::HARNESSES.iter().map(|k| (kind_name(*k).to_owned(), json!(harlo_core::sidebar::tilde(&mcp::config_path(*k, &scope))))).collect();
        QString::from(&json!({ "servers": servers, "errors": listing.errors, "files": files }).to_string())
    }

    pub fn save(mut self: Pin<&mut Self>, scope: &QString, old_name: &QString, form: &QString, harnesses: &QString) -> QString {
        let scope_s = scope.to_string();
        let form = form_from(&serde_json::from_str(&form.to_string()).unwrap_or_default());
        let kinds: Vec<HarnessKind> = serde_json::from_str::<Vec<String>>(&harnesses.to_string())
            .unwrap_or_default()
            .iter()
            .filter_map(|h| HarnessKind::parse(h))
            .filter(|k| mcp::HARNESSES.contains(k))
            .collect();
        mcp::use_claude(program(HarnessKind::Claude));
        match mcp::save(&Scope::parse(&scope_s), &old_name.to_string(), &form, &kinds) {
            Ok(()) => {
                // A changed server's old test result no longer applies.
                let mut this = self.as_mut().rust_mut();
                this.status.remove(&(scope_s.clone(), old_name.to_string()));
                this.status.remove(&(scope_s, form.name.clone()));
                self.bump();
                QString::default()
            }
            Err(e) => QString::from(&format!("{e:#}")),
        }
    }

    pub fn remove(mut self: Pin<&mut Self>, scope: &QString, name: &QString) -> QString {
        mcp::use_claude(program(HarnessKind::Claude));
        match mcp::remove(&Scope::parse(&scope.to_string()), &name.to_string()) {
            Ok(()) => {
                self.as_mut().rust_mut().status.remove(&(scope.to_string(), name.to_string()));
                self.bump();
                QString::default()
            }
            Err(e) => QString::from(&format!("{e:#}")),
        }
    }

    pub fn set_enabled(mut self: Pin<&mut Self>, scope: &QString, name: &QString, on: bool) -> QString {
        match mcp::set_enabled(&Scope::parse(&scope.to_string()), &name.to_string(), on) {
            Ok(()) => {
                self.as_mut().rust_mut().status.remove(&(scope.to_string(), name.to_string()));
                if on {
                    self.as_mut().test(scope, name);
                }
                self.bump();
                QString::default()
            }
            Err(e) => QString::from(&format!("{e:#}")),
        }
    }

    pub fn test(self: Pin<&mut Self>, scope: &QString, name: &QString) {
        self.test_names(scope.to_string(), vec![name.to_string()]);
    }

    pub fn test_all(self: Pin<&mut Self>, scope: &QString) {
        let s = scope.to_string();
        let names: Vec<String> = mcp::load(&Scope::parse(&s)).servers.into_iter().filter(|l| l.form.enabled).map(|l| l.form.name).collect();
        self.test_names(s, names);
    }

    fn test_names(mut self: Pin<&mut Self>, scope_s: String, names: Vec<String>) {
        let scope = Scope::parse(&scope_s);
        let listing = mcp::load(&scope);
        // Where a stdio server runs: the project, a host's home (over
        // SSH), or here.
        let project: Option<PathBuf> = match &scope {
            Scope::Project(p) => Some(p.clone()),
            Scope::Host(h) => Some(mcp::host_home(h)),
            Scope::Global => None,
        };
        for name in names {
            let Some(server) = listing.servers.iter().find(|s| s.form.name == name) else { continue };
            let key = (scope_s.clone(), name.clone());
            if self.status.get(&key).is_some_and(|p| p.state == "testing") {
                continue;
            }
            self.as_mut().rust_mut().status.insert(key.clone(), Probe { state: "testing".into(), ..Default::default() });
            let form = server.form.clone();
            let project = project.clone();
            // At most the server's own timeout, else 30 s.
            let timeout = Duration::from_secs_f64(form.timeout_secs.unwrap_or(30.0).clamp(3.0, 30.0));
            let thread = self.qt_thread();
            crate::runtime().spawn_blocking(move || {
                let result = mcp_probe::probe(&form, project.as_deref(), timeout);
                let _ = thread.queue(move |mut this| {
                    this.as_mut().rust_mut().status.insert(key, result);
                    this.bump();
                });
            });
        }
        self.bump();
    }

    pub fn status_json(&self, scope: &QString) -> QString {
        let scope = scope.to_string();
        let map: serde_json::Map<String, Value> = self.status.iter().filter(|((s, _), _)| *s == scope).map(|((_, n), p)| (n.clone(), probe_json(p))).collect();
        QString::from(&Value::Object(map).to_string())
    }

    pub fn parse_import(&self, text: &QString) -> QString {
        let result = match mcp::parse_import(&text.to_string()) {
            Ok(forms) => json!({ "forms": forms.iter().map(form_json).collect::<Vec<_>>(), "error": "" }),
            Err(e) => json!({ "forms": [], "error": format!("{e:#}") }),
        };
        QString::from(&result.to_string())
    }

    pub fn split_command(&self, line: &QString) -> QString {
        QString::from(&Value::from(mcp::split_command(&line.to_string())).to_string())
    }

    pub fn installed(&self, harness: &QString) -> bool {
        HarnessKind::parse(&harness.to_string()).and_then(program).is_some()
    }

    pub fn installed_on(&self, harness: &QString, scope: &QString) -> bool {
        let scope = scope.to_string();
        let host = scope.strip_prefix("host:").map(str::to_owned).or_else(|| harlo_core::remote::host_of(std::path::Path::new(&scope)));
        match host {
            Some(h) => harlo_core::remote::found_on(&h, &harness.to_string()),
            None => self.installed(harness),
        }
    }

    pub fn sign_in_command(&self, harness: &QString, name: &QString) -> QString {
        let name = harlo_core::remote::quote(&name.to_string());
        QString::from(&match harness.to_string().as_str() {
            "pi" => format!("pi mcp login {name}"),
            // omp signs in from its own terminal UI.
            _ => "omp".to_owned(),
        })
    }
}
