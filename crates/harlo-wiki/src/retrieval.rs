//! Search and exact section reads (`openwiki_search`, `openwiki_read`), as
//! OpenWiki answers them: sections indexed in an in-memory SQLite FTS5
//! table, ranked by source-path hints, then query-term coverage, then bm25.
//! (OpenWiki indexes the whole wiki again for every search; here the index
//! is kept, for the last few wikis searched, and built again only when a
//! page's text changed: the same answers, without re-reading every page.)

use crate::front;
use crate::jsre::{self, Re};
use crate::marked::{self, Block, Kind};
use crate::store::{Error, Result, Store, is_grounded};
use crate::workspaces::{self, Scope};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};

pub const QUERY_CHARACTERS: usize = 2_000;
pub const SOURCE_PATH_HINTS: usize = 20;
pub const SOURCE_PATH_CHARACTERS: usize = 500;
pub const SEARCH_RESULTS: usize = 20;
pub const DEFAULT_SEARCH_RESULTS: usize = 5;
pub const PAGE_CHARACTERS: usize = 1_000;
pub const SECTION_ANCHORS: usize = 20;
pub const SECTION_ANCHOR_CHARACTERS: usize = 500;
pub const EXCERPT_CHARACTERS: usize = 600;
pub const QUERY_TERMS: usize = 64;
pub const WIKI_ID_CHARACTERS: usize = 64;
pub const WORKSPACE_REFERENCE_CHARACTERS: usize = 80;

const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "can", "do", "does", "for", "from", "how", "i", "in", "is", "it", "of", "on", "or",
    "our", "that", "the", "their", "this", "to", "was", "we", "what", "when", "where", "which", "why", "with", "your",
];
const INVALID_PAGE: &str = "Page must be a non-structural Markdown path below openwiki/.";
const SYMBOL_ONLY_ANCHOR: &str = "section";

fn retrieval(m: impl Into<String>) -> Error {
    Error::Retrieval(m.into())
}

// ---- Text normalization ----

static CAMEL1: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"([\p{Ll}0-9])(\p{Lu})").unwrap());
static CAMEL2: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(\p{Lu})(\p{Lu}\p{Ll})").unwrap());
static SEPARATORS: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"[_./:#-]+").unwrap());
static WORDS: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"[\p{L}\p{N}_]+").unwrap());

/// Text plus its camel-case and path pieces split apart (`normalizeSearchText`).
pub fn normalize_text(text: &str) -> String {
    let a = CAMEL1.replace_all(text, "$1 $2");
    let b = CAMEL2.replace_all(&a, "$1 $2");
    let c = SEPARATORS.replace_all(&b, " ");
    format!("{text} {c}")
}

/// A query's distinct words, low-signal ones left out unless that's all.
pub fn query_terms(query: &str) -> Vec<String> {
    let lower = js_lower(&normalize_text(query));
    let mut all: Vec<String> = Vec::new();
    for m in WORDS.find_iter(&lower) {
        if !all.iter().any(|t| t == m.as_str()) {
            all.push(m.as_str().to_owned());
        }
    }
    all.truncate(QUERY_TERMS);
    let meaningful: Vec<String> = all.iter().filter(|t| !STOP_WORDS.contains(&t.as_str())).cloned().collect();
    if meaningful.is_empty() { all } else { meaningful }
}

/// JavaScript's `toLowerCase`.
pub fn js_lower(s: &str) -> String {
    s.to_lowercase()
}

// ---- Sections ----

pub struct Section {
    pub anchor: String,
    pub depth: usize,
    pub heading: String,
    pub raw: String,
}

static SLUG_DROP: LazyLock<Re> = LazyLock::new(|| Re::new(r"[^\p{L}\p{M}\p{N}_\s-]|\p{Variation_Selector}", "gu"));

/// A heading's anchor before duplicates are numbered (GitHub's slugs).
fn slug(block: &Block) -> String {
    let text = js_lower(&marked::inline_text(&block.inline));
    SLUG_DROP.replace(&text, true, |_| String::new()).replace(' ', "-")
}

