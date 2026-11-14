//! Writing a wiki: OpenWiki's resumable page-job lifecycle (begin, plan,
//! page jobs, finish) with its files kept the way OpenWiki keeps them:
//! `openwiki/.run.json` for the run, `openwiki/.page-manifest.json` for
//! which pages cover which source, `.last-update.json` for the last run,
//! and a managed block in AGENTS.md and CLAUDE.md.

use crate::claims::{self, Op};
use crate::finish::{self, Snapshot};
use crate::fsb::{Backend, ReadError};
use crate::ignore::Ignore;
use crate::lang;
use crate::repo::{self, LastUpdate, Noop, Source};
use crate::store::{self, Error, Store};
use crate::util::{self, cmp16};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, Error>;

const RUN_STATE: &str = "openwiki/.run.json";
const QUICKSTART: &str = "/openwiki/quickstart.md";

fn fail(code: &'static str, m: impl Into<String>) -> Error {
    Error::Run(code, m.into())
}

fn other(m: impl Into<String>) -> Error {
    Error::Other(m.into())
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_owned)
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_owned)).collect()
}

fn unique_sorted(mut v: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    v.retain(|x| seen.insert(x.clone()));
    v.sort_by(|a, b| cmp16(a, b));
    v
}

// ---- Schema checks and their key order ----

fn uuid(v: &Value) -> bool {
    static U: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$").unwrap()
    });
    v.as_str().is_some_and(|s| U.is_match(s))
}

fn sha(v: &Value) -> bool {
    v.as_str().is_some_and(|s| s.len() == 71 && s.starts_with("sha256:") && s[7..].bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}

fn min1(v: &Value) -> bool {
    v.as_str().is_some_and(|s| !s.is_empty())
}

fn trim_min1(v: &Value) -> bool {
    v.as_str().is_some_and(|s| !crate::jsre::trim(s).is_empty())
}

fn string_list(v: &Value, each: fn(&Value) -> bool) -> bool {
    v.as_array().is_some_and(|a| a.iter().all(each))
}

fn any_string(v: &Value) -> bool {
    v.is_string()
}

/// A strict object: only `keys`, each passing its check (optional ones may
/// be missing); the result in the schema's key order.
fn object(v: &Value, keys: &[(&str, bool, &dyn Fn(&Value) -> Option<Value>)]) -> Option<Value> {
    let o = v.as_object()?;
    if o.keys().any(|k| !keys.iter().any(|(n, _, _)| n == k)) {
        return None;
    }
    let mut out = Map::new();
    for (k, optional, check) in keys {
        match o.get(*k) {
            Some(x) => {
                out.insert((*k).to_owned(), check(x)?);
            }
            None if *optional => {}
            None => return None,
        }
    }
    Some(Value::Object(out))
}

fn when(f: fn(&Value) -> bool) -> impl Fn(&Value) -> Option<Value> {
    move |v| f(v).then(|| v.clone())
}

fn last_update_schema(v: &Value) -> Option<Value> {
    if v.is_null() {
        return Some(Value::Null);
    }
    object(v, &[
        ("updatedAt", false, &when(any_string)),
        ("command", false, &when(|v| matches!(v.as_str(), Some("init" | "update")))),
        ("gitHead", true, &when(any_string)),
        ("model", false, &when(any_string)),
        ("status", false, &when(|v| matches!(v.as_str(), Some("complete" | "interrupted")))),
        ("language", true, &when(any_string)),
    ])
}

fn prepared_schema(v: &Value) -> Option<Value> {
    let generated = |v: &Value| object(v, &[("by", false, &when(min1)), ("at", true, &when(min1))]);
    let entry = |v: &Value| object(v, &[("page", false, &when(min1)), ("bodyHash", false, &when(min1)), ("generated", true, &generated)]);
    let list = |v: &Value| v.as_array().and_then(|a| a.iter().map(entry).collect::<Option<Vec<_>>>()).map(Value::Array);
    object(v, &[("generatedProvenance", false, &list)])
}

fn job_schema(v: &Value) -> Option<Value> {
    object(v, &[
        ("id", false, &when(uuid)),
        ("path", false, &when(min1)),
        ("title", false, &when(trim_min1)),
        ("purpose", false, &when(trim_min1)),
        ("seedPaths", false, &when(|v| string_list(v, any_string))),
        ("relatedPages", false, &when(|v| string_list(v, any_string))),
        ("instructions", false, &when(|v| string_list(v, trim_min1))),
        ("status", false, &when(|v| matches!(v.as_str(), Some("pending" | "skipped" | "complete")))),
        ("completedBy", true, &when(trim_min1)),
    ])
}

fn plan_schema(v: &Value) -> Option<Value> {
    let pages = |v: &Value| v.as_array().and_then(|a| a.iter().map(job_schema).collect::<Option<Vec<_>>>()).map(Value::Array);
    object(v, &[
        ("pages", false, &pages),
        ("deletePages", false, &when(|v| string_list(v, any_string))),
        ("deferredClaimPages", true, &when(|v| string_list(v, any_string))),
    ])
}

/// `RepositoryRunStateSchema.parse`: the state in its schema's key order,
/// None when it isn't one.
fn state_schema(v: &Value) -> Option<Value> {
    let actor = |v: &Value| object(v, &[("producerActor", false, &when(trim_min1)), ("metadataModel", false, &when(trim_min1))]);
    object(v, &[
        ("schemaVersion", false, &when(|v| v.as_f64() == Some(1.0))),
        ("runId", false, &when(uuid)),
        ("mode", false, &when(|v| matches!(v.as_str(), Some("init" | "update")))),
        ("phase", false, &when(|v| matches!(v.as_str(), Some("planning" | "generating")))),
        ("startedAt", false, &when(min1)),
        ("language", false, &when(min1)),
        ("languageChanged", false, &when(Value::is_boolean)),
        ("requiredRewritePages", false, &when(|v| string_list(v, min1))),
        ("initialPages", false, &when(|v| string_list(v, min1))),
        ("sourceFingerprint", false, &when(sha)),
        ("targetGitHead", true, &when(min1)),
        ("planningContext", true, &when(min1)),
        ("actor", false, &actor),
        ("previousLastUpdate", false, &last_update_schema),
        ("baseGitHead", true, &when(min1)),
        ("wikiGoal", true, &when(any_string)),
        ("beforeContentSnapshot", false, &when(any_string)),
        ("preparedWiki", false, &prepared_schema),
        ("plan", true, &plan_schema),
    ])
}

fn entry_schema(v: &Value) -> Option<Value> {
    object(v, &[
        ("gitHead", true, &when(min1)),
        ("sourceFingerprint", true, &when(sha)),
        ("pageVersion", false, &when(sha)),
        ("completedBy", true, &when(trim_min1)),
        ("completedRunId", true, &when(uuid)),
    ])
}

// ---- Run state ----

fn state_path(root: &Path) -> PathBuf {
    root.join(RUN_STATE)
}

fn read_state(root: &Path) -> Result<Option<Value>> {
    let file = state_path(root);
    let raw = match std::fs::read(&file) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(other(e.to_string())),
    };
    let malformed = || fail("invalid_state", format!("OpenWiki run state is malformed at {}; refusing to discard resumable work.", file.display()));
    let text = String::from_utf8_lossy(&raw);
    let v: Value = serde_json::from_str(&text).map_err(|_| malformed())?;
    state_schema(&v).map(Some).ok_or_else(malformed)
}

fn write_state(root: &Path, state: &Value) -> Result<()> {
    if state_schema(state).is_none() {
        return Err(other("run state failed its schema"));
    }
    util::write_new_then_rename(&state_path(root), &format!("{}\n", util::json_pretty(state)), 0o666).map_err(|e| other(e.to_string()))
}

fn remove_state(root: &Path) {
    let _ = std::fs::remove_file(state_path(root));
}

// ---- Page manifest ----

type Manifest = Map<String, Value>;

fn manifest_path(root: &Path) -> PathBuf {
    root.join(repo::PAGE_MANIFEST_PATH)
}

fn canonical_pages(pages: &Manifest) -> Result<()> {
    for page in pages.keys() {
        match store::normalize_page(page) {
            Err(e) => return Err(fail("invalid_state", format!("OpenWiki page manifest contains an invalid page path {page}: {}", e.message()))),
            Ok(c) if &c != page => return Err(fail("invalid_state", format!("OpenWiki page manifest contains a non-canonical page path: {page}"))),
            Ok(_) => {}
        }
    }
    Ok(())
}

fn read_manifest(root: &Path) -> Result<Manifest> {
    let file = manifest_path(root);
    let raw = match std::fs::read(&file) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Manifest::new()),
        Err(e) => return Err(other(e.to_string())),
    };
    let malformed = || fail("invalid_state", format!("OpenWiki page manifest is malformed at {}; refusing to discard committed page coverage.", file.display()));
    let v: Value = serde_json::from_str(&String::from_utf8_lossy(&raw)).map_err(|_| malformed())?;
    let pages = |v: &Value| {
        let o = v.as_object()?;
        let mut out = Map::new();
        for (k, e) in o {
            if k.is_empty() {
                return None;
            }
            out.insert(k.clone(), entry_schema(e)?);
        }
        Some(Value::Object(out))
    };
    let parsed = object(&v, &[("schemaVersion", false, &when(|v| v.as_f64() == Some(1.0))), ("pages", false, &pages)]).ok_or_else(malformed)?;
    let m = parsed["pages"].as_object().cloned().unwrap_or_default();
    canonical_pages(&m)?;
    Ok(m)
}

