//! Linked wikis: the named workspaces OpenWiki's `openwiki link` keeps in
//! `~/.openwiki/wiki-workspaces.json` (or `$OPENWIKI_CONFIG_DIR`), read the
//! way OpenWiki reads them.

use crate::collate;
use crate::store::{Error, Result};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

const FILE: &str = "wiki-workspaces.json";
const MAX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_WIKIS: usize = 10_000;
const MAX_WORKSPACES: usize = 1_000;

#[derive(Clone, Debug)]
pub struct Wiki {
    pub id: String,
    pub name: String,
    pub root: PathBuf,
}

impl Wiki {
    pub fn identity(&self) -> Value {
        json!({"id": self.id, "name": self.name})
    }
}

#[derive(Clone, Debug)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub wikis: Vec<String>,
}

impl Workspace {
    pub fn summary(&self) -> Value {
        json!({"id": self.id, "name": self.name, "wikiCount": self.wikis.len()})
    }
}

#[derive(Default)]
pub struct Registry {
    pub wikis: Vec<Wiki>,
    pub workspaces: Vec<Workspace>,
    /// (wiki, workspace)
    pub active: Vec<(String, String)>,
}

fn invalid() -> Error {
    Error::Workspace(format!("The {FILE} file is invalid. Fix or remove it, then rerun openwiki link."))
}

fn stale() -> Error {
    Error::Workspace("A selected workspace contains an unavailable repository wiki. Repair it with openwiki link.".into())
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// OpenWiki's home (`$OPENWIKI_CONFIG_DIR`, else `~/.openwiki`).
pub fn openwiki_home() -> PathBuf {
    let home = home_dir();
    match std::env::var("OPENWIKI_CONFIG_DIR").ok().map(|s| crate::jsre::trim(&s).to_owned()).filter(|s| !s.is_empty()) {
        None => home.join(".openwiki"),
        Some(d) if d == "~" => home,
        Some(d) if d.starts_with("~/") || d.starts_with("~\\") => home.join(&d[2..]),
        Some(d) => {
            let p = PathBuf::from(&d);
            if p.is_absolute() { p } else { std::env::current_dir().unwrap_or_default().join(p) }
        }
    }
}

fn terminal_safe(s: &str) -> bool {
    s.chars().all(|c| c >= ' ' && c != '\u{7f}')
}

fn display_name(s: &str) -> bool {
    let n = crate::jsre::len16(s);
    n > 0 && n <= 80 && crate::jsre::trim(s) == s && terminal_safe(s)
}

fn canonical_absolute(s: &str) -> bool {
    let n = crate::jsre::len16(s);
    n > 1 && n <= 2000 && s.starts_with('/') && crate::store::posix_normalize(s) == s && terminal_safe(s)
}

fn identifier(s: &str) -> bool {
    static ID: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$").unwrap());
    ID.is_match(s)
}

fn only_keys(o: &Map<String, Value>, keys: &[&str]) -> bool {
    o.len() == keys.len() && o.keys().all(|k| keys.contains(&k.as_str()))
}

/// The registry (empty when there's none).
pub fn read_registry() -> Result<Registry> {
    let path = openwiki_home().join(FILE);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Registry::default()),
        Err(_) => return Err(invalid()),
    };
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return Err(invalid());
    }
    let text = std::fs::read(&path).map_err(|_| invalid())?;
    let text = String::from_utf8_lossy(&text);
    let v: Value = serde_json::from_str(&text).map_err(|_| invalid())?;
    parse_registry(&v).ok_or_else(invalid)
}

