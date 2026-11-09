//! MCP servers harlo installs with a click (Settings → MCP → Ready to
//! install): turned on or off, for Harlo and Claude, and updated from their
//! own releases.
//!
//! Turning one on the first time downloads its newest GitHub release (its
//! published SHA-256 checked) into harlo's data folder, `mcp/<id>/<version>`;
//! the agents' entries (global, this computer) run it through a `current`
//! link, so they keep one path while it's updated. Update takes a newer
//! release and keeps the one before for sessions still running it.
//!
//! Each runs with Node, the newest installed one that's new enough (in
//! PATH or a version manager's folder); its path goes in the entries, since
//! an agent started from the desktop may not have that PATH.

use std::path::{Path, PathBuf};
use std::time::Duration;

use harlo_harness::HarnessKind;
use serde_json::{Value, json};

use crate::mcp::{self, Form, Scope};

/// A server harlo offers to install.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Offered {
    /// Its folder's name, and its name in the agents' entries.
    pub id: &'static str,
    pub name: &'static str,
    /// What it does, in a line.
    pub about: &'static str,
    /// Its GitHub repository (releases) and the release file with its bundle.
    pub repo: &'static str,
    pub asset: &'static str,
    /// The script Node runs, in the bundle, and its arguments.
    pub entry: &'static str,
    pub args: &'static [&'static str],
    /// Tools Harlo doesn't offer the model: the server's own updating and
    /// set-up checks, which change other agents' settings (harlo updates it).
    pub hidden_for_harlo: &'static [&'static str],
    pub license: &'static str,
}

pub const OFFERED: [Offered; 1] = [Offered {
    id: "context-mode",
    name: "context-mode",
    about: "Keeps large output out of the conversation: runs code over logs, files and pages and returns only the answer, and indexes big results to search later.",
    repo: "mksglu/context-mode",
    asset: "context-mode.tar.gz",
    entry: "server.mjs",
    // (It asks which agent runs it, to find the project: Claude's way
    // takes the folder it's started in, as both agents start it.)
    args: &["--agent", "claude-code"],
    hidden_for_harlo: &["ctx_upgrade", "ctx_doctor"],
    license: "Elastic License 2.0",
}];

pub fn get(id: &str) -> Option<Offered> {
    OFFERED.iter().copied().find(|b| b.id == id)
}

/// Where harlo keeps its copies: `<data>/mcp`.
pub fn root() -> PathBuf {
    crate::db::default_path().parent().map(|d| d.join("mcp")).unwrap_or_else(|| PathBuf::from("mcp"))
}

/// A bundle's version (its package.json's).
fn version_in(dir: &Path) -> Option<String> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("package.json")).ok()?).ok()?;
    v["version"].as_str().map(str::to_owned)
}

/// The Node a bundle needs at least (its package.json's `engines.node`:
/// `>=24.15.0`, `^24.15`, `24`…).
fn needs_node(dir: &Path) -> Option<[u64; 3]> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("package.json")).ok()?).ok()?;
    parse_version(v["engines"]["node"].as_str()?)
}

/// The first version in `s` ("v26.13.0", ">=24.15.0": its three numbers).
fn parse_version(s: &str) -> Option<[u64; 3]> {
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let mut parts = s[start..].split(|c: char| !c.is_ascii_digit()).take_while(|p| !p.is_empty()).map(|p| p.parse::<u64>().ok());
    let major = parts.next()??;
    Some([major, parts.next().flatten().unwrap_or(0), parts.next().flatten().unwrap_or(0)])
}

fn show_version(v: [u64; 3]) -> String {
    if v[2] == 0 { format!("{}.{}", v[0], v[1]) } else { format!("{}.{}.{}", v[0], v[1], v[2]) }
}

/// The copy in use: `<root>/<id>/current`.
fn current(root: &Path, b: &Offered) -> PathBuf {
    root.join(b.id).join("current")
}

/// The version in use (None: none copied yet).
fn installed_in(root: &Path, b: &Offered) -> Option<String> {
    let cur = current(root, b);
    cur.join(b.entry).is_file().then(|| version_in(&cur)).flatten()
}

