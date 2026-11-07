//! Your own model servers (Settings → Model servers): any OpenAI-compatible
//! server, on this computer or elsewhere (Ollama, LM Studio, llama.cpp,
//! vLLM, SGLang, a company gateway). harlo asks the server what it serves
//! (`/models`) and adds the models you pick to each chosen agent's own
//! config, so they show in harlo's model lists and in the agent's own.
//!
//! | agent | where | entry |
//! |---|---|---|
//! | pi  | `~/.pi/agent/models.json` (`$PI_CODING_AGENT_DIR`) | `providers.<name>`: `baseUrl`, `api`, `apiKey`, `models: [{ id, contextWindow }]` |
//! | omp | `~/.omp/agent/models.yml` | the same, as YAML |
//! | Harlo | `$XDG_CONFIG_HOME/harlo-agent/providers.json` | as pi's |
//!
//! The rest of each file stays as it was: pi's and Harlo's JSON keep their
//! order, omp's YAML its text and comments (the new provider goes in as
//! text). A name already there isn't replaced.

use std::path::{Path, PathBuf};
use std::time::Duration;

use harlo_harness::HarnessKind;
use serde_json::{Map, Value, json};
use yaml_rust::{Yaml, YamlLoader};

/// A model the server serves.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Found {
    pub id: String,
    /// Its context window, when the server says it.
    pub context: Option<u64>,
    /// It thinks before it answers (harlo's test saw it): the agents then
    /// show its thinking and offer thinking levels.
    pub reasoning: bool,
    /// Its server tells low, medium and high apart (Basalt, NInfer's Qwen3.8):
    /// offered, where the others have only on and off. For writing an agent's
    /// file.
    pub levels: bool,
}

/// What a server serves, and the address its models answer at.
#[derive(Debug, Clone, PartialEq)]
pub struct Listing {
    pub base_url: String,
    pub models: Vec<Found>,
}

/// A server already in the agent's file.
#[derive(Debug, Clone, PartialEq)]
pub struct Server {
    pub name: String,
    pub base_url: String,
    pub models: Vec<String>,
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

/// The agent's file its servers are in (pi, omp, Harlo).
pub fn file(kind: HarnessKind) -> Option<PathBuf> {
    match kind {
        HarnessKind::Pi => Some(std::env::var_os("PI_CODING_AGENT_DIR").map(PathBuf::from).unwrap_or_else(|| home().join(".pi/agent")).join("models.json")),
        HarnessKind::Omp => Some(home().join(".omp/agent/models.yml")),
        HarnessKind::Harlo => Some(
            std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(".config")).join("harlo-agent/providers.json"),
        ),
        _ => None,
    }
}

/// The agents on this computer whose model servers harlo sets up.
pub const AGENTS: [HarnessKind; 3] = [HarnessKind::Pi, HarnessKind::Omp, HarnessKind::Harlo];

/// The address typed, as harlo asks it: a scheme, no trailing `/` or
/// `/models`.
fn tidy(url: &str) -> String {
    let u = url.trim().trim_end_matches('/');
    let u = u.strip_suffix("/models").unwrap_or(u).trim_end_matches('/');
    if u.contains("://") { u.to_owned() } else { format!("http://{u}") }
}

/// A key as the agent reads it: `$NAME` / `${NAME}` from the environment.
fn expand(key: &str) -> String {
    let k = key.trim();
    match k.strip_prefix('$') {
        Some(name) => std::env::var(name.trim_start_matches('{').trim_end_matches('}')).unwrap_or_default(),
        None => k.to_owned(),
    }
}

/// The models in a `/models` answer: OpenAI's `data`, with the context
/// window from the fields servers use for it (vLLM, LM Studio, llama.cpp,
/// gateways).
fn models_in(v: &Value) -> Option<Vec<Found>> {
    let list = v["data"].as_array().or(v["models"].as_array())?;
    let mut out: Vec<Found> = list
        .iter()
        .filter_map(|m| {
            let id = m["id"].as_str().or(m["model"].as_str()).or(m["name"].as_str())?.trim();
            let context = ["max_model_len", "context_length", "context_window", "max_context_length", "loaded_context_length"]
                .iter()
                .find_map(|k| m[*k].as_u64())
                .or_else(|| m["meta"]["n_ctx"].as_u64())
                .filter(|n| *n > 0);
            (!id.is_empty()).then(|| Found { id: id.to_owned(), context, ..Default::default() })
        })
        .collect();
    out.dedup_by(|a, b| a.id == b.id);
    Some(out)
}

/// Asks the server what it serves (blocking): `<url>/models`, then
/// `<url>/v1/models` (Ollama and others answer there).
pub fn find(url: &str, key: &str) -> Result<Listing, String> {
    let base = tidy(url);
    if !base.starts_with("http://") && !base.starts_with("https://") {
        return Err("Type the server's address, like http://localhost:11434.".into());
    }
    let http: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).http_status_as_error(false).build().into();
    let key = expand(key);
    let mut tries = vec![base.clone()];
    if !base.ends_with("/v1") {
        tries.push(format!("{base}/v1"));
    }
    let mut why = String::new();
    for b in tries {
        let mut req = http.get(&format!("{b}/models")).header("Accept", "application/json");
        if !key.is_empty() {
            req = req.header("Authorization", &format!("Bearer {key}"));
        }
        match req.call() {
            Err(e) => {
                let said = e.to_string();
                why = if said.contains("refused") {
                    format!("Nothing is answering at {base}. Is the server running?")
                } else {
                    format!("Couldn't reach {base}: {said}")
                };
                break;
            }
            Ok(mut resp) => {
                let status = resp.status().as_u16();
                if status == 401 || status == 403 {
                    return Err(if key.is_empty() { "The server wants a key." } else { "The server didn't accept the key." }.into());
                }
                let body = resp.body_mut().read_to_string().unwrap_or_default();
                if status == 200
                    && let Some(models) = serde_json::from_str::<Value>(&body).ok().as_ref().and_then(models_in)
                {
                    return Ok(Listing { base_url: b, models });
                }
                why = format!("{base} didn't list models (HTTP {status}). Is it an OpenAI-compatible server?");
            }
        }
    }
    Err(why)
}

/// Whether something answers at the server's address now (its model
/// list, within a second or two).
pub fn answers(base_url: &str) -> bool {
    let http: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_millis(1500))).http_status_as_error(false).build().into();
    http.get(&format!("{}/models", base_url.trim_end_matches('/'))).call().is_ok_and(|r| r.status().as_u16() < 500)
}

/// A name for a server at this address: the usual servers by their port,
/// else its host ("gpu-box", "localhost-8000").
pub fn suggest_name(base_url: &str) -> String {
    let rest = base_url.split_once("://").map_or(base_url, |(_, r)| r);
    let authority = rest.split('/').next().unwrap_or("");
    let (host, port) = authority.rsplit_once(':').map_or((authority, ""), |(h, p)| (h, p));
    let name = match port {
        "11434" => "ollama".to_owned(),
        "1234" => "lm-studio".to_owned(),
        _ => {
            let h = host.trim_matches(['[', ']']).split('.').next().unwrap_or("server");
            let h = if h.chars().all(|c| c.is_ascii_digit()) { host.replace('.', "-") } else { h.to_owned() };
            if port.is_empty() { h } else { format!("{h}-{port}") }
        }
    };
    name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect()
}

/// The servers in the agent's config: providers with an address and
/// models of their own.
pub fn servers(kind: HarnessKind) -> Vec<Server> {
    listed(&providers(kind))
}

/// A server at a local address, as far as listing a model goes: the start
/// command harlo has for it ("" for none), whether it answers, and what it
/// serves if it says (one with a key may not).
#[derive(Debug, Clone, Default)]
pub struct LocalServer {
    start: String,
    answers: bool,
    serving: Option<Vec<String>>,
}