fn write_manifest(root: &Path, pages: &Manifest) -> Result<()> {
    let mut sorted: Vec<(&String, &Value)> = pages.iter().collect();
    sorted.sort_by(|a, b| cmp16(a.0, b.0));
    let ordered: Manifest = sorted.into_iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    canonical_pages(&ordered)?;
    let v = json!({"schemaVersion": 1, "pages": Value::Object(ordered)});
    util::write_new_then_rename(&manifest_path(root), &format!("{}\n", util::json_pretty(&v)), 0o666).map_err(|e| other(e.to_string()))
}

/// A run's source checkpoint (fingerprint and, when known, its commit).
#[derive(Clone, Default)]
struct Checkpoint {
    fingerprint: Option<String>,
    git_head: Option<String>,
}

impl Checkpoint {
    fn of(s: &Source) -> Checkpoint {
        Checkpoint { fingerprint: Some(s.fingerprint.clone()), git_head: s.git_head.clone() }
    }
}

fn coverage_error(page: &str) -> Error {
    fail("invalid_state", format!("Cannot advance page coverage for {page}; Markdown and verified Claims are not durable."))
}

/// `buildManifestEntry`.
fn build_entry(root: &Path, page: &str, source: &Checkpoint, completed_by: Option<&str>, run_id: Option<&str>) -> Result<Value> {
    let page = store::normalize_page(page)?;
    let st = Store::new(root);
    let (persisted, version) = match (claims::load(&st, &page), st.hash_page(&page)) {
        (Ok(p), Ok(v)) => (p, v),
        _ => return Err(coverage_error(&page)),
    };
    match &persisted {
        Some(p) if p.verification.is_some() && p.page_version == version => {}
        _ => return Err(coverage_error(&page)),
    }
    let mut e = Map::new();
    e.insert("pageVersion".into(), version.into());
    if let Some(b) = completed_by.filter(|b| !b.is_empty()) {
        e.insert("completedBy".into(), b.into());
    }
    if let Some(r) = run_id.filter(|r| !r.is_empty()) {
        e.insert("completedRunId".into(), r.into());
    }
    if let Some(h) = source.git_head.as_ref().filter(|h| !h.is_empty()) {
        e.insert("gitHead".into(), h.clone().into());
    }
    if let Some(f) = source.fingerprint.as_ref().filter(|f| !f.is_empty()) {
        e.insert("sourceFingerprint".into(), f.clone().into());
    }
    Ok(Value::Object(e))
}

fn record_completion(root: &Path, page: &str, source: &Checkpoint, by: Option<&str>, run_id: Option<&str>) -> Result<Value> {
    let page = store::normalize_page(page)?;
    let entry = build_entry(root, &page, source, by, run_id)?;
    let mut m = read_manifest(root)?;
    m.insert(page, entry.clone());
    write_manifest(root, &m)?;
    Ok(entry)
}

fn seed_manifest(root: &Path, pages: &[String], git_head: &str) -> Result<()> {
    let mut m = read_manifest(root)?;
    let mut changed = false;
    for page in pages {
        let page = store::normalize_page(page)?;
        if m.contains_key(&page) {
            continue;
        }
        match build_entry(root, &page, &Checkpoint { fingerprint: None, git_head: Some(git_head.to_owned()) }, None, None) {
            Ok(e) => {
                m.insert(page, e);
                changed = true;
            }
            Err(Error::Run(..)) => {}
            Err(e) => return Err(e),
        }
    }
    if changed {
        write_manifest(root, &m)?;
    }
    Ok(())
}

fn replace_manifest(root: &Path, pages: &[String], source: &Checkpoint, preserve: &HashSet<String>, preserve_source: &HashSet<String>) -> Result<()> {
    let previous = read_manifest(root)?;
    let mut next = Manifest::new();
    for page in pages {
        let page = store::normalize_page(page)?;
        let prev = previous.get(&page);
        if preserve.contains(&page) {
            if let Some(p) = prev {
                next.insert(page, p.clone());
            }
            continue;
        }
        if preserve_source.contains(&page) {
            if let Some(p) = prev {
                let cp = Checkpoint { fingerprint: s(p, "sourceFingerprint"), git_head: s(p, "gitHead") };
                let refreshed = build_entry(root, &page, &cp, s(p, "completedBy").as_deref(), s(p, "completedRunId").as_deref())?;
                next.insert(page, if refreshed["pageVersion"] == p["pageVersion"] { p.clone() } else { refreshed });
                continue;
            }
            match build_entry(root, &page, source, None, None) {
                Ok(e) => {
                    next.insert(page, e);
                }
                Err(Error::Run(..)) => {}
                Err(e) => return Err(e),
            }
            continue;
        }
        let e = build_entry(root, &page, source, prev.and_then(|p| s(p, "completedBy")).as_deref(), prev.and_then(|p| s(p, "completedRunId")).as_deref())?;
        next.insert(page, e);
    }
    write_manifest(root, &next)
}

/// `getCurrentRepositoryPageCompletion`.
fn current_completion(root: &Path, page: &str, source: &Checkpoint) -> Result<Option<Value>> {
    let Some(fp) = &source.fingerprint else { return Ok(None) };
    let page = store::normalize_page(page)?;
    let m = read_manifest(root)?;
    let Some(entry) = m.get(&page) else { return Ok(None) };
    if s(entry, "sourceFingerprint").as_ref() != Some(fp) {
        return Ok(None);
    }
    if source.git_head.is_some() && s(entry, "gitHead") != source.git_head {
        return Ok(None);
    }
    let st = Store::new(root);
    let current = (|| -> Result<bool> {
        let sidecar = claims::load(&st, &page)?;
        let version = s(entry, "pageVersion");
        Ok(sidecar.is_some_and(|p| p.verification.is_some() && Some(&p.page_version) == version.as_ref()) && Some(st.hash_page(&page)?) == version)
    })()
    .unwrap_or(false);
    Ok(current.then(|| entry.clone()))
}

// ---- Code-mode setup: AGENTS.md / CLAUDE.md blocks and the workflow ----

const START: &str = "<!-- OPENWIKI:START -->";
const END: &str = "<!-- OPENWIKI:END -->";
const LEGACY_HEADING: &str = "## OpenWiki";
const LEGACY_SENTENCE: &str = "This repository has documentation located in the /openwiki directory.";
const LEGACY_LINES: [&str; 4] = [
    "Start here:",
    "- [OpenWiki quickstart](openwiki/quickstart.md)",
    "OpenWiki includes repository overview, architecture notes, workflows, domain concepts, operations, integrations, testing guidance, and source maps.",
    "When working in this repository, read the OpenWiki quickstart first, then follow its links to the relevant architecture, workflow, domain, operation, and testing notes.",
];

fn agents_snippet() -> String {
    format!("{START}\n\n## OpenWiki\n\nThis repository has a generated `openwiki/` evidence index. It is optional just-in-time context, not required startup reading.\n\n- Do not enumerate, preload, or search wikis at task start. Use retrieval when the user asks for it, when unfamiliar architecture or dependency behavior materially affects the task, or when source inspection leaves an important uncertainty. Stop once the question is grounded.\n- When those conditions apply and OpenWiki retrieval tools are available, use `openwiki_search` for just-in-time context and `openwiki_read` for the relevant complete sections. If search returns `workspace_required`, ask which listed workspace to use and retry with its ID.\n- Use `openwiki_list_workspaces` or `openwiki_list_wikis` when workspace membership itself needs to be discovered.\n- If the retrieval tools are unavailable, read `openwiki/quickstart.md` and follow its links to the relevant pages.\n- Treat source code and tests as authoritative. A brief's unknowns and review items are verification gaps, not automatic requirements.\n- Prefer the narrowest quiet validation that proves the changed behavior. Preserve complete failure output.\n\nUse the repository's documented maintenance process to refresh generated pages after source changes. Check its configuration and completed runs before relying on automatic updates. Do not hand-edit generated OpenWiki pages unless explicitly asked; prefer updating source code/docs and letting OpenWiki regenerate.\n\n{END}")
}

fn claude_snippet() -> String {
    format!("{START}\n\n## OpenWiki\n\n@AGENTS.md\n\n{END}")
}

fn fence_marker(body: &str) -> Option<(char, usize, String)> {
    static F: std::sync::LazyLock<crate::jsre::Re> = std::sync::LazyLock::new(|| crate::jsre::Re::new(r"^\s{0,3}(`{3,}|~{3,})(.*)$", "u"));
    let m = F.exec(body)?;
    let mk = m.str(1);
    Some((mk.chars().next().unwrap(), mk.len(), m.str(2).to_owned()))
}