/// Every heading with its anchor and its section's raw Markdown (to the next
/// heading of its level or higher).
pub fn heading_sections(tokens: &[Block]) -> Vec<Section> {
    struct H {
        anchor: String,
        depth: usize,
        heading: String,
        index: usize,
    }
    let mut heads: Vec<H> = Vec::new();
    let mut suffixes: HashMap<String, usize> = HashMap::new();
    let mut taken: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (i, t) in tokens.iter().enumerate() {
        if t.kind != Kind::Heading {
            continue;
        }
        let base = slug(t);
        let mut anchor = base.clone();
        let mut suffix = *suffixes.get(&base).unwrap_or(&0);
        while taken.contains(&anchor) {
            suffix += 1;
            anchor = format!("{base}-{suffix}");
        }
        suffixes.insert(base, suffix);
        taken.insert(anchor.clone());
        heads.push(H { anchor, depth: t.depth, heading: t.text.clone(), index: i });
    }
    if let Some(h) = heads.iter_mut().find(|h| h.anchor.is_empty()) {
        let mut fallback = SYMBOL_ONLY_ANCHOR.to_owned();
        let mut n = 1;
        while taken.contains(&fallback) {
            fallback = format!("{SYMBOL_ONLY_ANCHOR}-{n}");
            n += 1;
        }
        h.anchor = fallback;
    }
    heads
        .into_iter()
        .map(|h| {
            let mut end = h.index + 1;
            while end < tokens.len() && !(tokens[end].kind == Kind::Heading && tokens[end].depth <= h.depth) {
                end += 1;
            }
            let raw: String = tokens[h.index..end].iter().map(|t| t.raw.as_str()).collect();
            Section { anchor: h.anchor, depth: h.depth, heading: h.heading, raw: jsre::trim(&raw).to_owned() }
        })
        .collect()
}

// ---- Search ----

struct Unit {
    reference: String,
    title: String,
    description: String,
    heading: String,
    prose: String,
    intro_only: bool,
    identifiers: String,
    source_paths: Vec<String>,
    wiki: String,
}

static RELATED: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?i)^(related (pages|reading|links)|see also|navigation)$").unwrap());

fn source_resources(fields: &Map<String, Value>) -> Vec<String> {
    match fields.get("sources") {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_object()?.get("resource")?.as_str().filter(|r| r.starts_with("repo://")).map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

fn repository_path(resource: &str) -> String {
    let p = resource.strip_prefix("repo://").unwrap_or(resource);
    let p = p.split('#').next().unwrap_or("").replace('\\', "/");
    js_lower(p.strip_prefix("./").unwrap_or(&p))
}

fn search_units(markdown: &str, page: &str, wiki: &str) -> Vec<Unit> {
    let fields = front::fields(markdown).unwrap_or_default();
    if fields.get("status").and_then(Value::as_str) == Some("deprecated") {
        return Vec::new();
    }
    let body = front::body(markdown);
    if body.is_empty() {
        return Vec::new();
    }
    let relative = &page[1..];
    let title = front::string_field(fields.get("title")).unwrap_or_else(|| relative.to_owned());
    let description = front::string_field(fields.get("description")).unwrap_or_default();
    let tags = front::string_array(fields.get("tags"));
    let source_paths: Vec<String> = source_resources(&fields).iter().map(|r| repository_path(r)).collect();
    let tokens = marked::lex(body);
    let headings = heading_sections(&tokens);
    let identifiers = std::iter::once(relative.to_owned()).chain(tags).chain(source_paths.iter().cloned()).collect::<Vec<_>>().join(" ");
    let unit = |s: &Section, raw: &str| Unit {
        reference: format!("{relative}#{}", s.anchor),
        title: title.clone(),
        description: description.clone(),
        heading: if s.depth == 1 { String::new() } else { s.heading.clone() },
        prose: raw.to_owned(),
        intro_only: false,
        identifiers: identifiers.clone(),
        source_paths: source_paths.clone(),
        wiki: wiki.to_owned(),
    };
    let sections: Vec<&Section> = headings.iter().filter(|h| h.depth == 2 && !RELATED.is_match(jsre::trim(&h.heading))).collect();
    let first_h1 = headings.iter().find(|h| h.depth == 1);
    if !sections.is_empty() {
        let mut units: Vec<Unit> = sections.iter().map(|s| unit(s, &s.raw)).collect();
        if let Some(h1) = first_h1 {
            let page_tokens = marked::lex(&h1.raw);
            let first_h2 = page_tokens.iter().position(|t| t.kind == Kind::Heading && t.depth == 2).unwrap_or(page_tokens.len());
            let intro: String = page_tokens[..first_h2].iter().filter(|t| t.kind != Kind::Heading).map(|t| t.raw.as_str()).collect();
            if !jsre::trim(&intro).is_empty() {
                let mut u = unit(h1, &intro);
                u.intro_only = true;
                units.insert(0, u);
            }
        }
        return units;
    }
    if let Some(h1) = first_h1 {
        return vec![unit(h1, body)];
    }
    Vec::new()
}

static IMAGE: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"!\[([^\]]*)\]\([^)]*\)").unwrap());
static LINK: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"\[([^\]]+)\]\([^)]*\)").unwrap());
static MARKS: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"[`*_>#|]").unwrap());
static SPACES: LazyLock<Re> = LazyLock::new(|| Re::new(r"\s+", "gu"));