/// A model Oh My Pi lists from its own discovery of a local engine
/// ("llama.cpp", "ollama", "lm-studio") that's not there: omp keeps the models
/// it found after their server stopped for good, and takes whatever answers on
/// an engine's usual port for that engine. `seen` caches by address across a
/// list.
pub fn stale_discovered(provider: &str, model: &str, base_url: &str, seen: &mut std::collections::HashMap<String, LocalServer>) -> bool {
    // (This computer only, where omp looks: a refused connection is instant.)
    let host = base_url.split("://").nth(1).unwrap_or_default().split(['/', ':']).next().unwrap_or_default();
    if !["llama.cpp", "ollama", "lm-studio"].contains(&provider) || !["127.0.0.1", "localhost"].contains(&host) {
        return false;
    }
    let url = base_url.trim_end_matches('/').to_owned();
    let url = if url.ends_with("/v1") { url } else { format!("{url}/v1") };
    let server = seen.entry(url.clone()).or_insert_with(|| {
        let answering = answers(&url);
        LocalServer {
            start: crate::db::Db::open(&crate::db::default_path())
                .ok()
                .and_then(|d| d.get(&format!("model_server.{url}")).ok().flatten())
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .and_then(|v| v["start"].as_str().map(str::to_owned))
                .unwrap_or_default(),
            answers: answering,
            serving: answering.then(|| find(&url, "").ok()).flatten().map(|l| l.models.into_iter().map(|m| m.id).collect()),
        }
    });
    discovered_is_stale(provider, model, server)
}

fn discovered_is_stale(provider: &str, model: &str, server: &LocalServer) -> bool {
    let same_engine = match provider {
        "llama.cpp" => server.start.contains("llama-server"),
        "ollama" => server.start.contains("ollama"),
        _ => false,
    };
    // Another engine harlo set up on that port: not what omp calls it (it's
    // in omp's file under its own name, if it's there at all).
    if !server.start.is_empty() && !same_engine {
        return true;
    }
    match &server.serving {
        Some(models) => !models.iter().any(|m| m == model),
        // Not answering: harlo starts its own before a message; any other's
        // gone. (Answering without saying: left as it is.)
        None => !server.answers && server.start.is_empty(),
    }
}

/// The agent's providers, as JSON.
fn providers(kind: HarnessKind) -> Value {
    file(kind).map(|f| providers_in(kind, &f)).unwrap_or_default()
}

#[cfg(test)]
fn servers_in(kind: HarnessKind, file: &Path) -> Vec<Server> {
    listed(&providers_in(kind, file))
}

fn providers_in(kind: HarnessKind, file: &Path) -> Value {
    let Ok(text) = std::fs::read_to_string(file) else { return Value::Null };
    providers_of(kind, &text)
}

/// The providers in an agent file's text (pi, omp, Harlo).
fn providers_of(kind: HarnessKind, text: &str) -> Value {
    let text = text.to_owned();
    match kind {
        HarnessKind::Omp => YamlLoader::load_from_str(&text).ok().and_then(|d| d.into_iter().next()).map_or(Value::Null, |doc| yaml_json(&doc["providers"])),
        _ => serde_json::from_str::<Value>(&text).map(|v| v["providers"].clone()).unwrap_or_default(),
    }
}

/// A server as an agent has it, to add to another (`add`): its address,
/// key (`$NAME` for one from the environment; "" none) and models.
pub fn details(kind: HarnessKind, name: &str) -> Option<(String, String, Vec<Found>)> {
    details_in(&providers(kind), name)
}

fn details_in(all: &Value, name: &str) -> Option<(String, String, Vec<Found>)> {
    let p = all.get(name)?;
    let server = listed(&Value::Object(Map::from_iter([(name.to_owned(), p.clone())]))).into_iter().next()?;
    let key = p["apiKey"].as_str().filter(|k| *k != "none").unwrap_or_default().to_owned();
    let context = |id: &str| -> Option<u64> { p["models"].as_array()?.iter().find(|m| m["id"] == id)?["contextWindow"].as_u64() };
    let reasoning = |id: &str| -> bool { p["models"].as_array().and_then(|a| a.iter().find(|m| m["id"] == id)).and_then(|m| m["reasoning"].as_bool()).unwrap_or(false) };
    let models = server.models.iter().map(|id| Found { id: id.clone(), context: context(id), reasoning: reasoning(id), ..Default::default() }).collect();
    Some((server.base_url, key, models))
}

/// Sets the context of a server's models in the agent's file (the server
/// serves that now): whether it changed anything.
pub fn set_context(kind: HarnessKind, name: &str, context: u64) -> Result<bool, String> {
    let f = file(kind).ok_or("harlo can't change this agent's model servers.")?;
    set_context_in(kind, &f, name, context)
}

fn set_context_in(kind: HarnessKind, file: &Path, name: &str, context: u64) -> Result<bool, String> {
    let all = providers_in(kind, file);
    let Some((url, key, models)) = details_in(&all, name) else { return Ok(false) };
    if models.iter().all(|m| m.context == Some(context)) {
        return Ok(false);
    }
    let models: Vec<Found> = models.into_iter().map(|m| Found { context: Some(context), ..m }).collect();
    let shown = file.display();
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let out = match kind {
        // (Its text: the block again, comments elsewhere as they are.)
        HarnessKind::Omp => omp_with(&omp_without(&text, name)?, name, &url, placeholder(&key), &models).map_err(|e| format!("{e} ({shown})"))?,
        _ => {
            let mut v: Value = serde_json::from_str(&text).map_err(|e| format!("{shown} isn't valid JSON ({e})"))?;
            for m in v["providers"][name]["models"].as_array_mut().into_iter().flatten() {
                m["contextWindow"] = json!(context);
            }
            serde_json::to_string_pretty(&v).unwrap_or_default() + "\n"
        }
    };
    write(file, &out).map_err(|e| format!("Couldn't write {shown}: {e}"))?;
    Ok(true)
}

/// pi's thinking levels for a model that thinks (null: not offered).
fn pi_levels(levels: bool) -> Value {
    // (Their top level: "xhigh", which some of these models' templates alone take.)
    if levels {
        json!({ "off": "none", "minimal": null, "low": "low", "medium": "medium", "high": "xhigh" })
    } else {
        json!({ "off": "none", "minimal": null, "low": null, "medium": null, "high": "high" })
    }
}

/// Offers thinking levels for a server's thinking models in the agent's file,
/// where it was added without them: whether it changed anything.
pub fn offer_levels(kind: HarnessKind, name: &str) -> Result<bool, String> {
    let f = file(kind).ok_or("harlo can't change this agent's model servers.")?;
    offer_levels_in(kind, &f, name)
}

fn offer_levels_in(kind: HarnessKind, file: &Path, name: &str) -> Result<bool, String> {
    let shown = file.display();
    let text = std::fs::read_to_string(file).map_err(|e| format!("Couldn't read {shown}: {e}"))?;
    let mut v: Value = serde_json::from_str(&text).map_err(|e| format!("{shown} isn't valid JSON ({e})"))?;
    let (top, field, wanted) = match kind {
        HarnessKind::Pi | HarnessKind::Harlo => ("providers", "thinkingLevelMap", pi_levels(true)),
        _ => return Ok(false),
    };
    let Some(models) = v.get_mut(top).and_then(|p| p.get_mut(name)).and_then(|p| p.get_mut("models")) else { return Ok(false) };
    let models: Vec<&mut Value> = match models {
        Value::Object(m) => m.values_mut().collect(),
        Value::Array(m) => m.iter_mut().collect(),
        _ => return Ok(false),
    };
    let mut changed = false;
    for m in models.into_iter().filter(|m| m["reasoning"] == true) {
        if m[field] != wanted {
            m[field] = wanted.clone();
            changed = true;
        }
    }
    if changed {
        write(file, &(serde_json::to_string_pretty(&v).unwrap_or_default() + "\n")).map_err(|e| format!("Couldn't write {shown}: {e}"))?;
    }
    Ok(changed)
}

/// Takes a server out of the agent's file (its other settings as they are).
pub fn remove(kind: HarnessKind, name: &str) -> Result<(), String> {
    let f = file(kind).ok_or("harlo can't change this agent's model servers.")?;
    remove_from(kind, &f, name)
}