fn legacy_sections(content: &str) -> Vec<(usize, usize)> {
    let mut lines: Vec<&str> = Vec::new();
    let mut rest = content;
    while !rest.is_empty() {
        let n = rest.find('\n').map_or(rest.len(), |i| i + 1);
        lines.push(&rest[..n]);
        rest = &rest[n..];
    }
    let mut offsets = Vec::new();
    let mut running = 0;
    for l in &lines {
        offsets.push(running);
        running += l.len();
    }
    let body = |l: &str| -> String { l.strip_suffix('\n').map(|x| x.strip_suffix('\r').unwrap_or(x)).unwrap_or(l).to_owned() };
    let mut out = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut i = 0;
    while i < lines.len() {
        let b = body(lines[i]);
        let marker = fence_marker(&b);
        if let Some((ch, len)) = fence {
            if let Some((c, l, info)) = &marker {
                if *c == ch && *l >= len && crate::jsre::trim(info).is_empty() {
                    fence = None;
                }
            }
            i += 1;
            continue;
        }
        if let Some((c, l, _)) = marker {
            fence = Some((c, l));
            i += 1;
            continue;
        }
        if b != LEGACY_HEADING {
            i += 1;
            continue;
        }
        let mut next = i + 1;
        while next < lines.len() && body(lines[next]).is_empty() {
            next += 1;
        }
        if next >= lines.len() || body(lines[next]) != LEGACY_SENTENCE {
            i += 1;
            continue;
        }
        let mut end = i + 1;
        while end < lines.len() {
            let b = body(lines[end]);
            if !(b.is_empty() || b == LEGACY_SENTENCE || LEGACY_LINES.contains(&b.as_str())) {
                break;
            }
            end += 1;
        }
        out.push((offsets[i], if end < lines.len() { offsets[end] } else { content.len() }));
        i = end;
    }
    out
}

fn next_agents_content(path: &Path, snippet: &str, claude: bool) -> Result<Option<String>> {
    let current = match std::fs::read(path) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if claude {
                return Ok(None);
            }
            String::new()
        }
        Err(e) => return Err(other(e.to_string())),
    };
    if claude && crate::jsre::trim(&current) == "@AGENTS.md" {
        return Ok(None);
    }
    let sections = legacy_sections(&current);
    let mut without = current.clone();
    let mut sorted = sections.clone();
    sorted.sort_by(|a, b| b.0.cmp(&a.0));
    for (a, b) in sorted {
        without = format!("{}{}", &without[..a], &without[b..]);
    }
    let (start, end) = (without.find(START), without.find(END));
    if start.is_none() && end.is_none() {
        if claude && crate::jsre::trim(&without) == "@AGENTS.md" {
            return Ok(Some("@AGENTS.md\n".into()));
        }
        if sections.is_empty() {
            let sep = if crate::jsre::trim(&current).is_empty() { "" } else { "\n\n" };
            return Ok(Some(format!("{}{sep}{snippet}\n", crate::jsre::trim_end(&current))));
        }
        let anchor = sections[0].0;
        let before = crate::jsre::trim_end(&without[..anchor]);
        let after = without[anchor..].trim_start_matches(['\r', '\n']);
        let body: Vec<&str> = [before, snippet, after].into_iter().filter(|p| !p.is_empty()).collect();
        return Ok(Some(format!("{}\n", crate::jsre::trim_end(&body.join("\n\n")))));
    }
    let ok = matches!((start, end), (Some(s), Some(e)) if e > s && Some(s) == without.rfind(START) && Some(e) == without.rfind(END));
    if !ok {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        return Err(other(format!("Cannot update {name} because its OpenWiki managed markers are malformed or duplicated. Expected either no markers or exactly one {START} marker followed by one {END} marker. Repair or remove the markers and retry; the file was left unchanged.")));
    }
    let (s, e) = (start.unwrap(), end.unwrap());
    Ok(Some(format!("{}{snippet}{}", &without[..s], &without[e + END.len()..])))
}

fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.ino() != 0 && x.ino() == y.ino() && x.dev() == y.dev(),
        _ => false,
    }
}

const WORKFLOW: &str = include_str!("openwiki-update.yml");

/// `ensureCodeModeRepoSetup`: the workflow on init (when there's none), and
/// the managed block in AGENTS.md and CLAUDE.md.
fn code_mode_setup(root: &Path, workflow: bool) -> Result<()> {
    if workflow {
        let p = root.join(".github/workflows/openwiki-update.yml");
        match std::fs::read(&p) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| other(e.to_string()))?;
                std::fs::write(&p, WORKFLOW).map_err(|e| other(e.to_string()))?;
            }
            Err(e) => return Err(other(e.to_string())),
        }
    }
    let agents = agents_snippet();
    let (a, c) = (root.join("AGENTS.md"), root.join("CLAUDE.md"));
    let claude = if same_file(&a, &c) { agents.clone() } else { claude_snippet() };
    let updates = [(a.clone(), next_agents_content(&a, &agents, false)?), (c.clone(), next_agents_content(&c, &claude, true)?)];
    for (p, next) in updates {
        if let Some(n) = next {
            std::fs::write(&p, n).map_err(|e| other(e.to_string()))?;
        }
    }
    Ok(())
}

// ---- Replacing the wiki on init ----