pub fn installed(b: &Offered) -> Option<String> {
    installed_in(&root(), b)
}

fn newer(a: &str, b: &str) -> bool {
    crate::engines::version_key(a) > crate::engines::version_key(b)
}

/// Makes `<root>/<id>/<version>` (a complete copy in `staged`) the one in
/// use, and removes the versions older than the one it replaces (that one
/// stays for the sessions still running it).
fn switch_to(root: &Path, b: &Offered, staged: &Path, version: &str) -> Result<(), String> {
    let dir = root.join(b.id);
    let target = dir.join(version);
    if target.exists() {
        std::fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
    }
    std::fs::rename(staged, &target).map_err(|e| e.to_string())?;
    let before = installed_in(root, b);
    // (A new link, then renamed over the old one: never missing.)
    let link = dir.join(".current-new");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(version, &link).map_err(|e| e.to_string())?;
    std::fs::rename(&link, current(root, b)).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name == version || name == "current" || Some(&name) == before.as_ref() {
            continue;
        }
        let _ = if e.file_type().is_ok_and(|t| t.is_dir()) { std::fs::remove_dir_all(e.path()) } else { std::fs::remove_file(e.path()) };
    }
    Ok(())
}

// ---- Node ----

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub path: PathBuf,
    pub version: [u64; 3],
}

/// The `node` programs installed: in PATH, then version managers' folders
/// and the usual places.
fn node_candidates(home: &Path, path: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = path.map(|p| std::env::split_paths(p).map(|d| d.join("node")).collect()).unwrap_or_default();
    let versions = |dir: PathBuf, inside: &str| -> Vec<PathBuf> { std::fs::read_dir(dir).map(|r| r.flatten().map(|e| e.path().join(inside)).collect()).unwrap_or_default() };
    out.extend(versions(home.join(".nvm/versions/node"), "bin/node"));
    out.extend(versions(home.join(".local/share/fnm/node-versions"), "installation/bin/node"));
    out.extend(versions(home.join(".fnm/node-versions"), "installation/bin/node"));
    out.extend(versions(home.join(".local/share/mise/installs/node"), "bin/node"));
    out.extend(versions(home.join(".asdf/installs/nodejs"), "bin/node"));
    out.extend(versions(PathBuf::from("/usr/local/n/versions/node"), "bin/node"));
    out.extend([home.join(".volta/bin/node"), home.join(".local/bin/node"), "/usr/local/bin/node".into(), "/usr/bin/node".into()]);
    let mut seen = std::collections::HashSet::new();
    out.into_iter().filter(|p| p.is_file()).filter(|p| seen.insert(p.canonicalize().unwrap_or_else(|_| p.clone()))).collect()
}

fn node_version(path: &Path) -> Option<[u64; 3]> {
    let mut child = std::process::Command::new(path).arg("--version").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).spawn().ok()?;
    let end = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait().ok()? {
            Some(_) => break,
            None if std::time::Instant::now() > end => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let mut out = String::new();
    std::io::Read::read_to_string(child.stdout.as_mut()?, &mut out).ok()?;
    parse_version(out.trim())
}

/// The Node to run with: the first in PATH when it's new enough, else the
/// newest that is. Err: what to install (the newest found, if any).
pub fn find_node(at_least: Option<[u64; 3]>) -> Result<Node, String> {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let found: Vec<Node> = node_candidates(&home, std::env::var_os("PATH").as_deref()).into_iter().filter_map(|p| Some(Node { version: node_version(&p)?, path: p })).collect();
    pick_node(found, at_least)
}

fn pick_node(found: Vec<Node>, at_least: Option<[u64; 3]>) -> Result<Node, String> {
    let ok = |n: &Node| at_least.is_none_or(|min| n.version >= min);
    if let Some(first) = found.first().filter(|n| ok(n)) {
        return Ok(first.clone());
    }
    if let Some(best) = found.iter().filter(|n| ok(n)).max_by_key(|n| n.version) {
        return Ok(best.clone());
    }
    let want = at_least.map(|v| format!("Node {} or newer", show_version(v))).unwrap_or_else(|| "Node".into());
    Err(match found.iter().max_by_key(|n| n.version) {
        Some(n) => format!("It needs {want}; the newest installed is {}. Install a newer Node (nodejs.org, or your package manager), then turn it on again.", show_version(n.version)),
        None => format!("It needs {want}, which isn't installed. Install it (nodejs.org, or your package manager), then turn it on again."),
    })
}