fn remove_from(kind: HarnessKind, file: &Path, name: &str) -> Result<(), String> {
    let shown = file.display();
    let text = std::fs::read_to_string(file).map_err(|e| format!("Couldn't read {shown}: {e}"))?;
    let Some(out) = without_server(kind, &text, &shown.to_string(), name)? else { return Ok(()) };
    write(file, &out).map_err(|e| format!("Couldn't write {shown}: {e}"))
}

/// pi's, omp's or Harlo's file (`text`) without the provider: None when
/// it isn't there.
fn without_server(kind: HarnessKind, text: &str, shown: &str, name: &str) -> Result<Option<String>, String> {
    if kind == HarnessKind::Omp {
        return omp_without(text, name).map(Some).map_err(|e| format!("{e} ({shown})"));
    }
    let mut v: Value = serde_json::from_str(text).map_err(|e| format!("{shown} isn't valid JSON ({e})"))?;
    if v["providers"].as_object_mut().and_then(|m| m.remove(name)).is_none() {
        return Ok(None);
    }
    Ok(Some(serde_json::to_string_pretty(&v).unwrap_or_default() + "\n"))
}

/// omp's file without the provider, taken out as text (its comments and
/// layout stay).
fn omp_without(text: &str, name: &str) -> Result<String, String> {
    let doc = |t: &str| YamlLoader::load_from_str(t).ok().and_then(|d| d.into_iter().next()).unwrap_or(Yaml::Null);
    let before = doc(text);
    let Some(others) = before["providers"].as_hash().filter(|_| !before["providers"][name].is_badvalue()).map(|h| h.len() - 1) else {
        return Ok(text.to_owned());
    };
    let lines: Vec<&str> = text.lines().collect();
    let indent = |l: &str| l.len() - l.trim_start().len();
    let top = lines.iter().position(|l| l.split(" #").next().unwrap_or(l).trim_end() == "providers:").ok_or("Its providers are written in a way harlo can't change; take the server out by hand")?;
    let is_name = |l: &str| [format!("{name}:"), format!("\"{name}\":"), format!("'{name}':")].iter().any(|k| l.trim_start().starts_with(k.as_str()));
    let start = (top + 1..lines.len()).find(|&i| is_name(lines[i]) && indent(lines[i]) > 0).ok_or("Its providers are written in a way harlo can't change; take the server out by hand")?;
    let ind = indent(lines[start]);
    // Its block: the lines under it (blank ones too, when more follow).
    let mut end = start + 1;
    while end < lines.len() && (lines[end].trim().is_empty() || indent(lines[end]) > ind) {
        end += 1;
    }
    while end > start + 1 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    let mut out: Vec<String> = lines[..start].iter().map(|l| l.to_string()).collect();
    out.extend(lines[end..].iter().map(|l| l.to_string()));
    if others == 0 {
        out[top] = "providers: {}".into();
    }
    let out = out.join("\n") + "\n";
    // What omp will read: the others as they were, this one gone.
    let after = doc(&out);
    let left = after["providers"].as_hash().map_or(0, |h| h.len());
    if !after["providers"][name].is_badvalue() || left != others {
        return Err("Its providers are written in a way harlo can't change; take the server out by hand".into());
    }
    Ok(out)
}

/// The servers among an agent's providers (as JSON).
fn listed(providers: &Value) -> Vec<Server> {
    providers
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(name, p)| {
            let (base_url, models): (&str, Vec<String>) = (p["baseUrl"].as_str()?, p["models"].as_array()?.iter().filter_map(|m| m["id"].as_str().map(str::to_owned)).collect());
            (!models.is_empty()).then(|| Server { name: name.clone(), base_url: base_url.to_owned(), models })
        })
        .collect()
}

/// The parts of YAML a models file uses, as JSON.
fn yaml_json(y: &Yaml) -> Value {
    match y {
        Yaml::Hash(h) => Value::Object(h.iter().filter_map(|(k, v)| Some((k.as_str()?.to_owned(), yaml_json(v)))).collect()),
        Yaml::Array(a) => Value::Array(a.iter().map(yaml_json).collect()),
        Yaml::String(s) => json!(s),
        Yaml::Integer(i) => json!(i),
        Yaml::Real(r) => r.parse::<f64>().map_or(Value::Null, |f| json!(f)),
        Yaml::Boolean(b) => json!(b),
        _ => Value::Null,
    }
}

/// Adds a server and the models picked to the agent's file. `key`: what
/// the agent sends (`$NAME` reads it from the environment); empty, a
/// placeholder (servers that want none still need one for the agent to
/// offer their models). `taken`: provider names the agent has already.
pub fn add(kind: HarnessKind, name: &str, base_url: &str, key: &str, models: &[Found], taken: &[String]) -> Result<(), String> {
    let f = file(kind).ok_or("harlo can't add model servers to this agent.")?;
    add_to(kind, &f, name, base_url, key, models, taken)
}

/// A name the agent takes, and some models.
fn check_new(name: &str, models: &[Found]) -> Result<(), String> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') {
        return Err("Name it with letters, digits, - or _.".into());
    }
    if models.is_empty() {
        return Err("Pick at least one model.".into());
    }
    Ok(())
}

fn add_to(kind: HarnessKind, file: &Path, name: &str, base_url: &str, key: &str, models: &[Found], taken: &[String]) -> Result<(), String> {
    let name = name.trim();
    check_new(name, models)?;
    let shown = file.display().to_string();
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let out = with_server(kind, &text, &shown, name, base_url, key, models, taken)?;
    write(file, &out).map_err(|e| format!("Couldn't write {shown}: {e}"))
}

/// pi's, omp's or Harlo's file (`text`; `shown` in errors) with the
/// server in: a name already there isn't replaced, unless it's the same
/// server again.
#[allow(clippy::too_many_arguments)]
fn with_server(kind: HarnessKind, text: &str, shown: &str, name: &str, base_url: &str, key: &str, models: &[Found], taken: &[String]) -> Result<String, String> {
    let existing = listed(&providers_of(kind, text));
    // The same server again (set up again): its entry replaced.
    let again = existing.iter().any(|s| s.name == name && s.base_url == base_url);
    if !again && (existing.iter().any(|s| s.name == name) || taken.iter().any(|t| t == name)) {
        return Err(format!("There's already a provider called {name}. Pick another name."));
    }
    let text = text.to_owned();
    let text = match kind {
        _ if !again => text,
        HarnessKind::Omp => omp_without(&text, name).map_err(|e| format!("{e} ({shown})"))?,
        HarnessKind::Pi | HarnessKind::Harlo => {
            let mut v: Value = serde_json::from_str(&text).map_err(|e| format!("{shown} isn't valid JSON ({e})"))?;
            v["providers"].as_object_mut().map(|m| m.remove(name));
            serde_json::to_string_pretty(&v).unwrap_or_default()
        }
        _ => text,
    };
    match kind {
        HarnessKind::Omp => omp_with(&text, name, base_url, placeholder(key), models).map_err(|e| format!("{e} ({shown})")),
        _ => pi_with(&text, name, base_url, placeholder(key), models).map_err(|e| format!("{e} ({shown})")),
    }
}

/// pi and omp offer a provider's models only with a key: one that servers
/// wanting none ignore.
fn placeholder(key: &str) -> &str {
    if key.trim().is_empty() { "none" } else { key.trim() }
}

