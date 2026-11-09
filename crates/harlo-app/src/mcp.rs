//! `Mcp` (QML singleton): Settings → MCP. Lists, saves and removes the MCP
//! servers Harlo and Claude read (globally or in a project, here or on a remote
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
        /// harnesses, differs, form }], errors, files: { harlo, claude } }`.
        #[qinvokable]
        fn list_json(self: &Mcp, scope: &QString) -> QString;
        /// Saves a server (`form` JSON) for `harnesses` (JSON array of
        /// "harlo" / "claude"); renames from `old_name`. An error, or "".
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
        /// The agent is installed (its path in Settings → Providers, else
        /// found): new servers are for the installed ones.
        #[qinvokable]
        fn installed(self: &Mcp, harness: &QString) -> bool;
        /// The agent is installed where a scope's servers run (here, or
        /// the host of a `host:` / remote project scope).
        #[qinvokable]
        fn installed_on(self: &Mcp, harness: &QString, scope: &QString) -> bool;
        /// The servers harlo installs with a click: `[{ id, name, about,
        /// license, repo, version, agents, chosen, taken, needs, node,
        /// nodeError, latest, busy, progress, message, error }]` (as last
        /// looked: `catalog_refresh` looks again).
        #[qinvokable]
        fn catalog_json(self: &Mcp) -> QString;
        /// Looks at them again, and for a newer release (at most hourly
        /// unless `force`).
        #[qinvokable]
        fn catalog_refresh(self: Pin<&mut Mcp>, force: bool);
        /// Turns one on for `harnesses` (JSON array; downloading it first if
        /// it isn't yet), or off (`harnesses`: chosen for next time).
        #[qinvokable]
        fn catalog_set(self: Pin<&mut Mcp>, id: &QString, on: bool, harnesses: &QString);
        /// Updates one to its newest release.
        #[qinvokable]
        fn catalog_update(self: Pin<&mut Mcp>, id: &QString);
    }
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use harlo_core::mcp_catalog;
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
    /// The servers harlo offers, as last looked, with what's being done to them,
    /// by id.
    catalog: serde_json::Map<String, Value>,
    /// When a newer release was last looked for.
    catalog_checked: Option<std::time::Instant>,
}