// ---- the agents' entries ----

/// Whether an entry (its command words) runs one of harlo's copies of `b`.
fn runs(b: &Offered, root: &Path, command: &[String]) -> bool {
    let dir = root.join(b.id);
    command.iter().any(|w| Path::new(w).starts_with(&dir))
}

/// Which offered server a configured one is, if it's one of harlo's.
pub fn which(form: &Form) -> Option<Offered> {
    let root = root();
    OFFERED.iter().copied().find(|b| form.name == b.id && runs(b, &root, &form.command))
}

/// The agents it's on for (global, this computer), and whether a server of
/// its name that isn't harlo's is in the way.
fn agents_on(b: &Offered) -> (Vec<HarnessKind>, bool) {
    let root = root();
    let mut on = Vec::new();
    let mut taken = false;
    for kind in mcp::HARNESSES {
        if let Ok(Some(entry)) = mcp::entry(kind, &Scope::Global, b.id) {
            let form = Form::from_entry(kind, b.id, &entry);
            if !runs(b, &root, &form.command) {
                taken = true;
            } else if form.enabled {
                on.push(kind);
            }
        }
    }
    (on, taken)
}

fn agents_key(b: &Offered) -> String {
    format!("settings.mcp_catalog.{}.agents", b.id)
}

fn db() -> Option<crate::db::Db> {
    crate::db::Db::open(&crate::db::default_path()).ok()
}

/// The agents chosen for it last (kept while it's off; none yet: empty).
fn chosen(b: &Offered) -> Vec<HarnessKind> {
    let saved = db().and_then(|d| d.get(&agents_key(b)).ok().flatten()).unwrap_or_default();
    mcp::HARNESSES.into_iter().filter(|k| saved.split(',').any(|s| s == k.as_str())).collect()
}

/// Keeps the agents chosen (for when it's turned on again).
pub fn choose(b: &Offered, kinds: &[HarnessKind]) {
    if let Some(d) = db() {
        let _ = d.set(&agents_key(b), &kinds.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(","));
    }
}

/// Its entries for `kinds` (global; removed from the others' files).
fn write_entries(b: &Offered, node: &Path, copy: &Path, kinds: &[HarnessKind]) -> Result<(), String> {
    let mut command = vec![node.to_string_lossy().into_owned(), copy.join(b.entry).to_string_lossy().into_owned()];
    command.extend(b.args.iter().map(|a| a.to_string()));
    let form = Form { name: b.id.into(), enabled: true, transport: "stdio".into(), command, ..Form::default() };
    mcp::save(&Scope::Global, b.id, &form, kinds).map_err(|e| format!("{e:#}"))?;
    if kinds.contains(&HarnessKind::Harlo) {
        mcp::set_field(HarnessKind::Harlo, &Scope::Global, b.id, "hiddenTools", Some(json!(b.hidden_for_harlo))).map_err(|e| format!("{e:#}"))?;
    }
    Ok(())
}

/// Turns it on for `kinds` (empty: the ones chosen before), downloading it
/// first if it isn't yet (blocking; `progress`: bytes so far, of all).
pub fn enable(b: &Offered, kinds: &[HarnessKind], progress: &mut dyn FnMut(u64, u64)) -> Result<(), String> {
    let kinds = if kinds.is_empty() { chosen(b) } else { kinds.to_vec() };
    let (_, taken) = agents_on(b);
    if taken {
        return Err(format!("You already have a server called {} in your global servers: remove it first, so harlo's doesn't replace it.", b.id));
    }
    if kinds.is_empty() {
        return Err("Tick at least one agent.".into());
    }
    let root = root();
    if installed_in(&root, b).is_none() {
        install_latest(b, false, progress)?;
    }
    let node = find_node(needs_node(&current(&root, b)))?;
    write_entries(b, &node.path, &current(&root, b), &kinds)?;
    choose(b, &kinds);
    Ok(())
}