fn pi_with(text: &str, name: &str, base_url: &str, key: &str, models: &[Found]) -> Result<String, String> {
    let mut v: Value = if text.trim().is_empty() { json!({}) } else { serde_json::from_str(text).map_err(|e| format!("The file isn't valid JSON ({e}); fix it first"))? };
    let root = v.as_object_mut().ok_or("The file isn't a JSON object")?;
    let providers = root.entry("providers").or_insert_with(|| json!({}));
    let providers = providers.as_object_mut().ok_or("Its providers aren't a JSON object")?;
    if providers.contains_key(name) {
        return Err(format!("There's already a provider called {name}"));
    }
    let list: Vec<Value> = models
        .iter()
        .map(|m| {
            let mut e = Map::new();
            e.insert("id".into(), json!(m.id));
            if let Some(c) = m.context {
                e.insert("contextWindow".into(), json!(c));
            }
            // A model that thinks: pi's "off" sends "none" (else it sends
            // nothing and the model thinks anyway); its levels in between
            // only where the server tells them apart.
            if m.reasoning {
                e.insert("reasoning".into(), json!(true));
                e.insert("thinkingLevelMap".into(), pi_levels(m.levels));
            }
            Value::Object(e)
        })
        .collect();
    providers.insert(name.to_owned(), json!({ "baseUrl": base_url, "api": "openai-completions", "apiKey": key, "models": list }));
    Ok(serde_json::to_string_pretty(&v).unwrap_or_default() + "\n")
}

/// omp's file with the provider put in as text, after `providers:` (or a
/// new `providers:` at the end), so its comments and layout stay.
fn omp_with(text: &str, name: &str, base_url: &str, key: &str, models: &[Found]) -> Result<String, String> {
    if !text.trim().is_empty() {
        YamlLoader::load_from_str(text).map_err(|e| format!("The file isn't valid YAML ({e}); fix it first"))?;
    }
    let q = |s: &str| serde_json::to_string(s).unwrap_or_default();
    let block = |ind: &str| {
        let mut b = format!("{ind}{}:\n{ind}  baseUrl: {}\n{ind}  api: openai-completions\n{ind}  apiKey: {}\n{ind}  models:\n", q(name), q(base_url), q(key));
        for m in models {
            b.push_str(&format!("{ind}    - id: {}\n", q(&m.id)));
            if let Some(c) = m.context {
                b.push_str(&format!("{ind}      contextWindow: {c}\n"));
            }
            if m.reasoning {
                b.push_str(&format!("{ind}      reasoning: true\n"));
            }
        }
        b
    };
    let lines: Vec<&str> = text.lines().collect();
    let top = lines.iter().position(|l| {
        let l = l.split(" #").next().unwrap_or(l).trim_end();
        l == "providers:" || l == "providers: {}"
    });
    let out = match top {
        Some(i) => {
            // Its providers' indent (two spaces for a first one).
            let ind = lines[i + 1..]
                .iter()
                .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
                .map(|l| l.len() - l.trim_start().len())
                .filter(|n| *n > 0)
                .unwrap_or(2);
            let mut out: Vec<String> = lines[..i].iter().map(|l| l.to_string()).collect();
            out.push("providers:".into());
            out.push(block(&" ".repeat(ind)).trim_end().to_owned());
            out.extend(lines[i + 1..].iter().map(|l| l.to_string()));
            out.join("\n") + "\n"
        }
        None if lines.iter().any(|l| l.starts_with("providers")) => return Err("Its providers are written in a way harlo can't add to; add the server by hand".into()),
        None => {
            let mut out = text.trim_end().to_owned();
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str("providers:\n");
            out.push_str(&block("  "));
            out
        }
    };
    // What omp will read: the server, as written.
    let doc = YamlLoader::load_from_str(&out).ok().and_then(|d| d.into_iter().next()).unwrap_or(Yaml::Null);
    if doc["providers"][name]["baseUrl"].as_str() != Some(base_url) {
        return Err("Its providers are written in a way harlo can't add to; add the server by hand".into());
    }
    Ok(out)
}

// This computer's servers for another machine's agents: added to their
// files there with the same address, `localhost:<port>` forwarded back here
// over the SSH connection (`ssh -O forward -R`), again before each session
// starts there (a connection that ended took its forwards with it).

/// The agents there whose files harlo writes servers to.
pub const ON_HOST: [HarnessKind; 3] = [HarnessKind::Pi, HarnessKind::Omp, HarnessKind::Harlo];

/// An agent's server file on `host`, as an `ssh://` path.
fn file_on(host: &str, kind: HarnessKind) -> Option<PathBuf> {
    let home = crate::mcp::host_home(host);
    match kind {
        HarnessKind::Pi => Some(home.join(".pi/agent/models.json")),
        HarnessKind::Omp => Some(home.join(".omp/agent/models.yml")),
        // (harlo's agent there, as it starts: no XDG_CONFIG_HOME set over SSH.)
        HarnessKind::Harlo => Some(home.join(".config/harlo-agent/providers.json")),
        _ => None,
    }
}

/// A file there: its text, "" when there's none (never when it couldn't be
/// read: that's an error, so nothing is written over it).
fn read_on(file: &Path) -> Result<String, String> {
    let (name, path) = crate::remote::split(file).ok_or("not another machine's file")?;
    let host = crate::remote::host(&name).ok_or_else(|| format!("unknown host {name}"))?;
    let q = crate::remote::quote(&path);
    let out = crate::remote::run(&host, &format!("if [ -f {q} ]; then cat -- {q}; else exit 3; fi"), Duration::from_secs(30)).map_err(|e| format!("{e:#}"))?;
    match out.status.code() {
        Some(0) => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        Some(3) => Ok(String::new()),
        _ => Err(format!("Couldn't read {path} on {name}: {}", String::from_utf8_lossy(&out.stderr).trim())),
    }
}