fn parse_registry(v: &Value) -> Option<Registry> {
    let o = v.as_object()?;
    if !only_keys(o, &["version", "wikis", "workspaces", "active"]) || o["version"].as_f64() != Some(1.0) {
        return None;
    }
    let (wikis, workspaces, active) = (o["wikis"].as_array()?, o["workspaces"].as_array()?, o["active"].as_array()?);
    if wikis.len() > MAX_WIKIS || workspaces.len() > MAX_WORKSPACES || active.len() > MAX_WIKIS {
        return None;
    }
    let mut reg = Registry::default();
    for w in wikis {
        let w = w.as_object()?;
        if !only_keys(w, &["id", "name", "root"]) {
            return None;
        }
        let (id, name, root) = (w["id"].as_str()?, w["name"].as_str()?, w["root"].as_str()?);
        if !identifier(id) || reg.wikis.iter().any(|x| x.id == id) || !display_name(name) || !canonical_absolute(root) {
            return None;
        }
        if reg.wikis.iter().any(|x| x.root.as_os_str() == root) {
            return None;
        }
        reg.wikis.push(Wiki { id: id.into(), name: name.into(), root: root.into() });
    }
    for w in workspaces {
        let w = w.as_object()?;
        if !only_keys(w, &["id", "name", "wikis"]) {
            return None;
        }
        let (id, name, members) = (w["id"].as_str()?, w["name"].as_str()?, w["wikis"].as_array()?);
        if !identifier(id)
            || reg.workspaces.iter().any(|x| x.id == id)
            || !display_name(name)
            || reg.workspaces.iter().any(|x| x.name.to_lowercase() == name.to_lowercase())
            || members.len() < 2
            || members.len() > MAX_WIKIS
        {
            return None;
        }
        let mut ms: Vec<String> = Vec::new();
        for m in members {
            let m = m.as_str()?;
            if !reg.wikis.iter().any(|x| x.id == m) || ms.iter().any(|x| x == m) {
                return None;
            }
            ms.push(m.into());
        }
        reg.workspaces.push(Workspace { id: id.into(), name: name.into(), wikis: ms });
    }
    for a in active {
        let a = a.as_object()?;
        if !only_keys(a, &["wiki", "workspace"]) {
            return None;
        }
        let (wiki, ws) = (a["wiki"].as_str()?, a["workspace"].as_str()?);
        if reg.active.iter().any(|(w, _)| w == wiki) || !reg.workspaces.iter().any(|x| x.id == ws && x.wikis.iter().any(|m| m == wiki)) {
            return None;
        }
        reg.active.push((wiki.into(), ws.into()));
    }
    Some(reg)
}

/// A local-only identity for an unregistered repository.
pub fn local_wiki(root: &Path) -> Wiki {
    let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Wiki { id: slug_identifier(&name, "wiki"), name, root: root.to_path_buf() }
}

pub fn slug_identifier(value: &str, fallback: &str) -> String {
    let lower = value.to_lowercase();
    let mut slug = String::new();
    let mut dash = false;
    for c in lower.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if dash && !slug.is_empty() {
                slug.push('-');
            }
            dash = false;
            slug.push(c);
        } else {
            dash = true;
        }
    }
    // (A leading run of others was dropped; a trailing one never pushed.)
    let cut: String = slug.chars().take(56).collect();
    let cut = cut.trim_end_matches('-').to_owned();
    if cut.is_empty() { fallback.to_owned() } else { cut }
}

pub struct Context {
    pub registry: Registry,
    pub current: Wiki,
    pub registered: bool,
}

pub fn context(repository_root: &Path) -> Result<Context> {
    let root = std::fs::canonicalize(repository_root)
        .ok()
        .filter(|r| std::fs::symlink_metadata(r).is_ok_and(|m| m.is_dir()))
        .ok_or_else(|| Error::Workspace("The wiki location does not exist.".into()))?;
    let registry = read_registry()?;
    let found = registry.wikis.iter().find(|w| w.root == root).cloned();
    let registered = found.is_some();
    let current = found.unwrap_or_else(|| local_wiki(&root));
    Ok(Context { registry, current, registered })
}

fn require_registered(c: &Context) -> Result<()> {
    if c.registered {
        return Ok(());
    }
    Err(Error::Workspace("The current repository is not registered in a wiki workspace. Link it before accessing or changing workspace wikis.".into()))
}

fn find_reachable(c: &Context, wiki_id: &str) -> Result<Wiki> {
    require_registered(c)?;
    if wiki_id == c.current.id {
        return Ok(c.current.clone());
    }
    let Some(target) = c.registry.wikis.iter().find(|w| w.id == wiki_id) else {
        return Err(Error::Workspace("Unknown wiki ID. Use an ID returned by OpenWiki search or listing.".into()));
    };
    let shared = c.registry.workspaces.iter().any(|ws| ws.wikis.contains(&c.current.id) && ws.wikis.contains(&target.id));
    if !shared {
        return Err(Error::Workspace("The requested wiki does not share a workspace with the current repository.".into()));
    }
    Ok(target.clone())
}

fn containing_workspace(c: &Context, reference: &str) -> Result<Workspace> {
    require_registered(c)?;
    let normalized = crate::jsre::trim(reference).to_lowercase();
    let ws = c
        .registry
        .workspaces
        .iter()
        .find(|w| w.id == normalized)
        .or_else(|| c.registry.workspaces.iter().find(|w| w.name.to_lowercase() == normalized));
    match ws {
        Some(w) if w.wikis.contains(&c.current.id) => Ok(w.clone()),
        _ => Err(Error::Workspace("Unknown workspace for this repository. Use openwiki_list_workspaces to choose one.".into())),
    }
}

fn containing(reg: &Registry, wiki_id: &str) -> Vec<Workspace> {
    let mut v: Vec<Workspace> = reg.workspaces.iter().filter(|w| w.wikis.iter().any(|m| m == wiki_id)).cloned().collect();
    v.sort_by(|a, b| collate::compare(&a.name, &b.name).then_with(|| collate::compare(&a.id, &b.id)));
    v
}