/// Turns it off: its entries go (its copy stays), and `kinds` are the
/// agents chosen for turning it on again.
pub fn disable(b: &Offered, kinds: &[HarnessKind]) -> Result<(), String> {
    choose(b, kinds);
    let (on, taken) = agents_on(b);
    if on.is_empty() && taken {
        return Err(format!("The server called {} isn't harlo's: change it in your servers below.", b.id));
    }
    for kind in on {
        mcp::put_entry(kind, &Scope::Global, None, b.id).map_err(|e| format!("{e:#}"))?;
    }
    Ok(())
}

/// On harlo's start: the entries of one that's on get a Node again if
/// theirs is gone or now too old.
pub fn on_start() {
    for b in OFFERED {
        let root = root();
        let (on, _) = agents_on(&b);
        let Some(kind) = on.first() else { continue };
        let Ok(Some(entry)) = mcp::entry(*kind, &Scope::Global, b.id) else { continue };
        let node = Form::from_entry(*kind, b.id, &entry).command.first().map(PathBuf::from).unwrap_or_default();
        let need = needs_node(&current(&root, &b));
        if node_version(&node).is_some_and(|v| need.is_none_or(|n| v >= n)) {
            continue;
        }
        match find_node(need) {
            Ok(n) => {
                if let Err(e) = write_entries(&b, &n.path, &current(&root, &b), &on) {
                    tracing::warn!("{}: {e}", b.name);
                }
            }
            Err(e) => tracing::warn!("{}: {e}", b.name),
        }
    }
}

// ---- updating ----

/// `https://api.github.com` (`HARLO_MCP_CATALOG_API` for tests).
fn api() -> String {
    std::env::var("HARLO_MCP_CATALOG_API").unwrap_or_else(|_| "https://api.github.com".into())
}

/// Its newest release: its version and the file with its bundle.
pub fn latest(b: &Offered) -> Result<(String, crate::engines::Asset), String> {
    let release = crate::engines::get_json(&format!("{}/repos/{}/releases/latest", api(), b.repo))?;
    let tag = release["tag_name"].as_str().ok_or("Its latest release has no version.")?.trim_start_matches('v').to_owned();
    let asset = crate::engines::asset_named(&release, b.asset, None).ok_or_else(|| format!("Its latest release ({tag}) has no {} with a checksum.", b.asset))?;
    Ok((tag, asset))
}

/// How many stars its repository has on GitHub.
pub fn stars(b: &Offered) -> Result<u64, String> {
    let repo = crate::engines::get_json(&format!("{}/repos/{}", api(), b.repo))?;
    repo["stargazers_count"].as_u64().ok_or_else(|| "GitHub didn't say how many stars it has.".into())
}

/// Downloads its newest release when it's newer than the one in use (or
/// there's none), checked against its published SHA-256, and makes it the
/// one in use. With `node`, a release whose Node isn't installed is refused.
/// (The version before, the version now.)
fn install_latest(b: &Offered, node: bool, progress: &mut dyn FnMut(u64, u64)) -> Result<(Option<String>, String), String> {
    install_latest_in(&root(), b, node, progress)
}

fn install_latest_in(root: &Path, b: &Offered, node: bool, progress: &mut dyn FnMut(u64, u64)) -> Result<(Option<String>, String), String> {
    let before = installed_in(root, b);
    let (tag, asset) = latest(b)?;
    if let Some(now) = before.clone().filter(|now| !newer(&tag, now)) {
        return Ok((before, now));
    }
    let dir = root.join(b.id);
    let file = dir.join(format!(".download-{tag}.tar.gz"));
    let staged = dir.join(format!(".new-{tag}"));
    let _ = std::fs::remove_dir_all(&staged);
    let got = (|| {
        crate::engines::download(&asset.url, &file, Some(&asset.sha256), &mut |_, d, t| progress(d, t), &|| false)?;
        crate::engines::unpack(&file, &staged)?;
        if !staged.join(b.entry).is_file() {
            return Err(format!("{} {tag}'s release has no {}.", b.name, b.entry));
        }
        let version = version_in(&staged).unwrap_or(tag.clone());
        if node
            && let Some(need) = needs_node(&staged)
            && let Err(e) = find_node(Some(need))
        {
            return Err(format!("{} {version} {}", b.name, e.trim_start_matches("It ").replace(", then turn it on again", " and update again")));
        }
        Ok(version)
    })();
    let _ = std::fs::remove_file(&file);
    let version = match got {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staged);
            return Err(e);
        }
    };
    switch_to(root, b, &staged, &version)?;
    Ok((before, version))
}

