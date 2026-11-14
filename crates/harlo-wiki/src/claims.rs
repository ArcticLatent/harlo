//! Claims: the wiki's facts, each tied to the code it comes from
//! (`repo://path#Lx-Ly` evidence with a version), as OpenWiki keeps them in
//! `openwiki/.claims/`. Evidence is followed as its lines move; a Claim
//! whose code changed is stale, one whose code is gone (or newly ignored)
//! unresolved, until a page's writer confirms, revises or retracts it.

use crate::ignore::Ignore;
use crate::store::{self, Error, Result, Store, hex};
use crate::util;
use indexmap::IndexMap;
use serde_json::{Map, Value, json};
use sha2::Digest;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const PRODUCER_ACTOR: &str = "openwiki/0.9.0";
const PREFIX: &str = "repo://";

#[derive(Clone, Debug, PartialEq)]
pub struct Evidence {
    pub resource: String,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Claim {
    pub id: String,
    pub statement: String,
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub page: String,
    pub kind: &'static str,
    pub claim_id: String,
    pub resources: Vec<String>,
}

impl Issue {
    pub fn to_json(&self) -> Value {
        json!({"page": self.page, "kind": self.kind, "claimId": self.claim_id, "resources": self.resources})
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Verification {
    pub by: String,
    pub at: String,
}

/// A page's sidecar.
#[derive(Clone, Debug)]
pub struct PageClaims {
    pub page_version: String,
    pub claims: Vec<Claim>,
    pub verification: Option<Verification>,
}

impl PageClaims {
    pub fn from_json(v: &Value) -> PageClaims {
        let s = |v: &Value| v.as_str().unwrap_or_default().to_owned();
        PageClaims {
            page_version: s(&v["pageVersion"]),
            claims: v["claims"].as_array().into_iter().flatten().map(claim_from_json).collect(),
            verification: v.get("verification").map(|x| Verification { by: s(&x["by"]), at: s(&x["at"]) }),
        }
    }

    pub fn to_json(&self) -> Value {
        let mut o = Map::new();
        o.insert("schemaVersion".into(), json!(1));
        o.insert("pageVersion".into(), json!(self.page_version));
        o.insert("claims".into(), Value::Array(self.claims.iter().map(claim_to_json).collect()));
        if let Some(v) = &self.verification {
            o.insert("verification".into(), json!({"by": v.by, "at": v.at}));
        }
        Value::Object(o)
    }
}

fn claim_from_json(c: &Value) -> Claim {
    let s = |v: &Value| v.as_str().unwrap_or_default().to_owned();
    Claim {
        id: s(&c["id"]),
        statement: s(&c["statement"]),
        evidence: c["evidence"].as_array().into_iter().flatten().map(|e| Evidence { resource: s(&e["resource"]), version: s(&e["version"]) }).collect(),
    }
}

fn claim_to_json(c: &Claim) -> Value {
    json!({"id": c.id, "statement": c.statement, "evidence": c.evidence.iter().map(|e| json!({"resource": e.resource, "version": e.version})).collect::<Vec<_>>()})
}

pub fn load(store: &Store, page: &str) -> Result<Option<PageClaims>> {
    Ok(store.load_page(page)?.map(|v| PageClaims::from_json(&v)))
}

// ---- Errors ----

/// OpenWiki's Claims error classes, by what callers tell apart.
fn session_err(m: impl Into<String>) -> Error {
    Error::Claims(m.into())
}

fn resource_err(m: impl Into<String>) -> Error {
    Error::Other(format!("resource:{}", m.into()))
}

fn evidence_security(m: impl Into<String>) -> Error {
    Error::ClaimsSecurity(m.into())
}

/// The message of an error, without its class mark.
pub fn message(e: &Error) -> String {
    let m = e.message();
    m.strip_prefix("resource:").or_else(|| m.strip_prefix("resolution:")).unwrap_or(m).to_owned()
}

fn is_resource(e: &Error) -> bool {
    matches!(e, Error::Other(m) if m.starts_with("resource:"))
}

/// A Claims error a run may carry on past (warned), not a security one or
/// something unexpected.
fn recoverable(e: &Error) -> bool {
    match e {
        Error::Claims(_) | Error::PageMissing(_) => true,
        Error::Other(m) => m.starts_with("resource:") || m.starts_with("resolution:"),
        _ => false,
    }
}

// ---- Evidence resources ----

#[derive(Clone, Debug, PartialEq)]
pub struct Resource {
    pub path: String,
    pub range: Option<(u64, u64)>,
}

/// `encodeURIComponent`.
pub fn encode_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `decodeURIComponent` (None: it would throw).
pub fn decode_component(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = b.get(i + 1..i + 3)?;
            let hs = std::str::from_utf8(h).ok()?;
            if !hs.bytes().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            out.push(u8::from_str_radix(hs, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn has_control(s: &str) -> bool {
    s.chars().any(|c| (c as u32) <= 0x1f || c as u32 == 0x7f)
}

pub fn parse_resource(resource: &str) -> Result<Resource> {
    let Some(body) = resource.strip_prefix(PREFIX) else {
        return Err(resource_err(format!("Unsupported evidence resource: {resource}")));
    };
    let (encoded_path, encoded_fragment) = match body.find('#') {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    if encoded_fragment.is_some_and(|f| f.contains('#')) {
        return Err(resource_err(format!("Evidence resource contains an unescaped fragment delimiter: {resource}")));
    }
    let bad_encoding = || resource_err(format!("Evidence resource contains invalid percent encoding: {resource}"));
    let decoded_path = decode_component(encoded_path).ok_or_else(bad_encoding)?;
    let decoded_fragment = match encoded_fragment {
        Some(f) => Some(decode_component(f).ok_or_else(bad_encoding)?),
        None => None,
    };
    if has_control(&decoded_path) {
        return Err(resource_err(format!("Evidence path contains a control character: {resource}")));
    }
    if decoded_fragment.as_deref().is_some_and(has_control) {
        return Err(resource_err(format!("Evidence line range contains a control character: {resource}")));
    }
    let n = store::posix_normalize(&decoded_path.replace('\\', "/"));
    let normalized = n.strip_prefix("./").map(str::to_owned).unwrap_or(n);
    let drive = normalized.len() >= 3 && normalized.as_bytes()[0].is_ascii_alphabetic() && &normalized[1..3] == ":/";
    if normalized.is_empty() || normalized == "." || normalized.starts_with("../") || normalized.starts_with('/') || drive {
        return Err(resource_err(format!("Evidence path must remain inside the repository: {resource}")));
    }
    let lower = normalized.to_lowercase();
    if lower == ".git" || lower.starts_with(".git/") || lower == "openwiki" || lower.starts_with("openwiki/") {
        return Err(resource_err(format!("Evidence cannot reference Git metadata or generated OpenWiki output: {resource}")));
    }
    let range = match decoded_fragment {
        None => None,
        Some(f) => Some(parse_line_range(&f, resource)?),
    };
    Ok(Resource { path: normalized, range })
}

fn parse_line_range(fragment: &str, resource: &str) -> Result<(u64, u64)> {
    static RANGE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"^L([1-9][0-9]*)(?:-L([1-9][0-9]*))?$").unwrap());
    let Some(m) = RANGE.captures(fragment) else {
        return Err(resource_err(format!("Evidence fragment must be a line range such as #L10-L24: {resource}")));
    };
    const SAFE: f64 = 9_007_199_254_740_991.0;
    let start: f64 = m[1].parse().unwrap_or(f64::INFINITY);
    let end: f64 = m.get(2).map_or(&m[1], |g| g.as_str()).parse().unwrap_or(f64::INFINITY);
    if start > SAFE || end > SAFE {
        return Err(resource_err(format!("Evidence line range exceeds the supported integer range: {resource}")));
    }
    if end < start {
        return Err(resource_err(format!("Evidence line range must end at or after its start: {resource}")));
    }
    Ok((start as u64, end as u64))
}

pub fn format_resource(r: &Resource) -> Result<String> {
    let encoded: Vec<String> = r.path.split('/').map(encode_component).collect();
    let fragment = r.range.map(|(s, e)| format!("L{s}-L{e}"));
    let formatted = format!("{PREFIX}{}{}", encoded.join("/"), fragment.map(|f| format!("#{f}")).unwrap_or_default());
    let parsed = parse_resource(&formatted)?;
    if parsed.path != r.path || parsed.range != r.range {
        return Err(resource_err(format!("Repository evidence is not normalized: {formatted}")));
    }
    Ok(formatted)
}

// ---- Resolving evidence against the repository ----

/// What a resource resolves to now.
#[derive(Clone)]
pub struct Resolved {
    pub evidence: Evidence,
}

const CONTEXT_LINES: usize = 3;
const LINES_PREFIX: &str = "repo-lines-v1:sha256:";

fn hash_text(s: &str) -> String {
    hex(&sha2::Sha256::digest(s.as_bytes()))
}

pub struct Resolver {
    root: PathBuf,
    real_root: std::cell::OnceCell<std::result::Result<PathBuf, String>>,
    ignore: Ignore,
}

impl Resolver {
    pub fn new(root: &Path, ignore: Ignore) -> Resolver {
        Resolver { root: root.to_path_buf(), real_root: Default::default(), ignore }
    }

    /// Resolves through a per-operation cache (OpenWiki caches each batch's,
    /// each finalization's and each preflight's lookups; errors aren't kept).
    pub fn cached(&self, cache: &mut HashMap<(String, Option<String>), Option<Resolved>>, resource: &str, previous: Option<&str>) -> Result<Option<Resolved>> {
        let key = (resource.to_owned(), previous.map(str::to_owned));
        if let Some(r) = cache.get(&key) {
            return Ok(r.clone());
        }
        let r = self.resolve(resource, previous)?;
        cache.insert(key, r.clone());
        Ok(r)
    }

    pub fn resolve(&self, resource: &str, previous: Option<&str>) -> Result<Option<Resolved>> {
        let parsed = parse_resource(resource)?;
        let canonical = format_resource(&parsed)?;
        if self.ignore.ignores(&parsed.path, false) {
            return Err(resource_err(format!("Evidence path is excluded by .openwikiignore: {}", parsed.path)));
        }
        // (A lone ".." survives normalizing; it escapes the repository.)
        if parsed.path == ".." {
            return Err(resource_err(format!("Evidence path escapes the repository: {}", parsed.path)));
        }
        let absolute = self.root.join(&parsed.path);
        let unreadable = |e: &std::io::Error, call: &str, p: &Path| Error::Other(format!("resolution:Unable to read evidence {}: {}", parsed.path, util::node_io_error(e, call, p)));
        let meta = match std::fs::symlink_metadata(&absolute) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(unreadable(&e, "lstat", &absolute)),
        };
        if meta.file_type().is_symlink() {
            return Err(evidence_security(format!("Evidence cannot reference a symbolic link: {}", parsed.path)));
        }
        if !meta.is_file() {
            return Ok(None);
        }
        let real_root = self
            .real_root
            .get_or_init(|| std::fs::canonicalize(&self.root).map_err(|e| e.to_string()))
            .clone()
            .map_err(|e| evidence_security(format!("Unable to resolve repository root {}: {e}", self.root.display())))?;
        let physical = match std::fs::canonicalize(&absolute) {
            Ok(p) => p,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(unreadable(&e, "realpath", &absolute)),
        };
        if !physical.starts_with(&real_root) || physical == real_root || physical != real_root.join(&parsed.path) {
            return Err(evidence_security(format!("Evidence path traverses a symbolic link or filesystem alias: {}", parsed.path)));
        }
        let source = match std::fs::read(&physical) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(unreadable(&e, "open", &physical)),
        };
        match parsed.range {
            None => Ok(Some(Resolved { evidence: Evidence { resource: canonical, version: format!("repo-file-v1:sha256:{}", hash_text(&source)) } })),
            Some((start, end)) => line_range(&canonical, &source, start, end, previous),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Span {
    start: usize,
    end: usize,
}

struct Meta {
    selected: usize,
    first: String,
    last: String,
    before_count: usize,
    before: String,
    after_count: usize,
    after: String,
}

fn split_lines(source: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < source.len() {
        let end = source[start..].find('\n').map_or(source.len(), |i| start + i + 1);
        out.push(&source[start..end]);
        start = end;
    }
    out
}

fn content(lines: &[&str], s: Span) -> String {
    lines[s.start..s.end].concat()
}

fn line_range(resource: &str, source: &str, start: u64, end: u64, previous: Option<&str>) -> Result<Option<Resolved>> {
    let lines = split_lines(source);
    let hinted = (end as usize <= lines.len() && end <= usize::MAX as u64).then(|| Span { start: start as usize - 1, end: end as usize });
    let parsed = previous.and_then(parse_version);
    let (Some(prev), Some(previous)) = (parsed, previous) else {
        return hinted.map(|s| make_range(resource, &lines, s)).transpose();
    };
    let (hash, meta) = prev;
    if let Some(s) = unchanged(&lines, hinted, &hash, &meta) {
        return Ok(Some(Resolved { evidence: Evidence { resource: range_resource(resource, s)?, version: previous.to_owned() } }));
    }
    changed(&lines, &meta).map(|s| make_range(resource, &lines, s)).transpose()
}

fn range_resource(resource: &str, s: Span) -> Result<String> {
    format_resource(&Resource { path: parse_resource(resource)?.path, range: Some((s.start as u64 + 1, s.end as u64)) })
}

fn make_range(resource: &str, lines: &[&str], s: Span) -> Result<Resolved> {
    let text = content(lines, s);
    let before_start = s.start.saturating_sub(CONTEXT_LINES);
    let after_end = (s.end + CONTEXT_LINES).min(lines.len());
    let meta = json!({
        "selectedLineCount": s.end - s.start,
        "firstSelectedLineHash": hash_text(lines[s.start]),
        "lastSelectedLineHash": hash_text(lines[s.end - 1]),
        "precedingContextLineCount": s.start - before_start,
        "precedingContextHash": hash_text(&lines[before_start..s.start].concat()),
        "followingContextLineCount": after_end - s.end,
        "followingContextHash": hash_text(&lines[s.end..after_end].concat()),
    });
    let encoded = base64url(serde_json::to_string(&meta).unwrap_or_default().as_bytes());
    Ok(Resolved { evidence: Evidence { resource: range_resource(resource, s)?, version: format!("{LINES_PREFIX}{}:{encoded}", hash_text(&text)) } })
}

fn unchanged(lines: &[&str], hinted: Option<Span>, hash: &str, m: &Meta) -> Option<Span> {
    if let Some(h) = hinted {
        if h.end - h.start == m.selected && hash_text(&content(lines, h)) == hash {
            return Some(h);
        }
    }
    let line_hashes: Vec<String> = lines.iter().map(|l| hash_text(l)).collect();
    let mut matching = Vec::new();
    let mut start = 0;
    while start + m.selected <= lines.len() {
        let end = start + m.selected;
        if line_hashes[start] == m.first && line_hashes[end - 1] == m.last {
            let s = Span { start, end };
            if hash_text(&content(lines, s)) == hash {
                matching.push(s);
            }
        }
        start += 1;
    }
    if matching.len() == 1 {
        return Some(matching[0]);
    }
    let ctx: Vec<Span> = matching.into_iter().filter(|s| context_matches(lines, *s, m)).collect();
    (ctx.len() == 1).then(|| ctx[0])
}

fn context_matches(lines: &[&str], s: Span, m: &Meta) -> bool {
    let before = if m.before_count == 0 { s.start == 0 } else { s.start >= m.before_count && hash_text(&lines[s.start - m.before_count..s.start].concat()) == m.before };
    let after = if m.after_count == 0 { s.end == lines.len() } else { s.end + m.after_count <= lines.len() && hash_text(&lines[s.end..s.end + m.after_count].concat()) == m.after };
    before && after
}

fn changed(lines: &[&str], m: &Meta) -> Option<Span> {
    let starts = boundaries(lines, m.before_count, &m.before, true);
    let ends = boundaries(lines, m.after_count, &m.after, false);
    let mut found: Option<Span> = None;
    for &start in &starts {
        for &end in &ends {
            if end > start {
                if found.is_some() {
                    return None;
                }
                found = Some(Span { start, end });
            }
        }
    }
    found
}

fn boundaries(lines: &[&str], count: usize, hash: &str, before: bool) -> Vec<usize> {
    if count == 0 {
        return vec![if before { 0 } else { lines.len() }];
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i + count <= lines.len() {
        if hash_text(&lines[i..i + count].concat()) == hash {
            out.push(if before { i + count } else { i });
        }
        i += 1;
    }
    out
}

fn is_sha(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn parse_version(v: &str) -> Option<(String, Meta)> {
    let body = v.strip_prefix(LINES_PREFIX)?;
    let sep = body.find(':')?;
    let hash = &body[..sep];
    if !is_sha(hash) {
        return None;
    }
    let decoded = base64_decode(&body[sep + 1..]);
    let meta: Value = serde_json::from_str(&String::from_utf8_lossy(&decoded)).ok()?;
    let o = meta.as_object()?;
    if o.len() != 7 {
        return None;
    }
    let int = |k: &str, min: f64, max: f64| -> Option<usize> {
        let n = o.get(k)?.as_f64()?;
        (n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0 && n >= min && n <= max).then_some(n as usize)
    };
    let sha = |k: &str| -> Option<String> { o.get(k)?.as_str().filter(|s| is_sha(s)).map(str::to_owned) };
    Some((
        hash.to_owned(),
        Meta {
            selected: int("selectedLineCount", 1.0, 9_007_199_254_740_991.0)?,
            before_count: int("precedingContextLineCount", 0.0, CONTEXT_LINES as f64)?,
            after_count: int("followingContextLineCount", 0.0, CONTEXT_LINES as f64)?,
            first: sha("firstSelectedLineHash")?,
            last: sha("lastSelectedLineHash")?,
            before: sha("precedingContextHash")?,
            after: sha("followingContextHash")?,
        },
    ))
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64url without padding, as Node writes it.
pub fn base64url(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |a, (i, b)| a | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..chunk.len() + 1 {
            out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

/// Node's lenient base64/base64url decoding: both alphabets, other
/// characters skipped, stopping at padding.
fn base64_decode(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            _ => continue,
        };
        acc = acc << 6 | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

// ---- Mutations ----

#[derive(Clone, Debug)]
pub enum Op {
    Add { statement: String, evidence: Vec<String> },
    Confirm { id: String },
    Update { id: String, statement: Option<String>, evidence: Option<Vec<String>> },
    Retract { id: String },
}

impl Op {
    fn name(&self) -> &'static str {
        match self {
            Op::Add { .. } => "add",
            Op::Confirm { .. } => "confirm",
            Op::Update { .. } => "update",
            Op::Retract { .. } => "retract",
        }
    }

    fn id(&self) -> Option<&str> {
        match self {
            Op::Add { .. } => None,
            Op::Confirm { id } | Op::Update { id, .. } | Op::Retract { id } => Some(id),
        }
    }
}

fn require_canonical(value: &str, label: &str) -> Result<()> {
    let t = crate::jsre::trim(value);
    if t.is_empty() {
        return Err(session_err(format!("{label} cannot be empty.")));
    }
    if t != value {
        return Err(session_err(format!("{label} cannot contain surrounding whitespace.")));
    }
    Ok(())
}

fn validate_claims(claims: &[Claim]) -> Result<()> {
    let mut ids = HashSet::new();
    for c in claims {
        require_canonical(&c.id, "Claim id")?;
        if !ids.insert(c.id.as_str()) {
            return Err(session_err(format!("Duplicate claim id: {}", c.id)));
        }
        require_canonical(&c.statement, &format!("Claim {} statement", c.id))?;
        if c.evidence.is_empty() {
            return Err(session_err(format!("Claim {} requires at least one evidence resource.", c.id)));
        }
        let mut seen = HashSet::new();
        for e in &c.evidence {
            require_canonical(&e.resource, &format!("Claim {} evidence resource", c.id))?;
            require_canonical(&e.version, &format!("Claim {} evidence version", c.id))?;
            if !seen.insert(e.resource.as_str()) {
                return Err(session_err(format!("Claim {} repeats evidence {}", c.id, e.resource)));
            }
        }
    }
    Ok(())
}

fn validate_op(op: &Op) -> Result<()> {
    let (statement, evidence) = match op {
        Op::Retract { id } => return require_canonical(id, "Retract claim id"),
        Op::Confirm { id } => return require_canonical(id, "Confirm claim id"),
        Op::Update { id, statement, evidence } => {
            require_canonical(id, "Update claim id")?;
            if statement.is_none() && evidence.is_none() {
                return Err(session_err("An update requires a statement or evidence change."));
            }
            (statement.as_deref(), evidence.as_ref())
        }
        Op::Add { statement, evidence } => (Some(statement.as_str()), Some(evidence)),
    };
    if statement.is_some_and(|s| crate::jsre::trim(s).is_empty()) {
        return Err(session_err("Claim statement cannot be empty."));
    }
    if evidence.is_some_and(Vec::is_empty) {
        return Err(session_err("A claim requires at least one evidence resource."));
    }
    for r in evidence.into_iter().flatten() {
        require_canonical(r, "Proposed evidence resource")?;
    }
    Ok(())
}

type Cache = HashMap<(String, Option<String>), Option<Resolved>>;

fn resolve_all(inputs: &[(String, Option<String>)], resolver: &Resolver, cache: &mut Cache) -> Result<Vec<Evidence>> {
    let mut out = Vec::new();
    let mut seen_in = HashSet::new();
    let mut seen_out = HashSet::new();
    for (resource, previous) in inputs {
        if !seen_in.insert(resource.clone()) {
            return Err(session_err(format!("Claim evidence repeats {resource}")));
        }
        let Some(r) = resolver.cached(cache, resource, previous.as_deref())? else {
            return Err(session_err(format!("Evidence does not resolve: {resource}")));
        };
        require_canonical(&r.evidence.resource, "Resolved evidence resource")?;
        require_canonical(&r.evidence.version, "Resolved evidence version")?;
        if !seen_out.insert(r.evidence.resource.clone()) {
            return Err(session_err(format!("Claim evidence resolves to duplicate resource {}", r.evidence.resource)));
        }
        out.push(r.evidence);
    }
    Ok(out)
}

/// `applyClaimOperations`: the page's Claims after a batch.
fn apply(claims: &[Claim], ops: &[Op], resolver: &Resolver, mut new_id: impl FnMut() -> Result<String>) -> Result<Vec<Claim>> {
    if ops.is_empty() {
        return Err(session_err("A claim mutation requires at least one operation."));
    }
    validate_claims(claims)?;
    let mut working = claims.to_vec();
    let mut reserved: HashSet<String> = claims.iter().map(|c| c.id.clone()).collect();
    let mut targeted = HashSet::new();
    let mut cache = Cache::new();
    for op in ops {
        validate_op(op)?;
        if let Some(id) = op.id() {
            if !targeted.insert(id.to_owned()) {
                return Err(session_err(format!("Claim {id} is targeted more than once in one batch.")));
            }
            if !reserved.contains(id) {
                return Err(session_err(format!("Unknown claim id: {id}")));
            }
        }
    }
    let mut resolved: HashMap<usize, Vec<Evidence>> = HashMap::new();
    for (i, op) in ops.iter().enumerate() {
        let inputs: Vec<(String, Option<String>)> = match op {
            Op::Add { evidence, .. } => evidence.iter().map(|r| (r.clone(), None)).collect(),
            Op::Confirm { id } | Op::Update { id, .. } => {
                let Some(current) = working.iter().find(|c| &c.id == id) else {
                    return Err(session_err(format!("Unknown claim id: {id}")));
                };
                match op {
                    Op::Update { evidence: Some(e), .. } => e.iter().map(|r| (r.clone(), None)).collect(),
                    _ => current.evidence.iter().map(|e| (e.resource.clone(), Some(e.version.clone()))).collect(),
                }
            }
            Op::Retract { .. } => continue,
        };
        resolved.insert(i, resolve_all(&inputs, resolver, &mut cache)?);
    }
    for (i, op) in ops.iter().enumerate() {
        let evidence = || resolved.get(&i).cloned().ok_or_else(|| session_err("Resolved claim evidence is missing."));
        if let Op::Add { statement, .. } = op {
            let mut id = None;
            for _ in 0..10 {
                let candidate = new_id()?;
                require_canonical(&candidate, "Generated claim id")?;
                if reserved.insert(candidate.clone()) {
                    id = Some(candidate);
                    break;
                }
            }
            let Some(id) = id else { return Err(session_err("Unable to allocate a unique claim identifier.")) };
            working.push(Claim { id, statement: crate::jsre::trim(statement).to_owned(), evidence: evidence()? });
            continue;
        }
        let id = op.id().unwrap_or_default();
        let Some(k) = working.iter().position(|c| c.id == id) else {
            return Err(session_err(format!("Unknown claim id: {id}")));
        };
        if let Op::Retract { .. } = op {
            working.remove(k);
            continue;
        }
        let statement = match op {
            Op::Update { statement: Some(s), .. } => crate::jsre::trim(s).to_owned(),
            _ => working[k].statement.clone(),
        };
        working[k] = Claim { id: id.to_owned(), statement, evidence: evidence()? };
    }
    Ok(working)
}

// ---- The session: every page's Claims during a run ----

struct PageState {
    claims: Vec<Claim>,
    verification: Option<Verification>,
    persisted: bool,
    dirty: bool,
    deleted: bool,
    issues: Vec<Issue>,
}

impl PageState {
    fn new() -> PageState {
        PageState { claims: Vec::new(), verification: None, persisted: false, dirty: false, deleted: false, issues: Vec::new() }
    }
}

pub struct Session {
    pub resolver: Resolver,
    pages: IndexMap<String, PageState>,
    owners: HashMap<String, String>,
    orphans: Vec<String>,
}

/// A Claim as a page's writer sees it: its evidence without versions, and
/// its issue if it has one.
pub fn inspected(c: &Claim, issues: &[Issue]) -> Value {
    let mut v = json!({"id": c.id, "statement": c.statement, "evidence": c.evidence.iter().map(|e| e.resource.clone()).collect::<Vec<_>>()});
    if let Some(i) = issues.iter().find(|i| i.claim_id == c.id) {
        v["issue"] = json!({"kind": i.kind, "resources": i.resources});
    }
    v
}

fn locale_sort(v: &mut [String]) {
    v.sort_by(|a, b| crate::collate::compare(a, b));
}

impl Session {
    pub fn new(resolver: Resolver, persisted: Vec<(String, PageClaims)>, issues: &[Issue], orphans: &[String]) -> Result<Session> {
        let mut orphan_pages: Vec<String> = Vec::new();
        for o in orphans {
            let p = store::normalize_page(o).map_err(|e| session_err(e.message()))?;
            if !orphan_pages.contains(&p) {
                orphan_pages.push(p);
            }
        }
        locale_sort(&mut orphan_pages);
        let mut s = Session { resolver, pages: IndexMap::new(), owners: HashMap::new(), orphans: orphan_pages };
        for (input, p) in persisted {
            let page = store::normalize_page(&input).map_err(|e| session_err(e.message()))?;
            if s.pages.contains_key(&page) {
                return Err(session_err(format!("Duplicate persisted claim page: {page}")));
            }
            s.assert_available(&page, &p.claims)?;
            for c in &p.claims {
                s.owners.insert(c.id.clone(), page.clone());
            }
            let page_issues = issues.iter().filter(|i| store::normalize_page(&i.page).ok().as_deref() == Some(page.as_str())).cloned().collect();
            s.pages.insert(page, PageState { claims: p.claims, verification: p.verification, persisted: true, dirty: false, deleted: false, issues: page_issues });
        }
        Ok(s)
    }

    fn state(&mut self, page: &str) -> &mut PageState {
        self.pages.entry(page.to_owned()).or_insert_with(PageState::new)
    }

    fn assert_available(&self, page: &str, claims: &[Claim]) -> Result<()> {
        let mut ids = HashSet::new();
        for c in claims {
            if !ids.insert(c.id.as_str()) {
                return Err(session_err(format!("Duplicate claim id {} within {page}", c.id)));
            }
            if let Some(owner) = self.owners.get(&c.id).filter(|o| o.as_str() != page) {
                return Err(session_err(format!("Duplicate claim id {} across {owner} and {page}", c.id)));
            }
        }
        Ok(())
    }

    fn replace_ownership(&mut self, page: &str, previous: &[Claim], next: &[Claim]) {
        let next_ids: HashSet<&str> = next.iter().map(|c| c.id.as_str()).collect();
        for c in previous {
            if !next_ids.contains(c.id.as_str()) && self.owners.get(&c.id).is_some_and(|o| o == page) {
                self.owners.remove(&c.id);
            }
        }
        for c in next {
            self.owners.insert(c.id.clone(), page.to_owned());
        }
    }

    /// Applies a batch to a page; the ids of what it added and targeted.
    pub fn resolve_claims(&mut self, page: &str, ops: &[Op]) -> Result<Vec<(String, String)>> {
        let page = store::normalize_page(page).map_err(|e| session_err(e.message()))?;
        let previous = self.state(&page).claims.clone();
        let existing: HashSet<String> = previous.iter().map(|c| c.id.clone()).collect();
        let owners = &self.owners;
        let next = apply(&previous, ops, &self.resolver, || {
            for _ in 0..32 {
                let id = format!("claim_{}", util::new_id().replace('-', ""));
                if !owners.contains_key(&id) {
                    return Ok(id);
                }
            }
            Err(session_err("Unable to allocate a globally unique claim identifier."))
        })?;
        self.assert_available(&page, &next)?;
        self.replace_ownership(&page, &previous, &next);
        let allocated: Vec<String> = next.iter().map(|c| c.id.clone()).filter(|id| !existing.contains(id)).collect();
        let mut k = 0;
        let results: Vec<(String, String)> = ops
            .iter()
            .map(|op| {
                let id = match op.id() {
                    Some(id) => id.to_owned(),
                    None => {
                        k += 1;
                        allocated.get(k - 1).cloned().unwrap_or_default()
                    }
                };
                (op.name().to_owned(), id)
            })
            .collect();
        let state = self.state(&page);
        state.claims = next;
        state.dirty = true;
        state.deleted = false;
        let targeted: HashSet<&str> = results.iter().map(|(_, id)| id.as_str()).collect();
        state.issues.retain(|i| !targeted.contains(i.claim_id.as_str()));
        Ok(results)
    }

    pub fn inspect(&mut self, page: &str) -> Result<Vec<Value>> {
        let page = store::normalize_page(page).map_err(|e| session_err(e.message()))?;
        let state = self.state(&page);
        if state.deleted {
            return Ok(Vec::new());
        }
        Ok(state.claims.iter().map(|c| inspected(c, &state.issues)).collect())
    }

    pub fn claims_of(&mut self, page: &str) -> Result<Vec<Claim>> {
        let page = store::normalize_page(page).map_err(|e| session_err(e.message()))?;
        let state = self.state(&page);
        Ok(if state.deleted { Vec::new() } else { state.claims.clone() })
    }

    pub fn has_claim(&self, id: &str) -> bool {
        self.owners.contains_key(id)
    }

    /// Each page's cited resources (deleted pages left out), by page.
    pub fn evidence_by_page(&self) -> Vec<(String, Vec<String>)> {
        let mut pages: Vec<(&String, &PageState)> = self.pages.iter().collect();
        pages.sort_by(|a, b| crate::collate::compare(a.0, b.0));
        pages
            .into_iter()
            .filter(|(_, s)| !s.deleted)
            .map(|(p, s)| {
                let mut r: Vec<String> = Vec::new();
                for c in &s.claims {
                    for e in &c.evidence {
                        if !r.contains(&e.resource) {
                            r.push(e.resource.clone());
                        }
                    }
                }
                locale_sort(&mut r);
                (p.clone(), r)
            })
            .collect()
    }

    pub fn record_deletion(&mut self, page: &str) -> Result<()> {
        let page = store::normalize_page(page).map_err(|e| session_err(e.message()))?;
        let previous = self.state(&page).claims.clone();
        self.replace_ownership(&page, &previous, &[]);
        let s = self.state(&page);
        s.deleted = true;
        s.dirty = false;
        s.issues.clear();
        Ok(())
    }

    /// Writes every changed page's sidecar (verified now), removes deleted
    /// and orphaned ones; each page's verification for the front matter.
    fn finalize_sidecars(&mut self, store: &Store, verification: &Verification, excluded: &HashSet<String>) -> Result<(Vec<(String, Option<Verification>)>, Vec<String>)> {
        let mut cache = Cache::new();
        let mut warnings = Vec::new();
        let mut missing = Vec::new();
        let mut ready: Vec<(String, String)> = Vec::new();
        let pages: Vec<String> = self.pages.keys().cloned().collect();
        for page in &pages {
            if excluded.contains(page) {
                continue;
            }
            let state = &self.pages[page];
            if state.deleted || !state.dirty {
                continue;
            }
            let attempt = (|| -> Result<String> {
                if !state.issues.is_empty() {
                    return Err(session_err(format!("Unresolved evidence debt remains for {page}")));
                }
                for c in &state.claims {
                    for e in &c.evidence {
                        match self.resolver.cached(&mut cache, &e.resource, Some(&e.version))? {
                            None => return Err(session_err(format!("Evidence disappeared before finalizing {page}: {}", e.resource))),
                            Some(r) if r.evidence.version != e.version => return Err(session_err(format!("Evidence changed before finalizing {page}: {}", e.resource))),
                            _ => {}
                        }
                    }
                }
                store.hash_page(page)
            })();
            match attempt {
                Ok(hash) => ready.push((page.clone(), hash)),
                Err(e) if !recoverable(&e) => return Err(e),
                Err(Error::PageMissing(m)) => {
                    missing.push(page.clone());
                    warnings.push(format!("Could not verify Claims for {page} because its Markdown disappeared; its sidecar will be removed: {m}"));
                }
                Err(e) => warnings.push(warning(page, "verify", &e)),
            }
        }
        let remove = |page: &String, action: &str, warnings: &mut Vec<String>| -> Result<()> {
            if excluded.contains(page) {
                return Ok(());
            }
            match store.delete_page(page) {
                Ok(()) => Ok(()),
                Err(e) if !recoverable(&e) => Err(e),
                Err(e) => {
                    warnings.push(warning(page, action, &e));
                    Ok(())
                }
            }
        };
        for orphan in self.orphans.clone() {
            remove(&orphan, "clean up", &mut warnings)?;
        }
        for page in &missing {
            remove(page, "remove", &mut warnings)?;
        }
        for page in &pages {
            if self.pages[page].deleted {
                remove(page, "remove", &mut warnings)?;
            }
        }
        for (page, hash) in ready {
            let state = &self.pages[&page];
            let next = (!state.claims.is_empty()).then(|| verification.clone());
            let doc = PageClaims { page_version: hash, claims: state.claims.clone(), verification: next.clone() };
            match store.write_page(&page, &doc.to_json()) {
                Ok(()) => {
                    let s = self.pages.get_mut(&page).unwrap();
                    s.verification = next;
                    s.persisted = true;
                    s.dirty = false;
                }
                Err(e) if !recoverable(&e) => return Err(e),
                Err(e) => warnings.push(warning(&page, "persist", &e)),
            }
        }
        let mut by_page = Vec::new();
        for (page, s) in &self.pages {
            if excluded.contains(page) || s.deleted {
                continue;
            }
            let eligible = s.persisted && !s.dirty && !s.claims.is_empty() && s.issues.is_empty() && s.verification.is_some();
            by_page.push((page.clone(), if eligible { s.verification.clone() } else { None }));
        }
        Ok((by_page, warnings))
    }

    fn refresh_versions(&self, store: &Store, pages: &[String]) -> Result<(Vec<String>, Vec<String>)> {
        let mut warnings = Vec::new();
        let mut failed = Vec::new();
        let mut unique: Vec<String> = Vec::new();
        for p in pages {
            if !unique.contains(p) {
                unique.push(p.clone());
            }
        }
        locale_sort(&mut unique);
        for input in unique {
            let page = store::normalize_page(&input).map_err(|e| session_err(e.message()))?;
            let Some(s) = self.pages.get(&page) else { continue };
            if !s.persisted || s.deleted || s.dirty {
                continue;
            }
            let r = store.hash_page(&page).and_then(|hash| store.write_page(&page, &PageClaims { page_version: hash, claims: s.claims.clone(), verification: s.verification.clone() }.to_json()));
            match r {
                Ok(()) => {}
                Err(e) if !recoverable(&e) => return Err(e),
                Err(e) => {
                    failed.push(page.clone());
                    warnings.push(warning(&page, "synchronize", &e));
                }
            }
        }
        Ok((failed, warnings))
    }

    /// The runtime's `finalize`: sidecars, then each page's verification in
    /// its front matter, then every sidecar's page version refreshed.
    pub fn finalize(&mut self, store: &Store, at: &str, excluded: &HashSet<String>) -> Result<()> {
        let verification = Verification { by: PRODUCER_ACTOR.into(), at: at.into() };
        let (by_page, mut warnings) = self.finalize_sidecars(store, &verification, excluded)?;
        let originals = crate::okf::synchronize_verification(store, &by_page, excluded)?;
        let pages: Vec<String> = by_page.iter().map(|(p, _)| p.clone()).collect();
        let (failed, refresh_warnings) = self.refresh_versions(store, &pages)?;
        let unsafe_stamps: Vec<String> = failed.into_iter().filter(|p| by_page.iter().any(|(q, v)| q == p && v.is_some())).collect();
        if !unsafe_stamps.is_empty() {
            for page in &unsafe_stamps {
                if let Some((_, original)) = originals.iter().find(|(p, _)| p == page) {
                    store.write_markdown(page, original)?;
                }
            }
        }
        warnings.extend(refresh_warnings);
        if !warnings.is_empty() {
            return Err(Error::Claims(format!("Claims finalization was not fully durable: {}", warnings.join("; "))));
        }
        Ok(())
    }
}

fn warning(page: &str, action: &str, e: &Error) -> String {
    format!("Could not {action} Claims for {page}; its Claims sidecar was left unchanged: {}", message(e))
}

// ---- Preflight and the runtime ----

pub struct Runtime {
    pub session: Session,
    pub issues: Vec<Issue>,
    pub store: Store,
}

/// `prepareClaimsRuntime` for a repository run: a fresh init starts with
/// no Claims; otherwise every page's Claims are checked against the code.
pub fn prepare(root: &Path, ignore: &Ignore, fresh_init: bool) -> Result<Runtime> {
    let store = Store::new(root);
    let resolver = Resolver::new(root, ignore.clone());
    if fresh_init {
        let orphans = store.discover_sidecar_pages()?;
        let session = Session::new(resolver, Vec::new(), &[], &orphans)?;
        return Ok(Runtime { session, issues: Vec::new(), store });
    }
    let pages = store.discover_pages()?;
    let mut persisted: Vec<(String, PageClaims)> = Vec::new();
    for page in &pages {
        if let Some(p) = load(&store, page)? {
            persisted.push((store::normalize_page(page)?, p));
        }
    }
    let page_set: HashSet<&String> = pages.iter().collect();
    let mut orphans: Vec<String> = store.discover_sidecar_pages()?.into_iter().filter(|p| !page_set.contains(p)).collect();
    let mut issues = Vec::new();
    let mut cache = Cache::new();
    for page in &pages {
        let Some((_, p)) = persisted.iter().find(|(q, _)| q == page) else { continue };
        for c in &p.claims {
            let mut changed = Vec::new();
            let mut unresolved = Vec::new();
            for e in &c.evidence {
                let current = match resolver.cached(&mut cache, &e.resource, Some(&e.version)) {
                    Ok(r) => r,
                    // (harlo: evidence a newer .openwikiignore excludes is
                    // reconciled like a refused resource, not a run abort.)
                    Err(err) if matches!(err, Error::ClaimsSecurity(_)) || (is_resource(&err) && message(&err).starts_with("Evidence path is excluded by .openwikiignore")) => None,
                    Err(err) => return Err(err),
                };
                match current {
                    None => unresolved.push(e.resource.clone()),
                    Some(r) if r.evidence.version != e.version => changed.push(e.resource.clone()),
                    _ => {}
                }
            }
            if !unresolved.is_empty() {
                unresolved.sort_by(|a, b| util::cmp16(a, b));
                issues.push(Issue { page: page.clone(), kind: "unresolved", claim_id: c.id.clone(), resources: unresolved });
            } else if !changed.is_empty() {
                changed.sort_by(|a, b| util::cmp16(a, b));
                issues.push(Issue { page: page.clone(), kind: "stale", claim_id: c.id.clone(), resources: changed });
            }
        }
    }
    issues.sort_by(|a, b| crate::collate::compare(&a.page, &b.page).then_with(|| crate::collate::compare(a.kind, b.kind)).then_with(|| crate::collate::compare(&a.claim_id, &b.claim_id)));
    locale_sort(&mut orphans);
    let session = Session::new(resolver, persisted, &issues, &orphans)?;
    Ok(Runtime { session, issues, store })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resources() {
        let r = parse_resource("repo://src/a%20b.rs#L2-L4").unwrap();
        assert_eq!(r, Resource { path: "src/a b.rs".into(), range: Some((2, 4)) });
        assert_eq!(format_resource(&r).unwrap(), "repo://src/a%20b.rs#L2-L4");
        assert_eq!(format_resource(&parse_resource("repo://./x//y.rs#L3").unwrap()).unwrap(), "repo://x/y.rs#L3-L3");
        for bad in ["src/a.rs", "repo://../x", "repo://.git/config", "repo://openwiki/a.md", "repo://a#L0", "repo://a#L3-L2", "repo://a%zz", "repo://a#x#y", "repo:///etc/passwd"] {
            assert!(parse_resource(bad).is_err(), "{bad}");
        }
        assert_eq!(base64url(b"ab"), "YWI");
        assert_eq!(base64_decode("YWI"), b"ab");
        assert_eq!(base64_decode("YW I="), b"ab");
        assert_eq!(encode_component("a b/é"), "a%20b%2F%C3%A9");
    }

    #[test]
    fn ranges_follow_moved_lines() {
        let dir = std::env::temp_dir().join(format!("harlo-claims-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.rs"), "one\ntwo\nthree\nfour\nfive\n").unwrap();
        let r = Resolver::new(&dir, Ignore::parse(""));
        let first = r.resolve("repo://a.rs#L2-L3", None).unwrap().unwrap().evidence;
        assert_eq!(first.resource, "repo://a.rs#L2-L3");
        std::fs::write(dir.join("a.rs"), "zero\none\ntwo\nthree\nfour\nfive\n").unwrap();
        let moved = r.resolve(&first.resource, Some(&first.version)).unwrap().unwrap().evidence;
        assert_eq!(moved.resource, "repo://a.rs#L3-L4");
        assert_eq!(moved.version, first.version);
        std::fs::write(dir.join("a.rs"), "zero\none\nTWO\nthree\nfour\nfive\n").unwrap();
        let changed = r.resolve(&first.resource, Some(&first.version)).unwrap().unwrap().evidence;
        assert_eq!(changed.resource, "repo://a.rs#L3-L4");
        assert_ne!(changed.version, first.version);
        assert!(r.resolve("repo://gone.rs", None).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