fn materialize(w: &Wiki) -> Result<Wiki> {
    let canonical = std::fs::canonicalize(&w.root).map_err(|_| stale())?;
    let git = std::fs::symlink_metadata(canonical.join(".git")).is_ok_and(|m| m.is_dir() || m.is_file());
    let marker = std::fs::symlink_metadata(canonical.join("openwiki/.last-update.json")).is_ok_and(|m| m.is_file());
    if canonical != w.root || !git || !marker {
        return Err(stale());
    }
    Ok(Wiki { root: canonical, ..w.clone() })
}

fn require_wiki(reg: &Registry, id: &str) -> Result<Wiki> {
    reg.wikis.iter().find(|w| w.id == id).cloned().ok_or_else(invalid)
}

/// `openwiki_list_workspaces`.
pub fn list_workspaces(root: &Path, wiki_id: Option<&str>) -> Result<Value> {
    let c = context(root)?;
    let wiki = match wiki_id.filter(|w| !w.is_empty()) {
        Some(id) => find_reachable(&c, id)?,
        None => c.current.clone(),
    };
    let workspaces = if c.registered { containing(&c.registry, &wiki.id) } else { Vec::new() };
    let active = if c.registered { c.registry.active.iter().find(|(w, _)| *w == wiki.id).map(|(_, ws)| ws.clone()) } else { None };
    let mut out = Map::new();
    out.insert("wiki".into(), wiki.identity());
    if let Some(a) = active {
        out.insert("activeWorkspace".into(), a.into());
    }
    out.insert("workspaces".into(), workspaces.iter().map(Workspace::summary).collect());
    Ok(Value::Object(out))
}

/// `openwiki_list_wikis`.
pub fn list_wikis(root: &Path, reference: &str) -> Result<Value> {
    let c = context(root)?;
    let ws = containing_workspace(&c, reference)?;
    let wikis = ws.wikis.iter().map(|id| require_wiki(&c.registry, id).map(|w| w.identity())).collect::<Result<Vec<_>>>()?;
    Ok(json!({"workspace": ws.summary(), "wikis": wikis}))
}

pub enum Scope {
    Ready { workspace: Option<Workspace>, wikis: Vec<Wiki> },
    Required(Value),
}

/// Which wikis a search covers.
pub fn search_scope(root: &Path, requested: Option<&str>) -> Result<Scope> {
    let c = context(root)?;
    let local = || Scope::Ready { workspace: None, wikis: vec![c.current.clone()] };
    if !c.registered {
        if requested.is_some_and(|r| !r.is_empty()) {
            require_registered(&c)?;
        }
        return Ok(local());
    }
    let materialize_ws = |ws: Workspace| -> Result<Scope> {
        let wikis = ws.wikis.iter().map(|id| require_wiki(&c.registry, id).and_then(|w| materialize(&w))).collect::<Result<Vec<_>>>()?;
        Ok(Scope::Ready { workspace: Some(ws), wikis })
    };
    let all = containing(&c.registry, &c.current.id);
    if let Some(r) = requested.filter(|r| !r.is_empty()) {
        return materialize_ws(containing_workspace(&c, r)?);
    }
    match all.len() {
        0 => Ok(local()),
        1 => materialize_ws(all[0].clone()),
        _ => {
            let active = c.registry.active.iter().find(|(w, _)| *w == c.current.id).map(|(_, ws)| ws.clone());
            if let Some(ws) = active.and_then(|id| all.iter().find(|w| w.id == id).cloned()) {
                return materialize_ws(ws);
            }
            Ok(Scope::Required(json!({
                "status": "workspace_required",
                "wiki": c.current.identity(),
                "workspaces": all.iter().map(Workspace::summary).collect::<Vec<_>>(),
            })))
        }
    }
}

/// The wiki a read reads.
pub fn readable_wiki(root: &Path, wiki_id: Option<&str>) -> Result<Wiki> {
    let c = context(root)?;
    let id = wiki_id.filter(|w| !w.is_empty());
    if !c.registered {
        if id.is_some() {
            require_registered(&c)?;
        }
        return Ok(c.current.clone());
    }
    match id {
        None => Ok(c.current.clone()),
        Some(id) if id == c.current.id => Ok(c.current.clone()),
        Some(id) => materialize(&find_reachable(&c, id)?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug_identifier("My Repo!", "wiki"), "my-repo");
        assert_eq!(slug_identifier("--__", "wiki"), "wiki");
        assert_eq!(slug_identifier("Ünïcode name", "wiki"), "n-code-name");
    }
}