/// Writes a file there whole (its folder made; a new one only for you).
fn write_on(file: &Path, text: &str) -> Result<(), String> {
    let (name, path) = crate::remote::split(file).ok_or("not another machine's file")?;
    let host = crate::remote::host(&name).ok_or_else(|| format!("unknown host {name}"))?;
    let q = crate::remote::quote;
    let dir = Path::new(&path).parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = format!("{path}.harlo.tmp");
    let script = format!(
        "umask 077; mkdir -p {d} && cat > {t} && {{ [ -f {f} ] && chmod --reference={f} {t} 2>/dev/null; mv -f {t} {f}; }}",
        d = q(&dir),
        t = q(&tmp),
        f = q(&path)
    );
    let out = crate::remote::run_with_input(&host, &script, Some(text.as_bytes()), Duration::from_secs(30)).map_err(|e| format!("{e:#}"))?;
    if !out.status.success() {
        return Err(format!("Couldn't write {path} on {name}: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
}

/// The servers an agent on `host` has.
pub fn servers_on(host: &str, kind: HarnessKind) -> Result<Vec<Server>, String> {
    let f = file_on(host, kind).ok_or("harlo doesn't add model servers to this agent there.")?;
    Ok(listed(&providers_of(kind, &read_on(&f)?)))
}

/// Adds a server to an agent on `host` (see [`add`]). `forward`: it runs on
/// this computer, its port forwarded there (else it's the machine's own, or
/// reached at the same address from there).
pub fn add_on(host: &str, kind: HarnessKind, name: &str, base_url: &str, key: &str, models: &[Found], forward_here: bool) -> Result<(), String> {
    let f = file_on(host, kind).ok_or("harlo doesn't add model servers to this agent there.")?;
    let name = name.trim();
    check_new(name, models)?;
    if let Some(port) = local_port(base_url).filter(|_| forward_here) {
        let mut ports = forwards(host);
        if !ports.contains(&port) {
            ports.push(port);
        }
        forward(host, port)?;
        save_forwards(host, &ports);
    }
    let text = read_on(&f)?;
    let out = with_server(kind, &text, &f.to_string_lossy(), name, base_url, key, models, &[])?;
    write_on(&f, &out)
}

/// Takes a server out of an agent on `host`; its port stops being
/// forwarded there once no agent there has a server on it.
pub fn remove_on(host: &str, kind: HarnessKind, name: &str) -> Result<(), String> {
    let f = file_on(host, kind).ok_or("harlo doesn't change this agent's model servers there.")?;
    let text = read_on(&f)?;
    if let Some(out) = without_server(kind, &text, &f.to_string_lossy(), name)? {
        write_on(&f, &out)?;
    }
    let used: Vec<u16> = ON_HOST.iter().filter_map(|k| servers_on(host, *k).ok()).flatten().filter_map(|s| local_port(&s.base_url)).collect();
    let (kept, dropped): (Vec<u16>, Vec<u16>) = forwards(host).into_iter().partition(|p| used.contains(p));
    for port in dropped {
        let _ = forward_control(host, port, "cancel");
    }
    save_forwards(host, &kept);
    Ok(())
}

/// The port of a server on this computer (`localhost`, `127.0.0.1`…).
pub fn local_port(base_url: &str) -> Option<u16> {
    let authority = base_url.split("://").nth(1)?.split('/').next()?;
    let (host, port) = authority.rsplit_once(':')?;
    let host = host.trim_matches(['[', ']']);
    ["127.0.0.1", "localhost", "::1", "0.0.0.0"].contains(&host).then(|| port.parse().ok()).flatten()
}

/// The ports forwarded to `host` (harlo's settings).
pub fn forwards(host: &str) -> Vec<u16> {
    let Ok(db) = crate::db::Db::open(&crate::db::default_path()) else { return Vec::new() };
    db.get(&format!("host_forwards.{host}")).ok().flatten().and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

fn save_forwards(host: &str, ports: &[u16]) {
    if let Ok(db) = crate::db::Db::open(&crate::db::default_path()) {
        let _ = db.set(&format!("host_forwards.{host}"), &serde_json::to_string(ports).unwrap_or_default());
    }
}

fn forward_control(host: &str, port: u16, op: &str) -> Result<std::process::Output, String> {
    let h = crate::remote::host(host).ok_or_else(|| format!("unknown host {host}"))?;
    let args = crate::remote::ssh_args(&h);
    let (options, destination) = args.split_at(args.len().saturating_sub(1));
    std::process::Command::new("ssh")
        .args(options)
        .args(["-O", op, "-R", &format!("{port}:localhost:{port}")])
        .args(destination)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| e.to_string())
}

/// This computer's `port`, as `localhost:<port>` on `host` (its connection
/// open first).
fn forward(host: &str, port: u16) -> Result<(), String> {
    let h = crate::remote::host(host).ok_or_else(|| format!("unknown host {host}"))?;
    if !crate::remote::connected(&h) {
        crate::remote::run(&h, "true", Duration::from_secs(30)).map_err(|e| format!("{e:#}"))?;
    }
    let out = forward_control(host, port, "forward")?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    // (Asked before on this connection: it's there.)
    if err.contains("already") {
        return Ok(());
    }
    Err(format!("Couldn't forward port {port} to {host} (is it taken there?): {}", err.trim()))
}

/// Before a session starts on `host`: this computer's servers its agents
/// have, forwarded there.
pub fn forward_to(host: &str) {
    for port in forwards(host) {
        if let Err(e) = forward(host, port) {
            tracing::warn!("model server on {host}: {e}");
        }
    }
}

// ---- Another machine's server, used here ----
//
// A server harlo set up on another machine answers on that machine's
// localhost only; this computer's agents reach it through a port forwarded
// over the SSH connection (`-L`), at an address of their own here. harlo
// keeps which (`host_uses`: `{ <address here>: { host, url } }`) and puts the
// forward back before a message on it, the server started there first.

fn load_uses() -> serde_json::Map<String, Value> {
    let Ok(db) = crate::db::Db::open(&crate::db::default_path()) else { return Default::default() };
    db.get("host_uses").ok().flatten().and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

fn save_uses(uses: &serde_json::Map<String, Value>) {
    if let Ok(db) = crate::db::Db::open(&crate::db::default_path()) {
        let _ = db.set("host_uses", &Value::Object(uses.clone()).to_string());
    }
}

/// The machine and its server's address there, for an address here that
/// reaches one.
pub fn through(local_url: &str) -> Option<(String, String)> {
    let uses = load_uses();
    let v = uses.get(local_url.trim_end_matches('/'))?;
    Some((v["host"].as_str()?.to_owned(), v["url"].as_str()?.to_owned()))
}

/// The address here that reaches `url` on `host`, if there's one.
pub fn used_here(host: &str, url: &str) -> Option<String> {
    load_uses().into_iter().find(|(_, v)| v["host"] == host && v["url"] == url).map(|(k, _)| k)
}

/// An address here for `url` on `host` (the one it has, else a free port
/// here: its own number when that's free), its port forwarded now.
pub fn use_from(host: &str, url: &str) -> Result<String, String> {
    if let Some(here) = used_here(host, url) {
        forward_here(&here)?;
        return Ok(here);
    }
    let port = local_port(url).ok_or("Only a server on that machine's own address can be reached through SSH.")?;
    let mut uses = load_uses();
    let taken: Vec<u16> = uses.keys().filter_map(|k| local_port(k)).chain(crate::servers::ports_in_use().into_iter().map(|(p, _)| p)).collect();
    let mut here_port = port;
    loop {
        here_port = crate::engines::free_port(here_port);
        if !taken.contains(&here_port) || here_port >= 65000 {
            break;
        }
        here_port += 1;
    }
    let path = url.split("://").nth(1).and_then(|r| r.find('/').map(|i| r[i..].to_owned())).unwrap_or_default();
    let here = format!("http://127.0.0.1:{here_port}{path}");
    uses.insert(here.clone(), serde_json::json!({ "host": host, "url": url }));
    save_uses(&uses);
    forward_here(&here)?;
    Ok(here)
}

/// Forgets the addresses here no agent here has any more (their forwards
/// cancelled).
pub fn forget_unused_uses() {
    let mut uses = load_uses();
    let used: Vec<String> = AGENTS
        .iter()
        .flat_map(|k| servers(*k))
        .map(|s| s.base_url.trim_end_matches('/').to_owned())
        .collect();
    let before = uses.len();
    uses.retain(|here, v| {
        let keep = used.contains(here);
        if !keep && let (Some(host), Some(p), Some(rp)) = (v["host"].as_str(), local_port(here), v["url"].as_str().and_then(local_port)) {
            let _ = local_forward(host, p, rp, "cancel");
        }
        keep
    });
    if uses.len() != before {
        save_uses(&uses);
    }
}

fn local_forward(host: &str, port: u16, remote_port: u16, op: &str) -> Result<std::process::Output, String> {
    let h = crate::remote::host(host).ok_or_else(|| format!("unknown host {host}"))?;
    let args = crate::remote::ssh_args(&h);
    let (options, destination) = args.split_at(args.len().saturating_sub(1));
    std::process::Command::new("ssh")
        .args(options)
        .args(["-O", op, "-L", &format!("127.0.0.1:{port}:127.0.0.1:{remote_port}")])
        .args(destination)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| e.to_string())
}

/// The forward behind an address here (its machine's connection opened
/// first).
pub fn forward_here(here: &str) -> Result<(), String> {
    let (host, url) = through(here).ok_or("harlo doesn't know this address.")?;
    let (Some(port), Some(remote_port)) = (local_port(here), local_port(&url)) else { return Err("Not an address harlo forwards.".into()) };
    let h = crate::remote::host(&host).ok_or_else(|| format!("unknown host {host}"))?;
    if !crate::remote::connected(&h) {
        crate::remote::run(&h, "true", Duration::from_secs(30)).map_err(|e| format!("{e:#}"))?;
    }
    let out = local_forward(&host, port, remote_port, "forward")?;
    let err = String::from_utf8_lossy(&out.stderr);
    if out.status.success() || err.contains("already") {
        return Ok(());
    }
    // (Asked before on this connection: the port's its own.)
    if answers(here) {
        return Ok(());
    }
    Err(format!("Couldn't reach {host}'s server through port {port} here (is it taken?): {}", err.trim()))
}

/// Before a message on an address here that reaches another machine's
/// server: the forward back, and the server started there if it's stopped
/// (blocking).
pub fn reach(here: &str) -> Result<(), String> {
    if answers(here) {
        return Ok(());
    }
    forward_here(here)?;
    if answers(here) {
        return Ok(());
    }
    let (host, url) = through(here).ok_or("harlo doesn't know this address.")?;
    let v = crate::engine_host::json(&host, &["start", &url], Duration::from_secs(20 * 60)).map_err(|e| format!("{e:#}"))?;
    if let Some(e) = v["error"].as_str() {
        return Err(e.to_owned());
    }
    Ok(())
}

/// At start: the forwards to machines harlo is connected to (the others
/// come back before a message on them).
pub fn forward_uses() {
    for (here, v) in load_uses() {
        let Some(host) = v["host"].as_str() else { continue };
        if crate::remote::is_disconnected(host) || !crate::remote::host(host).is_some_and(|h| crate::remote::connected(&h)) {
            continue;
        }
        if let Err(e) = forward_here(&here) {
            tracing::warn!("model server on {host}: {e}");
        }
    }
}

/// Writes the file whole (a temporary file renamed over it), keeping its
/// permissions; a new one only for you (it can hold a key).
fn write(file: &Path, text: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mode = std::fs::metadata(file).map(|m| m.permissions().mode() & 0o7777).unwrap_or(0o600);
    let tmp = file.with_extension(format!("harlo-{}.tmp", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))?;
    std::fs::rename(&tmp, file).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harlos_servers_on_a_host() {
        assert!(ON_HOST.contains(&HarnessKind::Harlo));
        // (Where harlo-agent reads them there.)
        let f = file_on("not-a-host", HarnessKind::Harlo).unwrap();
        assert_eq!(f, crate::remote::join("not-a-host", "~/.config/harlo-agent/providers.json"));
    }

    #[test]
    fn offers_thinking_levels() {
        let dir = std::env::temp_dir().join(format!("harlo-levels-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let thinking = Found { reasoning: true, ..found("qwen", Some(65536)) };
        let models = [thinking.clone(), found("small", None)];
        for (kind, file) in [(HarnessKind::Pi, "models.json"), (HarnessKind::Harlo, "providers.json")] {
            let f = dir.join(file);
            add_to(kind, &f, "other", "http://10.0.0.2:8000/v1", "", &[thinking.clone()], &[]).unwrap();
            add_to(kind, &f, "basalt", "http://127.0.0.1:8080/v1", "", &models, &[]).unwrap();
            // Added before harlo offered them: put in, once, and only there.
            assert!(offer_levels_in(kind, &f, "basalt").unwrap(), "{kind:?}");
            assert!(!offer_levels_in(kind, &f, "basalt").unwrap(), "{kind:?}");
            assert!(!offer_levels_in(kind, &f, "absent").unwrap(), "{kind:?}");
            let updated = std::fs::read_to_string(&f).unwrap();
            let v: Value = serde_json::from_str(&updated).unwrap();
            assert!(!updated.contains("absent"), "{kind:?}");
            let p = &v["providers"];
            let map = &p["basalt"]["models"][0]["thinkingLevelMap"];
            assert!(map["off"] == "none" && map["low"] == "low" && map["medium"] == "medium" && map["high"] == "xhigh" && map["minimal"].is_null());
            assert!(p["basalt"]["models"][1].get("thinkingLevelMap").is_none());
            assert!(p["other"]["models"][0]["thinkingLevelMap"]["low"].is_null());
            // Added with them: the same file.
            let with = [Found { levels: true, ..thinking.clone() }, found("small", None)];
            add_to(kind, &f, "basalt", "http://127.0.0.1:8080/v1", "", &with, &["basalt".to_owned()]).unwrap();
            assert!(!offer_levels_in(kind, &f, "basalt").unwrap(), "{kind:?}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn found(id: &str, context: Option<u64>) -> Found {
        Found { id: id.into(), context, ..Default::default() }
    }

    #[test]
    fn reads_what_servers_list() {
        let vllm = json!({ "object": "list", "data": [{ "id": "Qwen/Qwen3-32B", "max_model_len": 32768 }, { "id": "Qwen/Qwen3-32B" }] });
        assert_eq!(models_in(&vllm).unwrap(), vec![found("Qwen/Qwen3-32B", Some(32768))]);
        let ollama = json!({ "object": "list", "data": [{ "id": "qwen2.5-coder:7b", "owned_by": "library" }] });
        assert_eq!(models_in(&ollama).unwrap(), vec![found("qwen2.5-coder:7b", None)]);
        let llamacpp = json!({ "models": [{ "model": "gemma.gguf" }], "data": [{ "id": "gemma.gguf", "meta": { "n_ctx": 8192 } }] });
        assert_eq!(models_in(&llamacpp).unwrap(), vec![found("gemma.gguf", Some(8192))]);
        assert!(models_in(&json!({ "error": "no" })).is_none());

        assert_eq!(tidy("localhost:11434/"), "http://localhost:11434");
        assert_eq!(tidy(" https://gw.example.com/v1/models "), "https://gw.example.com/v1");
        assert_eq!(suggest_name("http://localhost:11434/v1"), "ollama");
        assert_eq!(suggest_name("http://127.0.0.1:1234/v1"), "lm-studio");
        assert_eq!(suggest_name("http://192.168.1.20:8000/v1"), "192-168-1-20-8000");
        assert_eq!(suggest_name("https://gpu-box.lan/v1"), "gpu-box");
    }

    /// A server answering `/v1/models` only, and only with the key.
    fn fake_server() -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(4) {
                let Ok(mut s) = stream else { continue };
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_lowercase();
                let (status, body) = if !req.contains("authorization: bearer k") {
                    ("401 Unauthorized", "{}".to_owned())
                } else if req.starts_with("get /v1/models ") {
                    ("200 OK", json!({ "data": [{ "id": "qwen", "max_model_len": 4096 }] }).to_string())
                } else {
                    ("404 Not Found", "nope".to_owned())
                };
                let _ = write!(s, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        format!("{addr}")
    }

    #[test]
    fn asks_a_server_what_it_serves() {
        let addr = fake_server();
        let l = find(&format!("{addr}/"), "k").unwrap();
        assert_eq!(l, Listing { base_url: format!("http://{addr}/v1"), models: vec![found("qwen", Some(4096))] });
        assert_eq!(find(&addr, "").unwrap_err(), "The server wants a key.");
        assert_eq!(find(&addr, "wrong").unwrap_err(), "The server didn't accept the key.");
        assert_eq!(find("127.0.0.1:9", "").unwrap_err(), "Nothing is answering at http://127.0.0.1:9. Is the server running?");
    }

    #[test]
    fn hides_omps_leftovers_from_gone_servers() {
        let mut seen = std::collections::HashMap::new();
        // Nothing answers on port 9: omp's llama.cpp models there are gone.
        assert!(stale_discovered("llama.cpp", "qwen", "http://127.0.0.1:9/v1", &mut seen));
        assert!(stale_discovered("ollama", "qwen", "http://127.0.0.1:9", &mut seen));
        // Not omp's discovery, or not this computer: left as they are.
        assert!(!stale_discovered("qwen3-4b", "qwen", "http://127.0.0.1:9/v1", &mut seen));
        assert!(!stale_discovered("llama.cpp", "qwen", "http://192.168.1.20:9/v1", &mut seen));
        // One that answers but wants a key to list its models: can't tell, stays.
        let addr = fake_server();
        assert!(!stale_discovered("llama.cpp", "qwen", &format!("http://{addr}/v1"), &mut seen));

        let server = |start: &str, serving: Option<&[&str]>| LocalServer { start: start.into(), answers: serving.is_some(), serving: serving.map(|m| m.iter().map(|s| s.to_string()).collect()) };
        // Someone's own server: what it serves stays, what it doesn't goes.
        assert!(!discovered_is_stale("llama.cpp", "qwen", &server("", Some(&["qwen"]))));
        assert!(discovered_is_stale("llama.cpp", "an-older-model", &server("", Some(&["qwen"]))));
        let ninfer = "exec /x/ninfer-serve --config /x/n.toml";
        // Another engine harlo set up on llama.cpp's port, stopped or running:
        // an old model of omp's, and its own under omp's "llama.cpp".
        assert!(discovered_is_stale("llama.cpp", "Qwen3.8-FN-GSQ-RCO-IQ3_S", &server(ninfer, None)));
        assert!(discovered_is_stale("llama.cpp", "Qwen3.8-27B-NVFP4", &server(ninfer, Some(&["Qwen3.8-27B-NVFP4"]))));
        // harlo's own llama.cpp: stopped, kept (it starts before a message);
        // running, what it serves.
        let llama = "exec /x/llama-server --models-preset /x/p.ini";
        assert!(!discovered_is_stale("llama.cpp", "qwen", &server(llama, None)));
        assert!(!discovered_is_stale("llama.cpp", "qwen", &server(llama, Some(&["qwen"]))));
        assert!(discovered_is_stale("llama.cpp", "old", &server(llama, Some(&["qwen"]))));
        assert!(!discovered_is_stale("ollama", "qwen", &server("OLLAMA_HOST=x exec /x/ollama serve", None)));
    }

    #[test]
    fn copies_and_removes_servers() {
        let dir = std::env::temp_dir().join(format!("harlo-model-servers-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let models = [found("qwen", Some(40960)), found("small", None)];
        // Each agent's entry reads back as added: address, key, models.
        for (kind, file, key) in [
            (HarnessKind::Pi, "models.json", ""),
            (HarnessKind::Omp, "models.yml", "$LLAMA_KEY"),
            (HarnessKind::Harlo, "providers.json", "$LLAMA_KEY"),
        ] {
            let f = dir.join(file);
            add_to(kind, &f, "other", "http://10.0.0.2:8000/v1", "k", &[found("x", None)], &[]).unwrap();
            add_to(kind, &f, "llama", "http://127.0.0.1:8080/v1", key, &models, &[]).unwrap();
            let got = details_in(&providers_in(kind, &f), "llama").unwrap();
            assert_eq!(got, ("http://127.0.0.1:8080/v1".to_owned(), key.to_owned(), models.to_vec()), "{kind:?}");
            // Set up again (same name, same address): replaced, not refused;
            // a model that thinks says so.
            let thinking = [Found { reasoning: true, ..found("qwen", Some(40960)) }, found("small", None)];
            add_to(kind, &f, "llama", "http://127.0.0.1:8080/v1", key, &thinking, &["llama".to_owned()]).unwrap();
            assert_eq!(details_in(&providers_in(kind, &f), "llama").unwrap().2, thinking.to_vec(), "{kind:?}");
            assert_eq!(servers_in(kind, &f).len(), 2, "{kind:?}");
            if kind == HarnessKind::Pi {
                let v: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
                let qwen = &v["providers"]["llama"]["models"][0];
                assert_eq!(qwen["thinkingLevelMap"]["off"], "none");
                assert!(qwen["thinkingLevelMap"]["low"].is_null() && qwen["thinkingLevelMap"]["high"] == "high");
                assert!(v["providers"]["llama"]["models"][1].get("thinkingLevelMap").is_none());
            }
            // Another address under that name: still refused.
            assert!(add_to(kind, &f, "llama", "http://127.0.0.1:9999/v1", key, &models, &[]).is_err(), "{kind:?}");
            // The server serves another context now: the file says so.
            assert!(set_context_in(kind, &f, "llama", 16384).unwrap(), "{kind:?}");
            assert!(!set_context_in(kind, &f, "llama", 16384).unwrap(), "{kind:?}");
            let now = details_in(&providers_in(kind, &f), "llama").unwrap().2;
            assert!(now.iter().all(|m| m.context == Some(16384)), "{kind:?} {now:?}");
            assert_eq!(servers_in(kind, &f).len(), 2, "{kind:?}");
            remove_from(kind, &f, "llama").unwrap();
            let left = servers_in(kind, &f);
            assert_eq!(left.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["other"], "{kind:?}");
            // Gone already: nothing to do.
            remove_from(kind, &f, "llama").unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn takes_a_server_out_of_omp_keeping_comments() {
        let text = "# mine\nmodelOverrides:\n  x:\n    contextWindow: 1\nproviders:\n  # the box\n  llama:\n    baseUrl: \"http://127.0.0.1:8080/v1\"\n    models:\n      - id: \"qwen\"\n\n  \"gw\":\n    baseUrl: \"https://gw/v1\"\n    models:\n      - id: a\ntheme: dark # keep\n";
        let out = omp_without(text, "llama").unwrap();
        assert_eq!(out, "# mine\nmodelOverrides:\n  x:\n    contextWindow: 1\nproviders:\n  # the box\n\n  \"gw\":\n    baseUrl: \"https://gw/v1\"\n    models:\n      - id: a\ntheme: dark # keep\n");
        let last = omp_without(&out, "gw").unwrap();
        assert!(last.contains("providers: {}\n") && last.contains("theme: dark # keep"), "{last}");
        assert_eq!(omp_without(&last, "gw").unwrap(), last);
    }

    #[test]
    fn adds_a_server_to_pi_keeping_the_rest() {
        let dir = std::env::temp_dir().join(format!("harlo-model-servers-pi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let f = dir.join("models.json");
        let models = [found("qwen2.5-coder:7b", None), found("big", Some(65536))];
        add_to(HarnessKind::Pi, &f, "ollama", "http://localhost:11434/v1", "", &models, &[]).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        assert_eq!(v["providers"]["ollama"], json!({ "baseUrl": "http://localhost:11434/v1", "api": "openai-completions", "apiKey": "none", "models": [{ "id": "qwen2.5-coder:7b" }, { "id": "big", "contextWindow": 65536 }] }));
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
        }

        // Someone's own file: its providers and their order stay.
        std::fs::write(&f, r#"{"$schema":"x","providers":{"mine":{"baseUrl":"http://a/v1","api":"openai-completions","models":[{"id":"m","samplingParams":{"top_k":20}}]},"anthropic":{"baseUrl":"https://proxy/v1"}}}"#).unwrap();
        add_to(HarnessKind::Pi, &f, "gpu", "http://gpu.lan:8000/v1", "$GPU_KEY", &models[..1], &[]).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        assert_eq!(v.as_object().unwrap().keys().collect::<Vec<_>>(), ["$schema", "providers"]);
        assert_eq!(v["providers"].as_object().unwrap().keys().collect::<Vec<_>>(), ["mine", "anthropic", "gpu"]);
        assert_eq!(v["providers"]["mine"]["models"][0]["samplingParams"]["top_k"], 20);
        assert_eq!(v["providers"]["gpu"]["apiKey"], "$GPU_KEY");
        // Only those with models of their own are servers.
        let names: Vec<String> = servers_in(HarnessKind::Pi, &f).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["mine", "gpu"]);

        assert!(add_to(HarnessKind::Pi, &f, "mine", "http://b/v1", "", &models, &[]).unwrap_err().contains("already"));
        assert!(add_to(HarnessKind::Pi, &f, "openai", "http://b/v1", "", &models, &["openai".into()]).unwrap_err().contains("already"));
        assert!(add_to(HarnessKind::Pi, &f, "bad name", "http://b/v1", "", &models, &[]).is_err());
        assert!(add_to(HarnessKind::Pi, &f, "none", "http://b/v1", "", &[], &[]).is_err());
        std::fs::write(&f, "{ broken").unwrap();
        assert!(add_to(HarnessKind::Pi, &f, "x", "http://b/v1", "", &models, &[]).unwrap_err().contains("valid JSON"));
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "{ broken");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// harlo-agent's file is laid out as pi's (harlo-agent reads it so).
    #[test]
    fn harlo_agents_servers() {
        let dir = std::env::temp_dir().join(format!("harlo-model-servers-harlo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let f = dir.join("providers.json");
        let thinks = Found { id: "qwen3".into(), context: Some(32768), reasoning: true, levels: false };
        add_to(HarnessKind::Harlo, &f, "llamacpp", "http://127.0.0.1:8090/v1", "", &[thinks.clone(), found("plain", None)], &["chatgpt".into()]).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        assert_eq!(
            v["providers"]["llamacpp"],
            json!({ "baseUrl": "http://127.0.0.1:8090/v1", "api": "openai-completions", "apiKey": "none", "models": [
                { "id": "qwen3", "contextWindow": 32768, "reasoning": true, "thinkingLevelMap": pi_levels(false) }, { "id": "plain" }] })
        );
        assert_eq!(servers_in(HarnessKind::Harlo, &f)[0].models, ["qwen3", "plain"]);
        assert!(add_to(HarnessKind::Harlo, &f, "chatgpt", "http://b/v1", "", &[found("m", None)], &["chatgpt".into()]).unwrap_err().contains("already"));
        // Its levels offered later, its context changed, set up again, taken out.
        assert!(offer_levels_in(HarnessKind::Harlo, &f, "llamacpp").unwrap());
        assert!(set_context_in(HarnessKind::Harlo, &f, "llamacpp", 65536).unwrap());
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        assert_eq!((v["providers"]["llamacpp"]["models"][0]["thinkingLevelMap"].clone(), v["providers"]["llamacpp"]["models"][1]["contextWindow"].as_u64()), (pi_levels(true), Some(65536)));
        add_to(HarnessKind::Harlo, &f, "llamacpp", "http://127.0.0.1:8090/v1", "$KEY", &[thinks], &[]).unwrap();
        assert_eq!(details_in(&providers_in(HarnessKind::Harlo, &f), "llamacpp").unwrap().1, "$KEY");
        remove_from(HarnessKind::Harlo, &f, "llamacpp").unwrap();
        assert!(servers_in(HarnessKind::Harlo, &f).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn adds_a_server_to_omp_keeping_its_text() {
        // No file yet.
        let models = [found("qwen2.5-coder:7b", None), found("big", Some(65536))];
        let new = omp_with("", "ollama", "http://localhost:11434/v1", "none", &models).unwrap();
        assert_eq!(new, "providers:\n  \"ollama\":\n    baseUrl: \"http://localhost:11434/v1\"\n    api: openai-completions\n    apiKey: \"none\"\n    models:\n      - id: \"qwen2.5-coder:7b\"\n      - id: \"big\"\n        contextWindow: 65536\n");

        // Its own providers, indented by four, with comments.
        let own = "# my models\nproviders:\n    # the work gateway\n    work:\n        baseUrl: https://gw/v1\n        models:\n            - id: a\nmodelOverrides: {}\n";
        let out = omp_with(own, "gpu", "http://gpu.lan:8000/v1", "$GPU_KEY", &models[..1]).unwrap();
        assert!(out.starts_with("# my models\nproviders:\n    \"gpu\":\n      baseUrl:"));
        assert!(out.contains("    # the work gateway\n    work:\n        baseUrl: https://gw/v1\n"));
        let doc = &YamlLoader::load_from_str(&out).unwrap()[0];
        assert_eq!(doc["providers"]["work"]["models"][0]["id"].as_str(), Some("a"));
        assert_eq!(doc["providers"]["gpu"]["apiKey"].as_str(), Some("$GPU_KEY"));
        assert!(doc["modelOverrides"].as_hash().is_some());

        // Other settings, no providers yet.
        let out = omp_with("modelOverrides:\n  x:\n    contextWindow: 1\n", "ollama", "http://localhost:11434/v1", "none", &models[..1]).unwrap();
        let doc = &YamlLoader::load_from_str(&out).unwrap()[0];
        assert_eq!(doc["providers"]["ollama"]["models"][0]["id"].as_str(), Some("qwen2.5-coder:7b"));
        assert_eq!(doc["modelOverrides"]["x"]["contextWindow"].as_i64(), Some(1));
        // An empty mapping is filled in.
        let out = omp_with("providers: {}\n", "ollama", "http://localhost:11434/v1", "none", &models[..1]).unwrap();
        assert!(YamlLoader::load_from_str(&out).unwrap()[0]["providers"]["ollama"]["baseUrl"].as_str().is_some());
        // Written another way: harlo leaves it alone.
        assert!(omp_with("providers: { work: { baseUrl: x } }\n", "o", "http://l/v1", "none", &models).is_err());
        assert!(omp_with("providers: [\n", "o", "http://l/v1", "none", &models).is_err());

        let dir = std::env::temp_dir().join(format!("harlo-model-servers-omp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("models.yml");
        std::fs::write(&f, own).unwrap();
        add_to(HarnessKind::Omp, &f, "gpu", "http://gpu.lan:8000/v1", "", &models, &[]).unwrap();
        let s = servers_in(HarnessKind::Omp, &f);
        assert_eq!(s.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["gpu", "work"]);
        assert_eq!(s[0].models, ["qwen2.5-coder:7b", "big"]);
        assert!(add_to(HarnessKind::Omp, &f, "work", "http://b/v1", "", &models, &[]).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A port here, reached from another machine (`HARLO_TEST_SSH`, e.g.
    /// "ssh myhost") as its own `localhost:<port>`, then no more.
    #[test]
    #[ignore]
    fn forwards_a_port_there() {
        use std::io::{Read, Write};
        let args = crate::remote::parse_ssh_command(&std::env::var("HARLO_TEST_SSH").expect("HARLO_TEST_SSH")).unwrap();
        let host = crate::remote::Host { name: "t".into(), ssh: args };
        crate::remote::set_hosts(vec![host.clone()]);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut c in listener.incoming().flatten() {
                let mut buf = [0u8; 512];
                let _ = c.read(&mut buf);
                let _ = c.write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 5\r\n\r\nhello");
            }
        });
        forward("t", port).unwrap();
        // (Asked again: still fine.)
        forward("t", port).unwrap();
        let ask = format!("curl -s -m 5 http://localhost:{port}/ || wget -qO- -T 5 http://localhost:{port}/");
        let out = crate::remote::run(&host, &ask, Duration::from_secs(30)).unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "hello");
        assert!(forward_control("t", port, "cancel").unwrap().status.success());
        let out = crate::remote::run(&host, &ask, Duration::from_secs(30)).unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "");
        assert_eq!(local_port("http://127.0.0.1:8080/v1"), Some(8080));
        assert_eq!(local_port("http://[::1]:11434/v1"), Some(11434));
        assert_eq!(local_port("http://192.168.1.5:8080/v1"), None);
    }

    /// A server on another machine's localhost (`HARLO_TEST_SSH`), reached
    /// here at an address of its own, then not once nothing uses it. Its
    /// HOME a scratch folder (harlo's settings there).
    #[test]
    #[ignore]
    fn reaches_a_server_there() {
        let args = crate::remote::parse_ssh_command(&std::env::var("HARLO_TEST_SSH").expect("HARLO_TEST_SSH")).unwrap();
        let host = crate::remote::Host { name: "t".into(), ssh: args };
        crate::remote::set_hosts(vec![host.clone()]);
        // A server there answering /v1/models, in a folder of its own.
        let port = 20000 + (std::process::id() % 20000) as u16;
        let dir = format!("harlo-reach-test-{port}");
        let start = format!(
            "mkdir -p ~/{dir}/v1 && echo '{{\"data\":[]}}' > ~/{dir}/v1/models && cd ~/{dir} && (setsid python3 -m http.server {port} --bind 127.0.0.1 > /dev/null 2>&1 < /dev/null & echo $! > pid) && for i in $(seq 50); do curl -s -m 1 -o /dev/null http://127.0.0.1:{port}/v1/models && break; sleep 0.1; done"
        );
        crate::remote::run(&host, &start, Duration::from_secs(30)).unwrap();
        let url = format!("http://127.0.0.1:{port}/v1");
        let outcome = std::panic::catch_unwind(|| {
            let here = use_from("t", &url).unwrap();
            assert_eq!(through(&here), Some(("t".to_owned(), url.clone())));
            assert_eq!(used_here("t", &url).as_deref(), Some(here.as_str()));
            // (The same again: the same address.)
            assert_eq!(use_from("t", &url).unwrap(), here);
            assert!(answers(&here), "{here}");
            reach(&here).unwrap();
            // No agent here has it: forgotten, its forward gone.
            forget_unused_uses();
            assert_eq!(through(&here), None);
            assert!(!answers(&here));
        });
        let _ = crate::remote::run(&host, &format!("kill $(cat ~/{dir}/pid); rm -rf ~/{dir}"), Duration::from_secs(30));
        outcome.unwrap();
    }
}