/// The block of a section matching the most terms, as plain text of at most
/// 600 characters.
fn relevant_excerpt(raw: &str, terms: &[String]) -> String {
    let blocks: Vec<Block> = marked::lex(raw).into_iter().filter(|t| t.kind != Kind::Heading && !jsre::trim(&t.raw).is_empty()).collect();
    let mut best: Option<(usize, usize)> = None;
    for (i, b) in blocks.iter().enumerate() {
        let text = js_lower(&normalize_text(&b.raw));
        let score = terms.iter().filter(|t| text.contains(t.as_str())).count();
        if best.is_none_or(|(s, _)| score > s) {
            best = Some((score, i));
        }
    }
    let Some((_, i)) = best else { return String::new() };
    let compact = IMAGE.replace_all(&blocks[i].raw, "$1");
    let compact = LINK.replace_all(&compact, "$1");
    let compact = MARKS.replace_all(&compact, " ");
    let compact = SPACES.replace(&compact, true, |_| " ".into());
    let compact = jsre::trim(&compact);
    if jsre::len16(compact) <= EXCERPT_CHARACTERS {
        return compact.to_owned();
    }
    let shortened = jsre::slice16(compact, 0, Some(EXCERPT_CHARACTERS as isize + 1));
    let cut = match shortened.rfind(' ') {
        Some(b) if b > 0 => &shortened[..b],
        _ => jsre::slice16(compact, 0, Some(EXCERPT_CHARACTERS as isize)),
    };
    format!("{cut}…")
}

fn path_match_count(unit: &Unit, expected: &[String]) -> usize {
    expected
        .iter()
        .filter(|e| unit.source_paths.iter().any(|a| a == *e || a.ends_with(&format!("/{e}")) || e.ends_with(&format!("/{a}"))))
        .count()
}

/// A wiki's sections indexed for search: which pages' texts (by wiki,
/// page, hash and length), its units in row order, and the FTS5 table.
struct Index {
    pages: Vec<(String, String, u64, usize)>,
    page_units: Vec<Arc<Vec<Unit>>>,
    db: rusqlite::Connection,
}

/// The last few indexes built, the latest first, by the wikis' roots (with
/// their pages' text size).
static INDEXES: Mutex<Vec<(Vec<std::path::PathBuf>, usize, Index)>> = Mutex::new(Vec::new());
const KEPT_INDEXES: usize = 4;
/// At most this much page text indexed is kept, all wikis together (the
/// oldest go first; a wiki bigger than it is indexed per search).
const KEPT_BYTES: usize = 32 << 20;