/// Updates it to its newest release (blocking), used when its Node is
/// there. New sessions use it; running ones keep theirs.
pub fn update(b: &Offered, progress: &mut dyn FnMut(u64, u64)) -> Result<String, String> {
    let (before, version) = install_latest(b, true, progress)?;
    if before.as_deref() == Some(version.as_str()) {
        return Ok(format!("{} {version} is the newest.", b.name));
    }
    // (Its entries name the Node it needs now.)
    let root = root();
    let (on, _) = agents_on(b);
    if !on.is_empty()
        && let Ok(node) = find_node(needs_node(&current(&root, b)))
    {
        write_entries(b, &node.path, &current(&root, b), &on)?;
    }
    Ok(match before {
        Some(old) => format!("{} is updated: {old} → {version}. New sessions use it.", b.name),
        None => format!("{} {version} is downloaded.", b.name),
    })
}

/// What Settings → MCP shows for it: `{ id, name, about, license, repo,
/// version ("": not installed), agents, chosen, taken, needs, node,
/// nodeError }`.
pub fn status(b: &Offered) -> Value {
    let root = root();
    let version = installed_in(&root, b);
    let (on, taken) = agents_on(b);
    // (Not installed yet: any Node will do until its release says.)
    let need = version.as_ref().and_then(|_| needs_node(&current(&root, b)));
    let node = find_node(need);
    json!({
        "id": b.id, "name": b.name, "about": b.about, "license": b.license, "repo": b.repo,
        "version": version.unwrap_or_default(),
        "agents": on.iter().map(|k| k.as_str()).collect::<Vec<_>>(),
        "chosen": chosen(b).iter().map(|k| k.as_str()).collect::<Vec<_>>(),
        "taken": taken,
        "needs": need.map(show_version).unwrap_or_default(),
        "node": node.as_ref().map(|n| show_version(n.version)).unwrap_or_default(),
        "nodeError": node.err().unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Offered = OFFERED[0];

    fn bundle(dir: &Path, version: &str, node: &str) {
        std::fs::create_dir_all(dir.join("hooks")).unwrap();
        std::fs::write(dir.join("package.json"), json!({ "name": "context-mode", "version": version, "engines": { "node": node } }).to_string()).unwrap();
        std::fs::write(dir.join("server.mjs"), format!("// {version}\n")).unwrap();
        std::fs::write(dir.join("hooks/a.json"), "{}").unwrap();
    }

    /// Its real latest release from GitHub, installed in a scratch folder
    /// (online: `--include-ignored`).
    #[test]
    #[ignore]
    fn installs_the_latest_release() {
        let (version, asset) = latest(&B).unwrap();
        assert!(newer(&version, "2.1.6"), "{version}");
        assert_eq!(asset.sha256.len(), 64);
        assert!(asset.url.starts_with("https://github.com/mksglu/context-mode/releases/download/"), "{}", asset.url);
        assert!(stars(&B).unwrap() > 1000);
        let root = std::env::temp_dir().join(format!("harlo-catalog-live-{}", std::process::id()));
        let mut got = 0;
        let (before, now) = install_latest_in(&root, &B, true, &mut |d, _| got = d).unwrap();
        assert_eq!((before, now.as_str()), (None, version.as_str()));
        assert!(got > 100_000 && current(&root, &B).join("server.mjs").is_file() && needs_node(&current(&root, &B)).is_some());
        // Again: nothing newer.
        assert_eq!(install_latest_in(&root, &B, true, &mut |_, _| ()).unwrap(), (Some(version.clone()), version));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn reads_versions() {
        assert_eq!(parse_version("v26.13.0"), Some([26, 13, 0]));
        assert_eq!(parse_version(">=24.15.0"), Some([24, 15, 0]));
        assert_eq!(parse_version("^22"), Some([22, 0, 0]));
        assert_eq!(parse_version("latest"), None);
        assert_eq!(show_version([24, 15, 0]), "24.15");
        assert!(newer("2.1.10", "2.1.9") && !newer("2.1.7", "2.1.7"));
    }

    #[test]
    fn picks_a_node() {
        let n = |p: &str, v: [u64; 3]| Node { path: p.into(), version: v };
        // The one in PATH when it's enough, else the newest that is.
        assert_eq!(pick_node(vec![n("/a", [24, 16, 0]), n("/b", [26, 0, 0])], Some([24, 15, 0])).unwrap().path, PathBuf::from("/a"));
        assert_eq!(pick_node(vec![n("/a", [22, 0, 0]), n("/b", [25, 1, 0]), n("/c", [26, 0, 0])], Some([24, 15, 0])).unwrap().path, PathBuf::from("/c"));
        assert_eq!(pick_node(vec![n("/a", [22, 3, 0])], Some([24, 15, 0])).unwrap_err(), "It needs Node 24.15 or newer; the newest installed is 22.3. Install a newer Node (nodejs.org, or your package manager), then turn it on again.");
        assert!(pick_node(vec![], Some([24, 15, 0])).unwrap_err().contains("which isn't installed"));
    }

    #[test]
    fn finds_nodes_in_version_managers() {
        let home = std::env::temp_dir().join(format!("harlo-catalog-node-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        for p in [".nvm/versions/node/v24.15.0/bin/node", ".local/share/fnm/node-versions/v26.1.0/installation/bin/node", "bin/node"] {
            std::fs::create_dir_all(home.join(p).parent().unwrap()).unwrap();
            std::fs::write(home.join(p), "").unwrap();
        }
        let found = node_candidates(&home, Some(home.join("bin").as_os_str()));
        assert_eq!(found[..3], [home.join("bin/node"), home.join(".nvm/versions/node/v24.15.0/bin/node"), home.join(".local/share/fnm/node-versions/v26.1.0/installation/bin/node")]);
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn switches_versions() {
        let base = std::env::temp_dir().join(format!("harlo-catalog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("data/mcp");
        assert_eq!(installed_in(&root, &B), None);
        let staged = root.join("context-mode/.new-2.1.7");
        bundle(&staged, "2.1.7", ">=24.15.0");
        switch_to(&root, &B, &staged, "2.1.7").unwrap();
        assert_eq!(installed_in(&root, &B).as_deref(), Some("2.1.7"));
        assert!(current(&root, &B).join("hooks/a.json").is_file());
        assert_eq!(needs_node(&current(&root, &B)), Some([24, 15, 0]));
        // A newer one (an update): the one before stays, older ones go.
        for v in ["2.1.8", "2.1.9"] {
            let staged = root.join("context-mode").join(format!(".new-{v}"));
            bundle(&staged, v, ">=24.15.0");
            switch_to(&root, &B, &staged, v).unwrap();
        }
        assert_eq!(installed_in(&root, &B).as_deref(), Some("2.1.9"));
        let mut left: Vec<String> = std::fs::read_dir(root.join("context-mode")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, ["2.1.8", "2.1.9", "current"]);
        assert_eq!(std::fs::read_link(current(&root, &B)).unwrap(), PathBuf::from("2.1.9"));
        // An entry runs it when its script is in harlo's folder.
        assert!(runs(&B, &root, &["/usr/bin/node".into(), current(&root, &B).join("server.mjs").to_string_lossy().into_owned()]));
        assert!(!runs(&B, &root, &["npx".into(), "-y".into(), "context-mode".into()]));
        std::fs::remove_dir_all(&base).unwrap();
    }
}