struct Replacement {
    wiki: PathBuf,
    backup_parent: Option<PathBuf>,
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    let m = std::fs::symlink_metadata(from)?;
    if m.file_type().is_symlink() {
        std::os::unix::fs::symlink(std::fs::read_link(from)?, to)
    } else if m.is_dir() {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            copy_tree(&e.path(), &to.join(e.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}

impl Replacement {
    /// `beginRepositoryWikiReplacement`: openwiki/ backed up and emptied
    /// but for INSTRUCTIONS.md.
    fn begin(root: &Path) -> Result<Replacement> {
        let wiki = root.join("openwiki");
        let m = match std::fs::symlink_metadata(&wiki) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Replacement { wiki, backup_parent: None }),
            Err(e) => return Err(other(e.to_string())),
        };
        if !m.is_dir() {
            return Err(other("Refusing to replace openwiki: expected a real directory below the repository root."));
        }
        let parent = std::env::temp_dir().join(format!("openwiki-init-{}", &util::random_uuid()[..6]));
        std::fs::create_dir(&parent).map_err(|e| other(e.to_string()))?;
        let backup = parent.join("openwiki");
        if let Err(e) = copy_tree(&wiki, &backup) {
            let _ = std::fs::remove_dir_all(&parent);
            return Err(other(e.to_string()));
        }
        let r = Replacement { wiki: wiki.clone(), backup_parent: Some(parent) };
        let init = (|| -> std::io::Result<()> {
            std::fs::remove_dir_all(&wiki)?;
            std::fs::create_dir_all(&wiki)?;
            let instructions = backup.join("INSTRUCTIONS.md");
            match std::fs::symlink_metadata(&instructions) {
                Ok(m) if m.is_file() => std::fs::copy(&instructions, wiki.join("INSTRUCTIONS.md")).map(|_| ()),
                Ok(_) => Err(std::io::Error::other("Refusing to preserve openwiki/INSTRUCTIONS.md: expected a regular file.")),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            }
        })();
        if let Err(e) = init {
            r.rollback();
            return Err(other(e.to_string()));
        }
        Ok(r)
    }

    fn commit(&self) {
        if let Some(p) = &self.backup_parent {
            let _ = std::fs::remove_dir_all(p);
        }
    }

    fn rollback(&self) {
        if let Some(p) = &self.backup_parent {
            let _ = std::fs::remove_dir_all(&self.wiki);
            if copy_tree(&p.join("openwiki"), &self.wiki).is_ok() {
                let _ = std::fs::remove_dir_all(p);
            }
        }
    }
}

// ---- Plans ----

fn plan_page_path(value: &str) -> Result<String> {
    let invalid = || fail("invalid_input", format!("Invalid or reserved OpenWiki page path: {value}"));
    let c = store::normalize_tool_page(value).and_then(|p| store::normalize_page(&p)).map_err(|_| invalid())?;
    if c.rsplit('/').next().unwrap_or("").starts_with('_') {
        return Err(invalid());
    }
    Ok(c)
}

fn seed_path(value: &str) -> Result<String> {
    let t = crate::jsre::trim(value).replace('\\', "/");
    let t = t.trim_start_matches('/');
    if t.is_empty() || t.split('/').any(|p| p == "..") {
        return Err(fail("invalid_input", format!("Invalid seed path: {value}")));
    }
    Ok(t.to_owned())
}

fn evidence_seed(resource: &str) -> Result<String> {
    static L: std::sync::LazyLock<crate::jsre::Re> = std::sync::LazyLock::new(|| crate::jsre::Re::new(r"#L\d+(?:-L\d+)?$", "u"));
    let without = resource.strip_prefix("repo://").unwrap_or(resource);
    seed_path(&L.replace(without, false, |_| String::new()))
}

fn title_from_path(page: &str) -> String {
    let base = page.rsplit('/').next().unwrap_or(page);
    let base = base.strip_suffix(".md").unwrap_or(base);
    base.split(['-', '_'])
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            let first = c.next().unwrap();
            format!("{}{}", first.to_uppercase(), c.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn job(path: &str, title: &str, purpose: &str, seeds: Vec<String>, related: Vec<String>, instructions: Vec<String>) -> Value {
    json!({"id": util::new_id(), "path": path, "title": title, "purpose": purpose, "seedPaths": seeds, "relatedPages": related, "instructions": instructions, "status": "pending"})
}

/// `createRepositoryPlan`.
fn create_plan(mode: &str, input: &Value, issues: &[claims::Issue], required: &[String]) -> Result<Value> {
    let mut pages: Vec<Value> = Vec::new();
    for p in input["pages"].as_array().into_iter().flatten() {
        let path = plan_page_path(p["path"].as_str().unwrap_or(""))?;
        let title = crate::jsre::trim(p["title"].as_str().unwrap_or("")).to_owned();
        let purpose = crate::jsre::trim(p["purpose"].as_str().unwrap_or("")).to_owned();
        if title.is_empty() || purpose.is_empty() {
            return Err(fail("invalid_input", format!("Planned page requires title and purpose: {path}")));
        }
        let id = util::new_id();
        let seeds = unique_sorted(strings(&p["seedPaths"]).iter().map(|x| seed_path(x)).collect::<Result<Vec<_>>>()?);
        let related = unique_sorted(strings(&p["relatedPages"]).iter().map(|x| plan_page_path(x)).collect::<Result<Vec<_>>>()?);
        let instructions = unique_sorted(strings(&p["instructions"]).iter().map(|x| crate::jsre::trim(x).to_owned()).filter(|x| !x.is_empty()).collect());
        pages.push(json!({"id": id, "path": path, "title": title, "purpose": purpose, "seedPaths": seeds, "relatedPages": related, "instructions": instructions, "status": "pending"}));
    }
    let deletes = unique_sorted(strings(&input["deletePages"]).iter().map(|x| plan_page_path(x)).collect::<Result<Vec<_>>>()?);
    if mode == "init" && !deletes.is_empty() {
        return Err(fail("invalid_input", "Init plans cannot delete generated pages."));
    }
    if deletes.iter().any(|d| d == QUICKSTART) {
        return Err(fail("invalid_input", "The canonical /openwiki/quickstart.md page cannot be deleted."));
    }
    let mut planned: HashSet<String> = HashSet::new();
    for p in &pages {
        let path = p["path"].as_str().unwrap().to_owned();
        if !planned.insert(path.clone()) {
            return Err(fail("invalid_input", format!("Duplicate planned page: {path}")));
        }
    }
    for d in &deletes {
        if planned.contains(d) {
            return Err(fail("invalid_input", format!("A page cannot be both generated and deleted: {d}")));
        }
    }
    if mode == "init" && !planned.contains(QUICKSTART) {
        return Err(fail("invalid_input", "Init plan must include /openwiki/quickstart.md."));
    }
    let defer = input["deferUnplannedClaimIssues"].as_bool() == Some(true);
    if mode == "update" {
        let deleted: HashSet<&String> = deletes.iter().collect();
        if !defer {
            let mut grouped: Vec<(String, Vec<&claims::Issue>)> = Vec::new();
            for i in issues {
                let page = store::normalize_page(&i.page)?;
                match grouped.iter_mut().find(|(p, _)| *p == page) {
                    Some((_, l)) => l.push(i),
                    None => grouped.push((page, vec![i])),
                }
            }
            for (page, list) in grouped {
                if planned.contains(&page) || deleted.contains(&page) {
                    continue;
                }
                let seeds = unique_sorted(list.iter().flat_map(|i| i.resources.iter().map(|r| evidence_seed(r))).collect::<Result<Vec<_>>>()?);
                pages.push(job(&page, &title_from_path(&page), "Reconcile stale or unresolved Claims and update this page from current repository evidence while preserving unaffected accurate content.", seeds, vec![], vec![]));
                planned.insert(page);
            }
        }
        for p in required {
            let page = store::normalize_page(p)?;
            if planned.contains(&page) || deleted.contains(&page) {
                continue;
            }
            pages.push(job(&page, &title_from_path(&page), "Rewrite this existing page in the run's target language while preserving every accurate repository-supported fact and reconciling its complete Claim set.", vec![], vec![], vec![]));
            planned.insert(page);
        }
    } else if defer {
        return Err(fail("invalid_input", "Init cannot defer Claim issues."));
    }
    pages.sort_by(|a, b| {
        let (pa, pb) = (a["path"].as_str().unwrap(), b["path"].as_str().unwrap());
        (pa == QUICKSTART).cmp(&(pb == QUICKSTART)).then_with(|| cmp16(pa, pb))
    });
    let deferred: Vec<String> = if defer {
        unique_sorted(issues.iter().map(|i| store::normalize_page(&i.page)).collect::<std::result::Result<Vec<_>, _>>()?).into_iter().filter(|p| !planned.contains(p) && !deletes.contains(p)).collect()
    } else {
        Vec::new()
    };
    Ok(json!({"pages": pages, "deletePages": deletes, "deferredClaimPages": deferred}))
}

fn simplified(plan: &Value) -> Value {
    let pages: Vec<Value> = plan["pages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| json!({"path": p["path"], "title": p["title"], "purpose": p["purpose"], "seedPaths": p["seedPaths"], "relatedPages": p["relatedPages"], "instructions": p["instructions"]}))
        .collect();
    json!({"pages": pages, "deletePages": plan["deletePages"], "deferredClaimPages": plan.get("deferredClaimPages").cloned().unwrap_or(json!([]))})
}

// ---- Claims of a submitted page ----

fn same_set(a: &[String], b: &[String]) -> bool {
    unique_sorted(a.to_vec()) == unique_sorted(b.to_vec())
}

/// `reconcilePageClaims`: the writer's sparse decisions made a batch.
fn reconcile_claims(session: &mut claims::Session, page_input: &str, input: &Value) -> Result<()> {
    let page = store::normalize_page(page_input)?;
    let existing = session.inspect(&page)?;
    let ev = |c: &Value| strings(&c["evidence"]);
    let mut targeted: HashSet<String> = HashSet::new();
    let mut fingerprints: HashSet<String> = HashSet::new();
    let mut ops: Vec<Op> = Vec::new();
    let target = |targeted: &mut HashSet<String>, raw: &str, decision: &str| -> Result<Value> {
        let id = crate::jsre::trim(raw).to_owned();
        let Some(c) = existing.iter().find(|c| c["id"] == id.as_str()) else {
            return Err(fail("invalid_input", format!("Claim {} is not owned by {page}.", if id.is_empty() { "(empty)" } else { &id })));
        };
        if !targeted.insert(id.clone()) {
            return Err(fail("invalid_input", format!("Claim {id} has more than one reconciliation decision for {page} ({decision}).")));
        }
        Ok(c.clone())
    };
    for id in strings(&input["confirmedClaimIds"]) {
        let c = target(&mut targeted, &id, "confirm")?;
        ops.push(Op::Confirm { id: s(&c, "id").unwrap_or_default() });
    }
    for raw in input["claims"].as_array().into_iter().flatten() {
        let statement = crate::jsre::trim(raw["statement"].as_str().unwrap_or("")).to_owned();
        if statement.is_empty() {
            return Err(fail("invalid_input", "Claim statement is empty."));
        }
        let resources = unique_sorted(raw["evidence"].as_array().into_iter().flatten().filter_map(|e| e["resource"].as_str()).map(|r| crate::jsre::trim(r).to_owned()).filter(|r| !r.is_empty()).collect());
        if resources.is_empty() {
            return Err(fail("invalid_input", format!("Claim requires repository evidence: {statement}")));
        }
        let fp = serde_json::to_string(&json!([statement, resources])).unwrap_or_default();
        if !fingerprints.insert(fp) {
            return Err(fail("invalid_input", format!("Duplicate proposed Claim for {page}: {statement}")));
        }
        let id = raw["id"].as_str().map(|i| crate::jsre::trim(i).to_owned()).filter(|i| !i.is_empty());
        if let Some(id) = id {
            let c = target(&mut targeted, &id, "update")?;
            let cid = s(&c, "id").unwrap_or_default();
            let same_statement = s(&c, "statement").as_deref() == Some(statement.as_str());
            let same_evidence = same_set(&ev(&c), &resources);
            if same_statement && same_evidence {
                ops.push(Op::Confirm { id: cid });
            } else {
                ops.push(Op::Update { id: cid, statement: (!same_statement).then(|| statement.clone()), evidence: (!same_evidence).then(|| resources.clone()) });
            }
            continue;
        }
        match existing.iter().find(|c| s(c, "statement").as_deref() == Some(statement.as_str()) && same_set(&ev(c), &resources)) {
            Some(exact) => {
                let c = target(&mut targeted, exact["id"].as_str().unwrap_or(""), "confirm")?;
                ops.push(Op::Confirm { id: s(&c, "id").unwrap_or_default() });
            }
            None => ops.push(Op::Add { statement, evidence: resources }),
        }
    }
    for raw in strings(&input["retractedClaimIds"]) {
        let id = crate::jsre::trim(&raw).to_owned();
        let known = existing.iter().any(|c| c["id"] == id.as_str());
        if !known && !id.is_empty() && !session.has_claim(&id) {
            continue;
        }
        let c = target(&mut targeted, &id, "retract")?;
        ops.push(Op::Retract { id: s(&c, "id").unwrap_or_default() });
    }
    for c in &existing {
        let id = s(c, "id").unwrap_or_default();
        if targeted.contains(&id) {
            continue;
        }
        if let Some(kind) = c.get("issue").and_then(|i| i["kind"].as_str()) {
            return Err(fail("invalid_input", format!("Claim {id} is {kind} and requires an explicit confirm, update, or retract decision.")));
        }
        ops.push(Op::Confirm { id });
    }
    let retracted = ops.iter().filter(|o| matches!(o, Op::Retract { .. })).count();
    let added = ops.iter().filter(|o| matches!(o, Op::Add { .. })).count();
    if existing.len() + added == retracted {
        return Err(fail("invalid_input", format!("Completed factual page {page} must retain or establish at least one material Claim.")));
    }
    session.resolve_claims(&page, &ops)?;
    Ok(())
}

// ---- The run ----

pub struct Run {
    root: PathBuf,
    state: Value,
    backend: Backend,
    ignore: Ignore,
    claims: claims::Runtime,
}

fn checkpoint(state: &Value) -> Checkpoint {
    Checkpoint { fingerprint: s(state, "sourceFingerprint"), git_head: s(state, "targetGitHead") }
}

fn pending(plan: &Value) -> usize {
    plan["pages"].as_array().map_or(0, |a| a.iter().filter(|p| p["status"] == "pending").count())
}

fn not_found_thrown(e: &ReadError) -> bool {
    matches!(e, ReadError::Thrown(m) if m.starts_with("ENOENT"))
}

impl Run {
    fn st(&self) -> &str {
        self.state["mode"].as_str().unwrap_or("update")
    }

    fn language(&self) -> String {
        s(&self.state, "language").unwrap_or_else(|| "en".into())
    }

    fn actor(&self) -> String {
        self.state["actor"]["producerActor"].as_str().unwrap_or("").to_owned()
    }

    fn model(&self) -> String {
        self.state["actor"]["metadataModel"].as_str().unwrap_or("").to_owned()
    }

    fn run_id(&self) -> String {
        s(&self.state, "runId").unwrap_or_default()
    }

    fn base_head(&self) -> Option<String> {
        s(&self.state, "baseGitHead")
    }

    fn set_state(&mut self, next: Value) -> Result<()> {
        write_state(&self.root, &next)?;
        self.state = next;
        Ok(())
    }

    /// The pending job with this id.
    fn pending_job(&self, id: &str, action: &str) -> Result<Value> {
        self.state["plan"]["pages"]
            .as_array()
            .and_then(|a| a.iter().find(|p| p["id"] == id))
            .filter(|p| p["status"] == "pending")
            .cloned()
            .ok_or_else(|| fail("invalid_state", format!("Only a pending OpenWiki page job may be {action}.")))
    }

    fn active_view(&mut self, resumed: bool) -> Result<Value> {
        let pages = self.state["plan"]["pages"].as_array().cloned().unwrap_or_default();
        let windows = if self.st() == "update" { update_windows(&self.root, &self.ignore, &strings(&self.state["initialPages"]))? } else { Vec::new() };
        let changed = unique_sorted(windows.iter().flat_map(|w| strings(&w["changedPaths"])).collect());
        let mut v = Map::new();
        v.insert("status".into(), "active".into());
        v.insert("runId".into(), self.state["runId"].clone());
        v.insert("root".into(), self.root.to_string_lossy().into_owned().into());
        v.insert("mode".into(), self.state["mode"].clone());
        v.insert("language".into(), self.state["language"].clone());
        v.insert("languageChanged".into(), self.state["languageChanged"].clone());
        v.insert("phase".into(), self.state["phase"].clone());
        v.insert("resumed".into(), resumed.into());
        v.insert("lastUpdate".into(), self.state["previousLastUpdate"].clone());
        if let Some(g) = s(&self.state, "wikiGoal").filter(|g| !g.is_empty()) {
            v.insert("wikiGoal".into(), g.into());
        }
        v.insert("changedPaths".into(), json!(changed));
        v.insert("pageUpdateWindows".into(), Value::Array(windows));
        v.insert("claimIssues".into(), Value::Array(self.claims.issues.iter().map(claims::Issue::to_json).collect()));
        v.insert("completedPages".into(), pages.iter().filter(|p| p["status"] == "complete").count().into());
        if self.state.get("plan").is_some() {
            v.insert("totalPages".into(), pages.len().into());
        }
        Ok(host_view(Value::Object(v)))
    }
}

/// `toHostActiveBeginView`: long lists capped, with their totals.
fn host_view(mut v: Value) -> Value {
    const PATHS: usize = 200;
    const ISSUES: usize = 50;
    let changed = strings(&v["changedPaths"]);
    let issues = v["claimIssues"].as_array().cloned().unwrap_or_default();
    let windows = v["pageUpdateWindows"].as_array().cloned().unwrap_or_default();
    let per_window = if windows.is_empty() { usize::MAX } else { PATHS / windows.len() };
    v["changedPaths"] = json!(changed.iter().take(PATHS).collect::<Vec<_>>());
    v["changedPathCount"] = changed.len().into();
    v["changedPathsTruncated"] = (changed.len() > PATHS).into();
    v["pageUpdateWindows"] = Value::Array(
        windows
            .into_iter()
            .map(|mut w| {
                let c = strings(&w["changedPaths"]);
                w["changedPaths"] = json!(c.iter().take(per_window).collect::<Vec<_>>());
                w["changedPathCount"] = c.len().into();
                w["changedPathsTruncated"] = (c.len() > per_window).into();
                w
            })
            .collect(),
    );
    v["claimIssues"] = Value::Array(issues.iter().take(ISSUES).cloned().collect());
    v["claimIssueCount"] = issues.len().into();
    v["claimIssuesTruncated"] = (issues.len() > ISSUES).into();
    v["claimIssuePages"] = json!(unique_sorted(issues.iter().filter_map(|i| s(i, "page")).collect()));
    v
}

fn update_windows(root: &Path, ignore: &Ignore, pages: &[String]) -> Result<Vec<Value>> {
    let m = read_manifest(root)?;
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for p in pages {
        let base = m.get(p).and_then(|e| s(e, "gitHead")).unwrap_or_default();
        match groups.iter_mut().find(|(b, _)| *b == base) {
            Some((_, l)) => l.push(p.clone()),
            None => groups.push((base, vec![p.clone()])),
        }
    }
    groups.sort_by(|a, b| cmp16(&a.0, &b.0));
    let mut out = Vec::new();
    for (base, mut list) in groups {
        let full = base.is_empty() || !repo::is_readable_commit(root, &base);
        list.sort_by(|a, b| cmp16(a, b));
        let mut w = Map::new();
        if !base.is_empty() {
            w.insert("baseGitHead".into(), base.clone().into());
        }
        w.insert("pages".into(), json!(list));
        w.insert("changedPaths".into(), json!(repo::changed_paths(root, ignore, (!base.is_empty()).then_some(base.as_str()))));
        w.insert("commits".into(), Value::Array(if full { Vec::new() } else { repo::commit_context(root, ignore, Some(&base)) }));
        w.insert("fullReview".into(), full.into());
        out.push(Value::Object(w));
    }
    Ok(out)
}

fn fast_forward(root: &Path, ignore: &Ignore, pages: &[String], source: &Checkpoint) -> Result<()> {
    let Some(head) = &source.git_head else { return Ok(()) };
    let m = read_manifest(root)?;
    let mut unchanged: HashMap<String, bool> = HashMap::new();
    let mut sorted = pages.to_vec();
    sorted.sort_by(|a, b| cmp16(a, b));
    for page in sorted {
        let Some(entry) = m.get(&page) else { continue };
        let Some(base) = s(entry, "gitHead") else { continue };
        if base.is_empty() || &base == head {
            continue;
        }
        let same = *unchanged.entry(base.clone()).or_insert_with(|| repo::is_readable_commit(root, &base) && repo::changed_paths(root, ignore, Some(&base)).is_empty());
        if !same {
            continue;
        }
        record_completion(root, &page, source, s(entry, "completedBy").as_deref(), s(entry, "completedRunId").as_deref())?;
    }
    Ok(())
}

/// `reconcileManifestPageJobs`: jobs a manifest shows done (by this run)
/// marked complete, and the other way round.
fn reconcile_manifest_jobs(root: &Path, state: Value, legacy_by: &str) -> Result<Value> {
    if state.get("plan").is_none() {
        return Ok(state);
    }
    let source = checkpoint(&state);
    let run_id = s(&state, "runId").unwrap_or_default();
    let mut changed = false;
    let mut pages = Vec::new();
    for page in state["plan"]["pages"].as_array().cloned().unwrap_or_default() {
        let path = s(&page, "path").unwrap_or_default();
        let status = s(&page, "status").unwrap_or_default();
        let by = s(&page, "completedBy").unwrap_or_else(|| legacy_by.to_owned());
        let mut current = current_completion(root, &path, &source)?;
        if status == "complete" && current.is_none() {
            current = Some(record_completion(root, &path, &source, Some(&by), Some(&run_id))?);
        }
        if status == "complete" && current.as_ref().is_some_and(|c| s(c, "completedBy").is_none() || s(c, "completedRunId").as_deref() != Some(run_id.as_str())) {
            current = Some(record_completion(root, &path, &source, Some(&by), Some(&run_id))?);
        }
        if status == "pending" && current.as_ref().and_then(|c| s(c, "completedRunId")).as_deref() == Some(run_id.as_str()) {
            let mut p = page.clone();
            p["status"] = "complete".into();
            p["completedBy"] = current.as_ref().and_then(|c| s(c, "completedBy")).unwrap_or_else(|| legacy_by.to_owned()).into();
            pages.push(p);
            changed = true;
        } else if status == "complete" && page.get("completedBy").is_none() {
            let mut p = page.clone();
            p["completedBy"] = current.as_ref().and_then(|c| s(c, "completedBy")).unwrap_or_else(|| legacy_by.to_owned()).into();
            pages.push(p);
            changed = true;
        } else {
            pages.push(page);
        }
    }
    if !changed {
        return Ok(state);
    }
    let mut next = state;
    next["plan"]["pages"] = Value::Array(pages);
    Ok(next)
}

// ---- The lifecycle ----

pub struct Begin {
    pub root: PathBuf,
    pub mode: String,
    pub language: Option<String>,
    pub force: bool,
    pub producer: String,
    pub model: String,
}

pub enum Begun {
    Noop(Value),
    Active(Box<Run>, Value),
}

fn prepare_claims(root: &Path, ignore: &Ignore, mode: &str, resume_init: bool) -> Result<claims::Runtime> {
    claims::prepare(root, ignore, mode == "init" && !resume_init)
}

/// `beginRepositoryRun`.
pub fn begin(input: &Begin) -> Result<Begun> {
    let requested = match lang::resolve(input.language.as_deref()) {
        lang::Resolved::Unrecognized(m) => return Err(fail("invalid_input", m)),
        lang::Resolved::Language(l) => Some(l),
        lang::Resolved::Absent => None,
    };
    let root = &input.root;
    code_mode_setup(root, input.mode == "init")?;
    if let Some(persisted) = read_state(root)? {
        return resume(input, requested, persisted);
    }
    let last = repo::read_last_update(root).map_err(other)?;
    let language = requested.clone().or_else(|| last.as_ref().and_then(|l| l.language.clone())).unwrap_or_else(|| "en".into());
    let goal = repo::wiki_goal(root).map_err(other)?;
    let language_changed = requested.is_some() && lang::primary(requested.as_deref()) != lang::primary(last.as_ref().and_then(|l| l.language.as_deref()));
    let ignore = Ignore::load(root).map_err(|e| other(e.to_string()))?;
    let replacement = if input.mode == "init" { Some(Replacement::begin(root)?) } else { None };
    let mut written = false;
    let result = (|| -> Result<Begun> {
        let backend = Backend::new(root, ignore.clone());
        let mut rt = prepare_claims(root, &ignore, &input.mode, false)?;
        let initial = Store::new(root).discover_pages()?;
        let source = repo::source_snapshot(root, &ignore).map_err(other)?;
        let update = input.mode == "update";
        if update {
            if let Some(l) = last.as_ref().filter(|l| l.status == "complete") {
                if let Some(h) = &l.git_head {
                    seed_manifest(root, &initial, h)?;
                }
            }
            fast_forward(root, &ignore, &initial, &Checkpoint::of(&source))?;
        }
        let complete_coverage = !update || {
            let m = read_manifest(root)?;
            initial.iter().all(|p| m.contains_key(p))
        };
        if update && !input.force {
            if let Noop::Skip { git_head, model, language: noop_language } = repo::update_noop(root, &ignore, input.language.as_deref()).map_err(other)? {
                if rt.issues.is_empty() && complete_coverage {
                    let first = repo::source_snapshot(root, &ignore).map_err(other)?;
                    rt.session.finalize(&rt.store, &util::now_iso(), &HashSet::new())?;
                    let stable = repo::source_snapshot(root, &ignore).map_err(other)?;
                    if stable.fingerprint == first.fingerprint {
                        replace_manifest(root, &initial, &Checkpoint::of(&first), &HashSet::new(), &HashSet::new())?;
                        let published = repo::source_snapshot(root, &ignore).map_err(other)?;
                        if published.fingerprint == first.fingerprint {
                            repo::write_last_update(root, "update", &model, "complete", noop_language.as_deref(), None).map_err(other)?;
                            let mut pre = json!({"shouldSkip": true, "gitHead": git_head, "model": model});
                            if let Some(l) = noop_language {
                                pre["language"] = l.into();
                            }
                            return Ok(Begun::Noop(json!({"status": "noop", "root": root.to_string_lossy(), "mode": "update", "language": language, "updatePreflight": pre})));
                        }
                    }
                }
            }
        }
        let before = repo::content_snapshot(root).map_err(other)?;
        let prepared = finish::prepare(&backend, finish::concept_type(Some(&language))).map_err(|(_, e)| other(e))?;
        let required: Vec<String> = if update && language_changed { initial.clone() } else { Vec::new() };
        let mut st = Map::new();
        st.insert("schemaVersion".into(), 1.into());
        st.insert("runId".into(), util::new_id().into());
        st.insert("mode".into(), input.mode.clone().into());
        st.insert("phase".into(), "planning".into());
        st.insert("startedAt".into(), util::now_iso().into());
        st.insert("language".into(), language.clone().into());
        st.insert("languageChanged".into(), language_changed.into());
        st.insert("requiredRewritePages".into(), json!(required));
        st.insert("initialPages".into(), json!(initial));
        st.insert("sourceFingerprint".into(), source.fingerprint.clone().into());
        if let Some(h) = &source.git_head {
            st.insert("targetGitHead".into(), h.clone().into());
        }
        st.insert("actor".into(), json!({"producerActor": input.producer, "metadataModel": input.model}));
        st.insert("previousLastUpdate".into(), last.as_ref().map(LastUpdate::to_json).unwrap_or(Value::Null));
        if let Some(h) = last.as_ref().and_then(|l| l.git_head.clone()) {
            st.insert("baseGitHead".into(), h.into());
        }
        if let Some(g) = goal.filter(|g| !g.is_empty()) {
            st.insert("wikiGoal".into(), g.into());
        }
        st.insert("beforeContentSnapshot".into(), before.into());
        st.insert("preparedWiki".into(), json!({"generatedProvenance": finish::serialize_provenance(&prepared)}));
        let state = Value::Object(st);
        write_state(root, &state)?;
        written = true;
        repo::write_last_update(root, &input.mode, &input.model, "interrupted", Some(&language), Some(last.as_ref().and_then(|l| l.git_head.as_deref()))).map_err(other)?;
        if let Some(r) = &replacement {
            r.commit();
        }
        let mut run = Run { root: root.clone(), state, backend, ignore: ignore.clone(), claims: rt };
        let view = run.active_view(false)?;
        Ok(Begun::Active(Box::new(run), view))
    })();
    if result.is_err() {
        if let Some(r) = &replacement {
            if written {
                remove_state(root);
            }
            r.rollback();
        }
    }
    result
}

fn resume(input: &Begin, requested: Option<String>, state: Value) -> Result<Begun> {
    let root = &input.root;
    let mode = s(&state, "mode").unwrap_or_default();
    if mode != input.mode {
        return Err(fail("conflict", format!("An interrupted OpenWiki {mode} run already exists. Resume that run before starting {}.", input.mode)));
    }
    let language = s(&state, "language").unwrap_or_default();
    if let Some(r) = requested.filter(|r| *r != language) {
        return Err(fail("conflict", format!("Interrupted OpenWiki run uses {language}; resume it before changing the wiki language to {r}.")));
    }
    let ignore = Ignore::load(root).map_err(|e| other(e.to_string()))?;
    let current = repo::source_snapshot(root, &ignore).map_err(other)?;
    let source_changed = Some(&current.fingerprint) != s(&state, "sourceFingerprint").as_ref();
    let mut next = state.clone();
    next["actor"] = json!({"producerActor": input.producer, "metadataModel": input.model});
    if let Some(pages) = next.get_mut("plan").and_then(|p| p.get_mut("pages")).and_then(Value::as_array_mut) {
        for p in pages.iter_mut().filter(|p| p["status"] == "skipped") {
            p["status"] = "pending".into();
        }
    }
    if source_changed {
        next["phase"] = "planning".into();
        next["sourceFingerprint"] = current.fingerprint.clone().into();
        let o = next.as_object_mut().unwrap();
        match &current.git_head {
            Some(h) => {
                o.insert("targetGitHead".into(), h.clone().into());
            }
            None => {
                o.shift_remove("targetGitHead");
            }
        }
        o.shift_remove("plan");
    } else {
        if next.get("targetGitHead").is_none() {
            if let Some(h) = &current.git_head {
                next.as_object_mut().unwrap().insert("targetGitHead".into(), h.clone().into());
            }
        }
        let legacy = state["actor"]["producerActor"].as_str().unwrap_or("").to_owned();
        next = reconcile_manifest_jobs(root, next, &legacy)?;
    }
    let active = if serde_json::to_string(&next).ok() != serde_json::to_string(&state).ok() {
        write_state(root, &next)?;
        next
    } else {
        state
    };
    let backend = Backend::new(root, ignore.clone());
    let rt = prepare_claims(root, &ignore, &mode, mode == "init")?;
    let mut run = Run { root: root.clone(), state: active, backend, ignore, claims: rt };
    let view = run.active_view(true)?;
    Ok(Begun::Active(Box::new(run), view))
}

impl Run {
    /// `submitRepositoryPlan`.
    pub fn submit_plan(&mut self, input: &Value) -> Result<Value> {
        let required = strings(&self.state["requiredRewritePages"]);
        if self.state.get("plan").is_some() {
            let proposed = create_plan(self.st(), input, &self.claims.issues, &required)?;
            if simplified(&self.state["plan"]) != simplified(&proposed) {
                return Err(fail("invalid_state", "This OpenWiki run already has a different persisted plan."));
            }
            return Ok(json!({"status": "accepted", "totalPages": self.state["plan"]["pages"].as_array().map_or(0, Vec::len)}));
        }
        if self.state["phase"] != "planning" {
            return Err(fail("invalid_state", "OpenWiki plan can only be submitted during planning."));
        }
        let plan = create_plan(self.st(), input, &self.claims.issues, &required)?;
        let total = plan["pages"].as_array().map_or(0, Vec::len);
        let mut next = self.state.clone();
        next["phase"] = "generating".into();
        next["plan"] = plan;
        self.set_state(next)?;
        Ok(json!({"status": "accepted", "totalPages": total}))
    }

    /// `nextRepositoryPage`.
    pub fn next_page(&mut self) -> Result<Value> {
        if self.state.get("plan").is_none() || self.state["phase"] != "generating" {
            return Err(fail("invalid_state", "Submit the OpenWiki plan before requesting page work."));
        }
        let Some(job) = self.state["plan"]["pages"].as_array().and_then(|a| a.iter().find(|p| p["status"] == "pending")).cloned() else {
            return Ok(json!({"status": "complete"}));
        };
        let path = s(&job, "path").unwrap_or_default();
        let existing = match self.backend.read_raw(&path) {
            Ok(_) => true,
            Err(ReadError::Error(_)) => false,
            Err(e) if not_found_thrown(&e) => false,
            Err(e) => return Err(other(e.text().to_owned())),
        };
        let claims = self.claims.session.inspect(&path)?;
        let mut j = job;
        j["mode"] = self.state["mode"].clone();
        j["existing"] = existing.into();
        j["existingClaimCount"] = claims.len().into();
        j["claimsRequiringAttention"] = Value::Array(claims.into_iter().filter(|c| c.get("issue").is_some()).collect());
        Ok(json!({"status": "pending", "job": j}))
    }

    /// `inspectRepositoryPageClaims`.
    pub fn inspect(&mut self, job_id: &str) -> Result<Value> {
        let job = self.pending_job(job_id, "inspected")?;
        let page = s(&job, "path").unwrap_or_default();
        let claims = self.claims.session.inspect(&page)?;
        Ok(json!({"page": page, "claims": claims}))
    }

    /// `submitRepositoryPage`.
    pub fn submit_page(&mut self, input: &Value) -> Result<Value> {
        if self.state.get("plan").is_none() || self.state["phase"] != "generating" {
            return Err(fail("invalid_state", "Submit the OpenWiki plan before submitting pages."));
        }
        let job_id = s(input, "jobId").unwrap_or_default();
        let Some(requested) = self.state["plan"]["pages"].as_array().and_then(|a| a.iter().find(|p| p["id"] == job_id.as_str())).cloned() else {
            return Err(fail("not_found", "Unknown OpenWiki page job."));
        };
        let path = s(&requested, "path").unwrap_or_default();
        if requested["status"] == "complete" {
            return Ok(json!({"status": "complete", "page": path, "remaining": pending(&self.state["plan"])}));
        }
        self.pending_job(&job_id, "submitted")?;
        let readable = match self.backend.read_raw(&path) {
            Ok(_) => true,
            Err(ReadError::Error(_)) => false,
            Err(e) if not_found_thrown(&e) => false,
            Err(e) => return Err(other(e.text().to_owned())),
        };
        if !readable {
            return Err(fail("invalid_input", format!("Write {path} before submitting its page job.")));
        }
        let issues = self.repair_persisted(&path);
        if !issues.is_empty() {
            return Err(fail("invalid_input", format!("Could not deterministically repair front matter in {path}: {}", crate::okf::describe(&issues))));
        }
        let plan = self.state["plan"].clone();
        let others: HashSet<String> = plan["pages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p["status"] == "pending" && p["id"] != job_id.as_str())
            .filter_map(|p| s(p, "path"))
            .chain(strings(&plan["deferredClaimPages"]))
            .collect();
        let started = s(&self.state, "startedAt").unwrap_or_default();
        let checked = (|| -> Result<()> {
            reconcile_claims(&mut self.claims.session, &path, input)?;
            self.claims.session.finalize(&self.claims.store, &started, &others)?;
            self.durable(&path, "submit_page")
        })();
        if let Err(e) = checked {
            return Err(match e {
                Error::Run(..) => e,
                other => fail("invalid_input", claims::message(&other)),
            });
        }
        let actor = self.actor();
        let run_id = self.run_id();
        record_completion(&self.root, &path, &checkpoint(&self.state), Some(&actor), Some(&run_id))?;
        let mut next = self.state.clone();
        for p in next["plan"]["pages"].as_array_mut().into_iter().flatten() {
            if p["id"] == job_id.as_str() {
                p["status"] = "complete".into();
                p["completedBy"] = actor.clone().into();
            }
        }
        let remaining = pending(&next["plan"]);
        self.set_state(next)?;
        Ok(json!({"status": "complete", "page": path, "remaining": remaining}))
    }

    /// `repairPersistedFile`: the issues left after repairing the page's
    /// front matter (none: it's valid).
    fn repair_persisted(&self, path: &str) -> crate::okf::Validation {
        let read_failed = |m: String| vec![crate::okf::Issue { code: "file_read_failed".into(), line: None, message: format!("Could not read the final Markdown text: {m}.") }];
        let content = match self.backend.read_raw(path) {
            Ok(c) => c,
            Err(e) => return read_failed(e.text().to_owned()),
        };
        let (changed, repaired) = crate::okf::repair(&content, path, finish::concept_type(Some(&self.language())));
        if !changed {
            return crate::okf::validate(&content);
        }
        if let Err(e) = self.backend.write(path, &repaired) {
            return vec![crate::okf::Issue { code: "file_write_failed".into(), line: None, message: format!("Could not persist repaired Markdown text: {e}.") }];
        }
        match self.backend.read_raw(path) {
            Ok(c) => crate::okf::validate(&c),
            Err(e) => read_failed(e.text().to_owned()),
        }
    }

    /// `assertPageClaimsDurable`.
    fn durable(&mut self, page: &str, retry: &str) -> Result<()> {
        let st = &self.claims.store;
        let invalid = |m: String| fail("invalid_state", m);
        let Some(persisted) = claims::load(st, page)? else {
            return Err(invalid(format!("Claims for {page} were not durably persisted; retry {retry}.")));
        };
        if persisted.page_version != st.hash_page(page)? {
            return Err(invalid(format!("Claims for {page} do not match the current Markdown bytes; retry {retry}.")));
        }
        let Some(v) = &persisted.verification else {
            return Err(invalid(format!("Claims for {page} were not verified; retry {retry}.")));
        };
        let verified = crate::okf::fields(&st.read_markdown(page)?).and_then(|f| f.get("verified").cloned());
        let events = match verified {
            Some(Value::Array(a)) => a,
            None => Vec::new(),
            Some(x) => vec![x],
        };
        if !events.iter().any(|e| e["by"].is_string() && e["at"].is_string() && e["by"] == v.by.as_str() && e["at"] == v.at.as_str()) {
            return Err(invalid(format!("Claims verification for {page} was not durably projected; retry {retry}.")));
        }
        let expected = self.claims.session.inspect(page)?;
        let mut by_id: HashMap<&str, &claims::Claim> = HashMap::new();
        for c in &persisted.claims {
            by_id.insert(c.id.as_str(), c);
        }
        let partial = || invalid(format!("Claims for {page} were only partially persisted; retry {retry}."));
        if by_id.len() != expected.len() {
            return Err(partial());
        }
        for c in &expected {
            let Some(d) = by_id.get(c["id"].as_str().unwrap_or("")) else { return Err(partial()) };
            let resources: Vec<String> = d.evidence.iter().map(|e| e.resource.clone()).collect();
            if c["statement"] != d.statement.as_str() || !same_set(&resources, &strings(&c["evidence"])) {
                return Err(partial());
            }
        }
        Ok(())
    }

    fn source_changed(&self) -> Result<bool> {
        let current = repo::source_snapshot(&self.root, &self.ignore).map_err(other)?;
        Ok(Some(current.fingerprint) != s(&self.state, "sourceFingerprint"))
    }

    fn delete_page(&mut self, page: &str, what: &str) -> Result<()> {
        if let Err(e) = self.backend.delete(page) {
            if !e.contains("not found") {
                return Err(fail("invalid_state", format!("{what} {page}: {e}")));
            }
        }
        self.claims.session.record_deletion(page)
    }

    /// `finishRepositoryRun`.
    pub fn finish(&mut self) -> Result<Value> {
        let Some(plan) = self.state.get("plan").cloned() else {
            return Err(fail("invalid_state", "OpenWiki cannot finish before a plan is submitted."));
        };
        let n = pending(&plan);
        if n > 0 {
            return Err(fail("invalid_state", format!("OpenWiki cannot finish with {n} pending page job(s).")));
        }
        let jobs = plan["pages"].as_array().cloned().unwrap_or_default();
        if jobs.iter().any(|p| p["status"] == "skipped") {
            return Err(fail("invalid_state", "Every skipped OpenWiki page job requires its original page snapshot before finish."));
        }
        let skipped: HashSet<String> = strings(&plan["deferredClaimPages"]).into_iter().collect();
        let changed_before = self.source_changed()?;
        let run_id = self.run_id();
        let mut actors: HashMap<String, String> = HashMap::new();
        for (page, e) in read_manifest(&self.root)? {
            if let Some(by) = s(&e, "completedBy").filter(|b| !b.is_empty()) {
                if s(&e, "completedRunId").as_deref() == Some(run_id.as_str()) {
                    actors.insert(page, by);
                }
            }
        }
        let st = Store::new(&self.root);
        let initial: HashSet<String> = strings(&self.state["initialPages"]).into_iter().collect();
        let planned: HashSet<String> = jobs.iter().filter_map(|p| s(p, "path")).collect();
        for page in st.discover_pages()? {
            if initial.contains(&page) || planned.contains(&page) {
                continue;
            }
            self.delete_page(&page, "Could not remove page abandoned by an invalidated plan")?;
        }
        for page in strings(&plan["deletePages"]) {
            self.delete_page(&page, "Could not delete planned OpenWiki page")?;
        }
        let current: HashSet<String> = st.discover_pages()?.into_iter().collect();
        for page in st.discover_sidecar_pages()? {
            if !current.contains(&page) {
                self.claims.session.record_deletion(&page)?;
            }
        }
        let language = self.language();
        let prepared: Vec<Snapshot> = finish::deserialize_provenance(&self.state["preparedWiki"]["generatedProvenance"]);
        let sources: Vec<(String, Vec<String>)> = self.claims.session.evidence_by_page().into_iter().filter(|(p, _)| !skipped.contains(p)).collect();
        let started = s(&self.state, "startedAt").unwrap_or_default();
        finish::finalize(&self.backend, Some(&language), &prepared, &started, &self.actor(), &actors, Some(&sources)).map_err(|(_, e)| other(e))?;
        self.claims.session.finalize(&self.claims.store, &started, &skipped)?;
        self.durable_all(&skipped)?;
        let pages = st.discover_pages()?;
        let preserve_source: HashSet<String> = pages.iter().filter_map(|p| store::normalize_page(p).ok()).filter(|p| !actors.contains_key(p) && !skipped.contains(p)).collect();
        replace_manifest(&self.root, &pages, &checkpoint(&self.state), &skipped, &preserve_source)?;
        let changed = changed_before || self.source_changed()?;
        let mode = self.st().to_owned();
        if !skipped.is_empty() || changed {
            repo::write_last_update(&self.root, &mode, &self.model(), "interrupted", Some(&language), Some(self.base_head().as_deref())).map_err(other)?;
        } else {
            repo::write_last_update(&self.root, &mode, &self.model(), "complete", Some(&language), None).map_err(other)?;
        }
        remove_state(&self.root);
        Ok(if changed { json!({"status": "complete", "sourceChanged": true}) } else { json!({"status": "complete"}) })
    }

    /// `assertRepositoryClaimsDurable`.
    fn durable_all(&mut self, excluded: &HashSet<String>) -> Result<()> {
        let st = Store::new(&self.root);
        let current: HashSet<String> = st.discover_pages()?.into_iter().collect();
        for page in st.discover_sidecar_pages()? {
            if !current.contains(&page) {
                return Err(fail("invalid_state", format!("Deleted or missing page {page} still has a Claims sidecar; retry finish.")));
            }
        }
        for (page, _) in self.claims.session.evidence_by_page() {
            if excluded.contains(&page) || self.claims.session.inspect(&page)?.is_empty() {
                continue;
            }
            if !current.contains(&page) {
                return Err(fail("invalid_state", format!("Claimed page {page} is missing; retry finish.")));
            }
            self.durable(&page, "finish")?;
        }
        Ok(())
    }
}

// ---- The tools' session ----

/// One host's writing session: at most one active run.
pub struct Writer {
    host: String,
    active: Option<Box<Run>>,
}

impl Writer {
    pub fn new(host: &str) -> Writer {
        Writer { host: host.to_owned(), active: None }
    }

    fn require(&mut self, run_id: &str) -> Result<&mut Run> {
        match self.active.as_deref_mut() {
            Some(r) if r.run_id() == run_id => Ok(r),
            _ => Err(Error::Host("invalid_state", "No matching OpenWiki run is active. Call openwiki_begin to start or resume the durable run first.".into())),
        }
    }

    /// One writing tool's answer, its input already checked.
    pub fn call(&mut self, name: &str, args: &Value) -> Result<Value> {
        let run_id = s(args, "runId").unwrap_or_default();
        match name {
            "openwiki_begin" => {
                let root = crate::tools::repository_root(args["root"].as_str().unwrap_or(""))?;
                let input = Begin {
                    root,
                    mode: s(args, "mode").unwrap_or_default(),
                    language: s(args, "language"),
                    force: args["force"].as_bool() == Some(true),
                    producer: self.host.clone(),
                    model: format!("host-agent/{}", self.host),
                };
                match begin(&input)? {
                    Begun::Noop(v) => {
                        self.active = None;
                        Ok(v)
                    }
                    Begun::Active(run, v) => {
                        self.active = Some(run);
                        Ok(v)
                    }
                }
            }
            "openwiki_submit_plan" => self.require(&run_id)?.submit_plan(args),
            "openwiki_next_page" => self.require(&run_id)?.next_page(),
            "openwiki_inspect_page_claims" => self.require(&run_id)?.inspect(args["jobId"].as_str().unwrap_or("")),
            "openwiki_submit_page" => self.require(&run_id)?.submit_page(args),
            "openwiki_finish" => {
                let v = self.require(&run_id)?.finish()?;
                self.active = None;
                Ok(v)
            }
            _ => Err(other(format!("unknown tool {name}"))),
        }
    }
}

/// What a host shows for a writing tool's error.
pub fn error_text(e: Error) -> String {
    match e {
        Error::Host(code, m) => format!("{code}: {m}"),
        Error::Run("conflict", m) => format!("conflict: {m}"),
        Error::Run("invalid_input" | "not_found", m) => format!("invalid_input: {m}"),
        Error::Run("invalid_state", m) => format!("invalid_state: {m}"),
        e => {
            eprintln!("OpenWiki MCP operation failed: {}", e.message());
            "OpenWiki MCP operation failed.".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_and_seeds() {
        assert_eq!(title_from_path("/openwiki/a/retry-logic_v2.md"), "Retry Logic V2");
        assert_eq!(evidence_seed("repo://src/a%20b.rs#L1-L3").unwrap(), "src/a%20b.rs");
        assert!(seed_path("../x").is_err());
        assert_eq!(plan_page_path("architecture/x.md").unwrap(), "/openwiki/architecture/x.md");
        assert!(plan_page_path("/openwiki/_draft.md").is_err());
        assert!(plan_page_path("/openwiki/index.md").is_err());
    }

    #[test]
    fn agents_blocks() {
        let dir = std::env::temp_dir().join(format!("harlo-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("AGENTS.md");
        std::fs::write(&a, "# Rules\n\n## OpenWiki\n\nThis repository has documentation located in the /openwiki directory.\nStart here:\n- [OpenWiki quickstart](openwiki/quickstart.md)\n\n## Other\n").unwrap();
        let next = next_agents_content(&a, "<!-- OPENWIKI:START -->\nX\n<!-- OPENWIKI:END -->", false).unwrap().unwrap();
        assert_eq!(next, "# Rules\n\n<!-- OPENWIKI:START -->\nX\n<!-- OPENWIKI:END -->\n\n## Other\n");
        std::fs::write(&a, &next).unwrap();
        let again = next_agents_content(&a, "<!-- OPENWIKI:START -->\nY\n<!-- OPENWIKI:END -->", false).unwrap().unwrap();
        assert_eq!(again, "# Rules\n\n<!-- OPENWIKI:START -->\nY\n<!-- OPENWIKI:END -->\n\n## Other\n");
        assert!(next_agents_content(&dir.join("CLAUDE.md"), "x", true).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