fn text_hash(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

fn build_index(pages: Vec<(String, String, u64, usize)>, page_units: Vec<Arc<Vec<Unit>>>) -> Result<Index> {
    let db = rusqlite::Connection::open_in_memory().map_err(|e| Error::Other(e.to_string()))?;
    let fill = || -> rusqlite::Result<()> {
        db.execute_batch("CREATE VIRTUAL TABLE units USING fts5(title, description, heading, prose, identifiers, tokenize='porter unicode61'); BEGIN")?;
        {
            let mut insert = db.prepare("INSERT INTO units(rowid, title, description, heading, prose, identifiers) VALUES (?, ?, ?, ?, ?, ?)")?;
            for (i, u) in page_units.iter().flat_map(|p| p.iter()).enumerate() {
                let (t, d, id) = if u.intro_only {
                    (String::new(), String::new(), String::new())
                } else {
                    (normalize_text(&u.title), normalize_text(&u.description), normalize_text(&u.identifiers))
                };
                insert.execute(rusqlite::params![i as i64 + 1, t, d, normalize_text(&u.heading), normalize_text(&u.prose), id])?;
            }
        }
        db.execute_batch("COMMIT")
    };
    fill().map_err(|e| Error::Other(e.to_string()))?;
    Ok(Index { pages, page_units, db })
}

fn rank(index: &Index, terms: &[String], paths: &[String], limit: usize, include_wiki: bool) -> Result<Vec<Value>> {
    let units: Vec<&Unit> = index.page_units.iter().flat_map(|p| p.iter()).collect();
    let db = &index.db;
    let run = || -> rusqlite::Result<Vec<(i64, f64)>> {
        let quoted: Vec<String> = terms.iter().map(|t| format!("\"{t}\"")).collect();
        let mut ranked: Vec<(i64, f64)> = db
            .prepare_cached("SELECT rowid, bm25(units, 8, 4, 6, 1, 3) AS score FROM units WHERE units MATCH ? ORDER BY score, rowid")?
            .query_map([quoted.join(" OR ")], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut coverage: HashMap<i64, usize> = HashMap::new();
        let mut matching = db.prepare_cached("SELECT rowid FROM units WHERE units MATCH ?")?;
        for q in &quoted {
            for row in matching.query_map([q], |r| r.get::<_, i64>(0))? {
                *coverage.entry(row?).or_default() += 1;
            }
        }
        let path_scores: Vec<usize> = if paths.is_empty() { vec![0; units.len()] } else { units.iter().map(|u| path_match_count(u, paths)).collect() };
        ranked.sort_by(|(a, sa), (b, sb)| {
            let pa = path_scores[*a as usize - 1];
            let pb = path_scores[*b as usize - 1];
            pb.cmp(&pa)
                .then_with(|| coverage.get(b).unwrap_or(&0).cmp(coverage.get(a).unwrap_or(&0)))
                .then_with(|| sa.partial_cmp(sb).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| a.cmp(b))
        });
        Ok(ranked)
    };
    let ranked = run().map_err(|e| Error::Other(e.to_string()))?;
    Ok(ranked
        .into_iter()
        .take(limit)
        .map(|(rowid, _)| {
            let u = units[rowid as usize - 1];
            let heading = if u.heading.is_empty() { String::new() } else { format!("Section: {}", u.heading) };
            let excerpt = relevant_excerpt(&u.prose, terms);
            let content = [u.title.as_str(), heading.as_str(), u.description.as_str(), excerpt.as_str()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            let mut r = Map::new();
            r.insert("kind".into(), "section".into());
            r.insert("ref".into(), json!([u.reference]));
            r.insert("content".into(), content.into());
            if include_wiki {
                r.insert("wiki".into(), u.wiki.clone().into());
            }
            Value::Object(r)
        })
        .collect())
}

fn normalize_path_hint(value: &str) -> Result<String> {
    let t = jsre::trim(value).replace('\\', "/");
    let n = t.strip_prefix("./").unwrap_or(&t).to_owned();
    let bad = n.is_empty()
        || jsre::len16(&n) > SOURCE_PATH_CHARACTERS
        || n.starts_with('/')
        || n.contains(':')
        || n.split('/').any(|p| p.is_empty() || p == "." || p == "..")
        || n.chars().any(|c| c < ' ' || "*?[]".contains(c));
    if bad {
        return Err(retrieval("Source hints must be repository-relative paths without traversal or globs."));
    }
    Ok(js_lower(&n))
}

pub struct SearchRequest<'a> {
    pub query: &'a str,
    pub paths: Option<Vec<String>>,
    pub limit: Option<f64>,
    pub workspace: Option<&'a str>,
}

/// `openwiki_search`.
type PageKey = (String, String, u64, usize);

/// Each page's sections: an unchanged page's kept ones, the others made on
/// as many threads as help (a first search of a big wiki is mostly this).
fn units_of(pages: &[PageKey], texts: &[String], old: &HashMap<&PageKey, &Arc<Vec<Unit>>>) -> Vec<Arc<Vec<Unit>>> {
    let mut out: Vec<Option<Arc<Vec<Unit>>>> = pages.iter().map(|k| old.get(k).map(|u| Arc::clone(u))).collect();
    let todo: Vec<usize> = (0..pages.len()).filter(|&i| out[i].is_none()).collect();
    let bytes: usize = todo.iter().map(|&i| texts[i].len()).sum();
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(8).min(bytes / (64 << 10) + 1);
    if threads <= 1 {
        for i in todo {
            out[i] = Some(Arc::new(search_units(&texts[i], &pages[i].1, &pages[i].0)));
        }
    } else {
        let next = std::sync::atomic::AtomicUsize::new(0);
        let made: Vec<Vec<(usize, Vec<Unit>)>> = std::thread::scope(|s| {
            let workers: Vec<_> = (0..threads)
                .map(|_| {
                    s.spawn(|| {
                        let mut mine = Vec::new();
                        loop {
                            let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let Some(&i) = todo.get(k) else { break };
                            mine.push((i, search_units(&texts[i], &pages[i].1, &pages[i].0)));
                        }
                        mine
                    })
                })
                .collect();
            workers.into_iter().map(|w| w.join().unwrap_or_else(|e| std::panic::resume_unwind(e))).collect()
        });
        for (i, units) in made.into_iter().flatten() {
            out[i] = Some(Arc::new(units));
        }
    }
    out.into_iter().map(Option::unwrap_or_default).collect()
}

pub fn search(root: &Path, req: &SearchRequest) -> Result<Value> {
    let query = jsre::trim(req.query);
    let limit = req.limit.unwrap_or(DEFAULT_SEARCH_RESULTS as f64);
    if query.is_empty() || jsre::len16(query) > QUERY_CHARACTERS {
        return Err(retrieval(format!("Use a non-empty search query of at most {QUERY_CHARACTERS} characters.")));
    }
    if limit.fract() != 0.0 || limit < 1.0 || limit > SEARCH_RESULTS as f64 {
        return Err(retrieval(format!("Search limit must be an integer from 1 to {SEARCH_RESULTS}.")));
    }
    let paths = req.paths.clone().unwrap_or_default().iter().map(|p| normalize_path_hint(p)).collect::<Result<Vec<_>>>()?;
    if paths.len() > SOURCE_PATH_HINTS {
        return Err(retrieval(format!("Search accepts at most {SOURCE_PATH_HINTS} source path hints.")));
    }
    if let Some(w) = req.workspace {
        if jsre::trim(w).is_empty() || jsre::len16(w) > WORKSPACE_REFERENCE_CHARACTERS {
            return Err(retrieval("Use a valid workspace ID or name returned by openwiki_list_workspaces."));
        }
    }
    let scope = workspaces::search_scope(root, req.workspace.map(jsre::trim))?;
    let (workspace, wikis) = match scope {
        Scope::Required(v) => return Ok(v),
        Scope::Ready { workspace, wikis } => (workspace, wikis),
    };
    let wrap = |results: Vec<Value>| -> Value {
        match &workspace {
            Some(ws) => json!({"results": results, "workspace": ws.summary(), "wikis": wikis.iter().map(|w| w.identity()).collect::<Vec<_>>()}),
            None => json!({"results": results}),
        }
    };
    let terms = query_terms(query);
    if terms.is_empty() {
        return Ok(wrap(Vec::new()));
    }
    // The pages' texts, then the index kept for these wikis when none
    // changed, else one built again (reusing unchanged pages' sections).
    let roots: Vec<std::path::PathBuf> = wikis.iter().map(|w| w.root.clone()).collect();
    let kept = {
        let mut all = INDEXES.lock().unwrap();
        all.iter().position(|(r, _, _)| *r == roots).map(|i| all.remove(i).2)
    };
    let mut pages = Vec::new();
    let mut texts = Vec::new();
    for wiki in &wikis {
        let store = Store::new(&wiki.root);
        for page in store.discover_pages()? {
            if page.split('/').any(|s| s.starts_with('.')) {
                continue;
            }
            let markdown = store.read_markdown(&page)?;
            pages.push((wiki.id.clone(), page, text_hash(&markdown), markdown.len()));
            texts.push(markdown);
        }
    }
    let index = match kept {
        Some(index) if index.pages == pages => index,
        kept => {
            let old: HashMap<&(String, String, u64, usize), &Arc<Vec<Unit>>> = kept.as_ref().map(|k| k.pages.iter().zip(&k.page_units).collect()).unwrap_or_default();
            let page_units = units_of(&pages, &texts, &old);
            drop(old);
            build_index(pages, page_units)?
        }
    };
    let results = if index.page_units.iter().all(|p| p.is_empty()) { Vec::new() } else { rank(&index, &terms, &paths, limit as usize, workspace.is_some())? };
    let bytes: usize = texts.iter().map(String::len).sum();
    if bytes <= KEPT_BYTES {
        let mut all = INDEXES.lock().unwrap();
        all.insert(0, (roots, bytes, index));
        let mut total = 0;
        let keep = all.iter().take(KEPT_INDEXES).take_while(|(_, b, _)| {
            total += b;
            total <= KEPT_BYTES
        });
        let keep = keep.count();
        all.truncate(keep);
    }
    Ok(wrap(results))
}

fn normalize_retrievable_page(page: &str) -> Result<String> {
    let normalized = crate::store::normalize_page(page).map_err(|_| retrieval(INVALID_PAGE))?;
    if jsre::len16(jsre::trim(page)) > PAGE_CHARACTERS || normalized.split('/').any(|s| s.starts_with('.')) {
        return Err(retrieval(INVALID_PAGE));
    }
    Ok(normalized)
}

fn normalize_anchor(value: &str) -> Result<String> {
    let t = jsre::trim(value);
    let n = t.strip_prefix('#').unwrap_or(t);
    if n.is_empty() || jsre::len16(n) > SECTION_ANCHOR_CHARACTERS || n.contains('#') || n.contains('/') || n.chars().any(|c| c < ' ') {
        return Err(retrieval("Sections must be heading anchors without a page path."));
    }
    Ok(n.to_owned())
}

pub struct ReadRequest<'a> {
    pub page: &'a str,
    pub sections: Vec<String>,
    pub wiki: Option<&'a str>,
    pub include_provenance: bool,
}

/// `openwiki_read`.
pub fn read(root: &Path, req: &ReadRequest) -> Result<Value> {
    if req.sections.is_empty() {
        return Err(retrieval("Provide at least one section anchor."));
    }
    if req.sections.len() > SECTION_ANCHORS {
        return Err(retrieval(format!("Read accepts at most {SECTION_ANCHORS} section anchors.")));
    }
    if let Some(w) = req.wiki {
        if jsre::trim(w).is_empty() || jsre::len16(w) > WIKI_ID_CHARACTERS {
            return Err(retrieval("Use a valid linked wiki ID returned by search."));
        }
    }
    let wiki = workspaces::readable_wiki(root, req.wiki.map(jsre::trim))?;
    let page = normalize_retrievable_page(req.page)?;
    let requested = req.sections.iter().map(|s| normalize_anchor(s)).collect::<Result<Vec<_>>>()?;
    let store = Store::new(&wiki.root);
    let markdown = store.read_markdown(&page)?;
    let tokens = marked::lex(front::body(&markdown));
    let mut available: HashMap<String, String> = HashMap::new();
    for s in heading_sections(&tokens) {
        available.insert(s.anchor, s.raw);
    }
    let missing: Vec<&String> = requested.iter().filter(|s| !available.contains_key(*s)).collect();
    if !missing.is_empty() {
        let list = missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
        return Err(retrieval(format!("Unknown section{}: {list}", if missing.len() == 1 { "" } else { "s" })));
    }
    let mut out = Map::new();
    out.insert("page".into(), page[1..].to_owned().into());
    out.insert("sections".into(), requested.iter().map(|s| json!({"section": s, "content": available[s]})).collect());
    if req.include_provenance {
        out.insert("provenance".into(), provenance(&store, &page, &markdown)?);
    }
    if req.wiki.is_some() {
        out.insert("wiki".into(), wiki.id.into());
    }
    Ok(Value::Object(out))
}

fn provenance(store: &Store, page: &str, markdown: &str) -> Result<Value> {
    let fields = front::fields(markdown).unwrap_or_default();
    let sources: Vec<Value> = match fields.get("sources") {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| {
                let o = v.as_object()?;
                let resource = front::string_field(o.get("resource"))?;
                let mut m = Map::new();
                if let Some(id) = front::string_field(o.get("id")) {
                    m.insert("id".into(), id.into());
                }
                m.insert("resource".into(), resource.into());
                Some(Value::Object(m))
            })
            .collect(),
        _ => Vec::new(),
    };
    let claims: Vec<Value> = match store.load_page(page)? {
        Some(p) => p["claims"]
            .as_array()
            .map(|cs| {
                cs.iter()
                    .map(|c| {
                        let ev: Vec<Value> = c["evidence"]
                            .as_array()
                            .map(|e| e.iter().map(|e| json!({"resource": e["resource"], "version": e["version"]})).collect())
                            .unwrap_or_default();
                        json!({"id": c["id"], "evidence": ev})
                    })
                    .collect()
            })
            .unwrap_or_default(),
        None => Vec::new(),
    };
    Ok(json!({"sources": sources, "claims": claims}))
}