fn form_json(f: &Form) -> Value {
    json!({
        "name": f.name, "enabled": f.enabled, "transport": f.transport, "command": f.command, "url": f.url,
        "headers": f.headers, "env": f.env, "cwd": f.cwd, "timeoutSecs": f.timeout_secs,
        "readOnlyAll": f.read_only_all, "readOnly": f.read_only,
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
        read_only_all: v["readOnlyAll"].as_bool().unwrap_or(false),
        read_only: v["readOnly"].as_array().into_iter().flatten().filter_map(|t| t.as_str()).map(str::to_owned).collect(),
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
        "tools": p.tools.iter().map(|(n, d)| json!({ "name": n, "description": d, "looks": p.looks.contains(n) })).collect::<Vec<_>>(),
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
            // (harlo's own: under Ready to install.)
            .filter(|s| scope != Scope::Global || mcp_catalog::which(&s.form).is_none())
            .map(|s| {
                // (A project's server Harlo won't start yet: not saved here.)
                let unapproved = s.harnesses.contains(&HarnessKind::Harlo) && mcp::harlo_approved(&scope, &s.form.name) == Some(false);
                json!({ "name": s.form.name, "harnesses": s.harnesses.iter().map(|k| kind_name(*k)).collect::<Vec<_>>(), "differs": s.differs, "harloUnapproved": unapproved, "form": form_json(&s.form) })
            })
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
            let mut form = server.form.clone();
            // Harlo's sign-in, when it has one here (a host keeps its own).
            let here = match &scope {
                Scope::Global => true,
                Scope::Project(p) => harlo_core::remote::split(p).is_none(),
                Scope::Host(_) => false,
            };
            let harlo = here && server.harnesses.contains(&HarnessKind::Harlo) && form.transport != "stdio";
            let project = project.clone();
            // At most the server's own timeout, else 30 s.
            let timeout = Duration::from_secs_f64(form.timeout_secs.unwrap_or(30.0).clamp(3.0, 30.0));
            let thread = self.qt_thread();
            crate::runtime().spawn_blocking(move || {
                if harlo
                    && !form.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("authorization"))
                    && let Some(t) = mcp_probe::harlo_token(&mcp_probe::expand(form.url.trim()))
                {
                    form.headers.push(("Authorization".into(), format!("Bearer {t}")));
                }
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

    pub fn catalog_json(&self) -> QString {
        let list: Vec<Value> = mcp_catalog::OFFERED
            .iter()
            .map(|b| self.catalog.get(b.id).cloned().unwrap_or_else(|| json!({ "id": b.id, "name": b.name, "about": b.about, "license": b.license, "repo": b.repo, "loading": true })))
            .collect();
        QString::from(&Value::from(list).to_string())
    }

    /// Changes the fields of one's entry.
    fn catalog_put(mut self: Pin<&mut Self>, id: &str, fields: Value) {
        let mut this = self.as_mut().rust_mut();
        let entry = this.catalog.entry(id.to_owned()).or_insert_with(|| json!({}));
        if let (Some(e), Some(f)) = (entry.as_object_mut(), fields.as_object()) {
            for (k, v) in f {
                e.insert(k.clone(), v.clone());
            }
        }
        self.bump();
    }

    pub fn catalog_refresh(mut self: Pin<&mut Self>, force: bool) {
        let look_up = force || self.catalog_checked.is_none_or(|t| t.elapsed() > Duration::from_secs(3600));
        if look_up {
            self.as_mut().rust_mut().catalog_checked = Some(std::time::Instant::now());
        }
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            for b in mcp_catalog::OFFERED {
                let mut st = mcp_catalog::status(&b);
                st["loading"] = json!(false);
                let _ = thread.queue(move |this| this.catalog_put(b.id, st));
            }
            if look_up {
                for b in mcp_catalog::OFFERED {
                    // (Offline, or GitHub's limit: nothing to say.)
                    if let Ok((latest, _)) = mcp_catalog::latest(&b) {
                        let _ = thread.queue(move |this| this.catalog_put(b.id, json!({ "latest": latest })));
                    }
                    if let Ok(stars) = mcp_catalog::stars(&b) {
                        let _ = thread.queue(move |this| this.catalog_put(b.id, json!({ "stars": stars })));
                    }
                }
            }
        });
    }

    pub fn catalog_set(mut self: Pin<&mut Self>, id: &QString, on: bool, harnesses: &QString) {
        let Some(b) = mcp_catalog::get(&id.to_string()) else { return };
        let kinds: Vec<HarnessKind> = serde_json::from_str::<Vec<String>>(&harnesses.to_string()).unwrap_or_default().iter().filter_map(|h| HarnessKind::parse(h)).filter(|k| mcp::HARNESSES.contains(k)).collect();
        self.as_mut().catalog_put(b.id, json!({ "busy": if on { "on" } else { "off" }, "progress": 0.0, "error": "", "message": "" }));
        mcp::use_claude(program(HarnessKind::Claude));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let mut shown = std::time::Instant::now();
            let t = thread.clone();
            let mut progress = |done: u64, total: u64| {
                if total > 0 && shown.elapsed() > Duration::from_millis(150) {
                    shown = std::time::Instant::now();
                    let p = done as f64 / total as f64;
                    let _ = t.queue(move |this| this.catalog_put(b.id, json!({ "progress": p })));
                }
            };
            let result = if on { mcp_catalog::enable(&b, &kinds, &mut progress) } else { mcp_catalog::disable(&b, &kinds) };
            let mut st = mcp_catalog::status(&b);
            st["busy"] = json!("");
            st["error"] = json!(result.err().unwrap_or_default());
            let _ = thread.queue(move |mut this| {
                let tested = on && st["error"] == "";
                this.as_mut().rust_mut().status.remove(&("global".to_owned(), b.id.to_owned()));
                this.as_mut().catalog_put(b.id, st);
                if tested {
                    this.as_mut().test(&QString::from("global"), &QString::from(b.id));
                }
            });
        });
    }

    pub fn catalog_update(mut self: Pin<&mut Self>, id: &QString) {
        let Some(b) = mcp_catalog::get(&id.to_string()) else { return };
        self.as_mut().catalog_put(b.id, json!({ "busy": "update", "progress": 0.0, "error": "", "message": "" }));
        let thread = self.qt_thread();
        crate::runtime().spawn_blocking(move || {
            let mut shown = std::time::Instant::now();
            let t = thread.clone();
            let result = mcp_catalog::update(&b, &mut |done, total| {
                if total > 0 && shown.elapsed() > Duration::from_millis(150) {
                    shown = std::time::Instant::now();
                    let p = done as f64 / total as f64;
                    let _ = t.queue(move |this| this.catalog_put(b.id, json!({ "progress": p })));
                }
            });
            let mut st = mcp_catalog::status(&b);
            st["busy"] = json!("");
            match result {
                Ok(m) => st["message"] = json!(m),
                Err(e) => st["error"] = json!(e),
            }
            let _ = thread.queue(move |mut this| {
                let on = st["agents"].as_array().is_some_and(|a| !a.is_empty());
                this.as_mut().catalog_put(b.id, st);
                if on {
                    this.as_mut().rust_mut().status.remove(&("global".to_owned(), b.id.to_owned()));
                    this.as_mut().test(&QString::from("global"), &QString::from(b.id));
                }
            });
        });
    }

    pub fn installed_on(&self, harness: &QString, scope: &QString) -> bool {
        let scope = scope.to_string();
        let host = scope.strip_prefix("host:").map(str::to_owned).or_else(|| harlo_core::remote::host_of(std::path::Path::new(&scope)));
        match host {
            Some(h) => harlo_core::remote::found_on(&h, &harness.to_string()),
            None => self.installed(harness),
        }
    }
}