/// Whether a page is part of what search covers (for callers listing pages).
pub fn is_retrievable(page: &str) -> bool {
    is_grounded(page) && !page.split('/').any(|s| s.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_and_normalization() {
        assert_eq!(normalize_text("jobQueue HTTPServer a_b"), "jobQueue HTTPServer a_b job Queue HTTP Server a b");
        assert_eq!(query_terms("How does the jobQueue work"), ["jobqueue", "work", "job", "queue"]);
        assert_eq!(query_terms("the a"), ["the", "a"]);
    }

    /// The index kept between searches follows the pages: an edit, a page
    /// added and one removed each show in the next search.
    #[test]
    fn kept_index_follows_the_pages() {
        let root = std::env::temp_dir().join(format!("harlo-wiki-kept-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("openwiki")).unwrap();
        // (No linked wikis: not the user's registry.)
        unsafe { std::env::set_var("OPENWIKI_CONFIG_DIR", root.join("no-registry")) };
        let write = |p: &str, t: &str| std::fs::write(root.join("openwiki").join(p), t).unwrap();
        let refs = |q: &str| -> Vec<String> {
            let found = search(&root, &SearchRequest { query: q, paths: None, limit: Some(20.0), workspace: None }).unwrap();
            found["results"].as_array().unwrap().iter().map(|r| r["ref"][0].as_str().unwrap().to_owned()).collect()
        };
        write("a.md", "# A\n\n## Retry\n\nJobs retry thrice.\n");
        write("b.md", "# B\n\n## Queue\n\nJobs wait in the queue.\n");
        assert_eq!(refs("retry"), ["openwiki/a.md#retry"]);
        assert_eq!(refs("retry"), ["openwiki/a.md#retry"], "the same, from the kept index");
        write("b.md", "# B\n\n## Queue\n\nJobs wait, then retry.\n");
        assert_eq!(refs("retry").len(), 2, "an edited page");
        write("c.md", "# C\n\n## Backoff\n\nA retry waits longer each time.\n");
        assert_eq!(refs("retry").len(), 3, "a new page");
        std::fs::remove_file(root.join("openwiki/a.md")).unwrap();
        assert_eq!(refs("retry"), ["openwiki/b.md#queue", "openwiki/c.md#backoff"], "a removed page");
        assert!(refs("thrice").is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
