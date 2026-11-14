//! What a wiki run does to its pages when it finishes, and before its
//! authoring starts, as OpenWiki does it: front matter repaired, broken
//! Mermaid diagrams turned to text, each folder's index.md written, broken
//! internal links stamped, Claims' evidence listed as sources, and changed
//! pages stamped as generated.

use crate::claims::{encode_component, decode_component, format_resource, parse_resource, Resource};
use crate::fsb::{Backend, Entry, ReadError, dirname, ext_lower, join};
use crate::jsre::{self, Re};
use crate::okf;
use crate::store::posix_normalize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

pub const ROOT: &str = "/openwiki";
const EXCLUDED: [&str; 3] = ["index.md", "log.md", "INSTRUCTIONS.md"];

pub type R<T> = std::result::Result<T, String>;

fn read_text(b: &Backend, p: &str) -> R<String> {
    match b.read_raw(p) {
        Ok(s) => Ok(s),
        Err(ReadError::Error(e)) => Err(format!("Unable to read {p}: {e}")),
        Err(ReadError::Thrown(e)) => Err(e),
    }
}

fn listed_page(e: &Entry) -> bool {
    let name = e.name();
    !e.is_dir && !name.is_empty() && !name.starts_with('.') && ext_lower(name) == ".md" && !EXCLUDED.contains(&name)
}

/// Every page under a folder, sorted by UTF-16 code units (mermaid's and
/// the link check's `listMarkdownFiles`).
fn markdown_files(b: &Backend, dir: &str) -> Vec<String> {
    let Ok(entries) = b.ls(dir) else { return Vec::new() };
    let mut files = Vec::new();
    for e in &entries {
        let name = e.name();
        if name.is_empty() || name.starts_with('.') {
            continue;
        }
        let p = join(dir, name);
        if e.is_dir {
            files.extend(markdown_files(b, &p));
        } else if listed_page(e) {
            files.push(p);
        }
    }
    files.sort_by(|a, b| crate::util::cmp16(a, b));
    files
}

// ---- Folders and pages ----

pub struct Dir {
    pub path: String,
    pub entries: Vec<Entry>,
}

/// `collectDirectories`: each folder's descendants first, then itself.
pub fn directories(b: &Backend, dir: &str, allow_missing: bool) -> R<Vec<Dir>> {
    let entries = match b.ls(dir) {
        Ok(e) => e,
        Err(_) if allow_missing => return Ok(Vec::new()),
        Err(e) => return Err(format!("Unable to list {dir}: {e}")),
    };
    let mut out = Vec::new();
    for e in entries.iter().filter(|e| e.is_dir && !e.name().starts_with('.')) {
        out.extend(directories(b, &join(dir, e.name()), false)?);
    }
    out.push(Dir { path: dir.to_owned(), entries });
    Ok(out)
}

/// `listWikiConceptPaths`: the wiki's pages, sorted as `localeCompare` sorts.
pub fn concept_paths(b: &Backend) -> R<Vec<String>> {
    let mut out: Vec<String> = directories(b, ROOT, true)?
        .iter()
        .flat_map(|d| d.entries.iter().filter(|e| listed_page(e)).map(|e| join(&d.path, e.name())).collect::<Vec<_>>())
        .collect();
    out.sort_by(|a, b| crate::collate::compare(a, b));
    Ok(out)
}

fn normalize_concept(b: &Backend, p: &str, concept_type: &str) -> R<String> {
    let original = read_text(b, p)?;
    let (changed, content) = okf::repair(&original, p, concept_type);
    if changed {
        b.edit(p, &original, &content).map_err(|e| format!("Unable to normalize {p}: {e}"))?;
    }
    Ok(content)
}

/// `migrateWikiToOkf`: every page's front matter repaired.
pub fn migrate(b: &Backend, concept_type: &str) -> R<()> {
    for p in concept_paths(b)? {
        normalize_concept(b, &p, concept_type)?;
    }
    Ok(())
}

// ---- Labels ----

const INDEX_LABELS: &[(&str, &str, &str)] = &[
    ("en", "Files", "Directories"),
    ("ar", "ملفات", "مجلدات"),
    ("bg", "Файлове", "Директории"),
    ("ca", "Fitxers", "Directoris"),
    ("cs", "Soubory", "Adresáře"),
    ("da", "Filer", "Mapper"),
    ("de", "Dateien", "Verzeichnisse"),
    ("el", "Αρχεία", "Κατάλογοι"),
    ("es", "Archivos", "Directorios"),
    ("fi", "Tiedostot", "Hakemistot"),
    ("fr", "Fichiers", "Répertoires"),
    ("he", "קבצים", "תיקיות"),
    ("hi", "फ़ाइलें", "निर्देशिकाएँ"),
    ("hr", "Datoteke", "Direktoriji"),
    ("hu", "Fájlok", "Könyvtárak"),
    ("id", "Berkas", "Direktori"),
    ("it", "File", "Cartelle"),
    ("ja", "ファイル", "ディレクトリ"),
    ("ko", "파일", "디렉터리"),
    ("ms", "Fail", "Direktori"),
    ("nb", "Filer", "Mapper"),
    ("nl", "Bestanden", "Mappen"),
    ("no", "Filer", "Mapper"),
    ("pl", "Pliki", "Katalogi"),
    ("pt", "Arquivos", "Diretórios"),
    ("pt-PT", "Ficheiros", "Diretórios"),
    ("ro", "Fișiere", "Directoare"),
    ("ru", "Файлы", "Каталоги"),
    ("sk", "Súbory", "Adresáre"),
    ("sl", "Datoteke", "Mape"),
    ("sr", "Датотеке", "Директоријуми"),
    ("sv", "Filer", "Kataloger"),
    ("th", "ไฟล์", "ไดเรกทอรี"),
    ("tr", "Dosyalar", "Dizinler"),
    ("uk", "Файли", "Каталоги"),
    ("vi", "Tập tin", "Thư mục"),
    ("zh", "文件", "目录"),
    ("zh-TW", "檔案", "目錄"),
];

const CONCEPT_TYPES: &[(&str, &str)] = &[
    ("en", "Reference"),
    ("ar", "مرجع"),
    ("ca", "Referència"),
    ("cs", "Reference"),
    ("da", "Reference"),
    ("de", "Referenz"),
    ("el", "Αναφορά"),
    ("es", "Referencia"),
    ("fr", "Référence"),
    ("hi", "संदर्भ"),
    ("hr", "Referenca"),
    ("id", "Referensi"),
    ("it", "Riferimento"),
    ("ja", "リファレンス"),
    ("ko", "참조"),
    ("ms", "Rujukan"),
    ("nb", "Referanse"),
    ("nl", "Referentie"),
    ("no", "Referanse"),
    ("pt", "Referência"),
    ("ro", "Referință"),
    ("ru", "Справочник"),
    ("sk", "Referencia"),
    ("sr", "Референца"),
    ("sv", "Referens"),
    ("th", "อ้างอิง"),
    ("tr", "Referans"),
    ("uk", "Довідник"),
    ("vi", "Tham khảo"),
    ("zh", "参考"),
    ("zh-TW", "參考"),
];

/// `new Intl.Locale(tag).language`, the tag itself when it isn't one.
fn primary_subtag(tag: &str) -> String {
    match crate::lang::canonicalize(tag) {
        Some(_) => crate::lang::primary(Some(tag)).unwrap_or_else(|| "und".into()),
        None => tag.to_owned(),
    }
}

fn lookup<T: Copy>(table: &[(&str, T)], language: Option<&str>, default: T) -> T {
    let Some(l) = language.filter(|l| !l.is_empty()) else { return default };
    let find = |k: &str| table.iter().find(|(c, _)| *c == k).map(|(_, v)| *v);
    find(l).or_else(|| find(&primary_subtag(l))).unwrap_or(default)
}

/// (files, directories) headings for a language.
pub fn index_labels(language: Option<&str>) -> (&'static str, &'static str) {
    let table: Vec<(&str, (&str, &str))> = INDEX_LABELS.iter().map(|(c, f, d)| (*c, (*f, *d))).collect();
    lookup(&table, language, ("Files", "Directories"))
}

/// `resolveConceptTypeLabel`.
pub fn concept_type(language: Option<&str>) -> &'static str {
    lookup(CONCEPT_TYPES, language, "Reference")
}

// ---- Indexes ----

struct Link {
    description: Option<String>,
    href: String,
    label: String,
}

fn escape_label(v: &str) -> String {
    v.replace('\\', "\\\\").replace('[', "\\[").replace(']', "\\]")
}

fn render_links(heading: &str, links: &mut [Link], describe: bool) -> String {
    if links.is_empty() {
        return String::new();
    }
    links.sort_by(|a, b| crate::collate::compare(&a.href, &b.href));
    let items: Vec<String> = links
        .iter()
        .map(|l| {
            let link = format!("- [{}]({})", escape_label(&l.label), l.href);
            match &l.description {
                Some(d) if describe && !d.is_empty() => format!("{link} - {d}"),
                _ => link,
            }
        })
        .collect();
    format!("# {heading}\n\n{}", items.join("\n"))
}

fn usable(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).filter(|s| !jsre::trim(s).is_empty()).map(str::to_owned)
}

/// `synchronizeWikiIndexes`.
pub fn sync_indexes(b: &Backend, labels: (&str, &str), concept_type: &str) -> R<()> {
    for d in directories(b, ROOT, true)? {
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        for e in &d.entries {
            let name = e.name();
            if name.is_empty() || name.starts_with('.') {
                continue;
            }
            if e.is_dir {
                dirs.push(Link { description: None, href: format!("{}/", encode_component(name)), label: name.to_owned() });
                continue;
            }
            if ext_lower(name) != ".md" || EXCLUDED.contains(&name) {
                continue;
            }
            let p = join(&d.path, name);
            let content = normalize_concept(b, &p, concept_type)?;
            let f = okf::fields(&content);
            let description = f.as_ref().and_then(|f| usable(f.get("description")));
            let title = f.as_ref().and_then(|f| usable(f.get("title")));
            let stem = name.strip_suffix(".md").unwrap_or(name);
            files.push(Link { description, href: encode_component(name), label: title.unwrap_or_else(|| stem.to_owned()) });
        }
        let index = join(&d.path, "index.md");
        let sections: Vec<String> = [render_links(labels.0, &mut files, true), render_links(labels.1, &mut dirs, false)].into_iter().filter(|s| !s.is_empty()).collect();
        let body = if sections.is_empty() { format!("# {}", labels.0) } else { sections.join("\n\n") };
        let version = if d.path == ROOT { "---\nokf_version: \"0.2\"\n---\n\n" } else { "" };
        let content = format!("{version}{body}\n");
        let existing = if d.entries.iter().any(|e| !e.is_dir && e.name() == "index.md") { Some(read_text(b, &index)?) } else { None };
        if existing.as_deref() == Some(content.as_str()) {
            continue;
        }
        let r = match existing.as_deref() {
            Some(old) if !old.is_empty() => b.edit(&index, old, &content),
            _ => b.write(&index, &content),
        };
        r.map_err(|e| format!("Unable to write {index}: {e}"))?;
    }
    Ok(())
}

// ---- Mermaid ----

struct Fence {
    open: usize,
    close: usize,
    indent: String,
    marker: String,
    body: String,
}

fn mermaid_fences(md: &str) -> Vec<Fence> {
    static F: LazyLock<Re> = LazyLock::new(|| Re::new(r"^(\s*)(`{3,})\s*(\S*)\s*$", "u"));
    let lines: Vec<&str> = md.split('\n').collect();
    let mut out = Vec::new();
    let mut open: Option<(usize, String, String, Vec<&str>)> = None;
    let mut generic: Option<String> = None;
    for (i, line) in lines.iter().enumerate() {
        let m = F.exec(line);
        if let Some((o, indent, marker, body)) = open.as_mut() {
            if let Some(m) = m.as_ref().filter(|m| m.str(2).len() >= marker.len() && m.str(3).is_empty()) {
                let _ = m;
                out.push(Fence { open: *o, close: i, indent: indent.clone(), marker: marker.clone(), body: body.join("\n") });
                open = None;
            } else {
                body.push(line);
            }
            continue;
        }
        if let Some(g) = &generic {
            if m.as_ref().is_some_and(|m| m.str(2).len() >= g.len() && m.str(3).is_empty()) {
                generic = None;
            }
            continue;
        }
        if let Some(m) = m {
            if m.str(3).to_lowercase() == "mermaid" {
                open = Some((i, m.str(1).to_owned(), m.str(2).to_owned(), Vec::new()));
            } else if !m.str(3).is_empty() {
                generic = Some(m.str(2).to_owned());
            }
        }
    }
    out
}

/// What makes a diagram fail to draw, judged without a Mermaid parser.
pub fn heuristic_error(body: &str) -> Option<&'static str> {
    static END_NODE: LazyLock<Re> = LazyLock::new(|| Re::new(r"(?:^|\n|\s)end\s*[[({]", "u"));
    static END_TARGET: LazyLock<Re> = LazyLock::new(|| Re::new(r"-->\s*end\s*(?:$|\n|;)", "mu"));
    static SEMI: LazyLock<Re> = LazyLock::new(|| Re::new(r"[[({][^)\]}]*;[^)\]}]*[)\]}]", "u"));
    static ANGLE: LazyLock<Re> = LazyLock::new(|| Re::new(r"[[({][^)\]}]*[<>][^)\]}]*[)\]}]", "u"));
    let first = jsre::trim(body).split(jsre::is_js_space).next().unwrap_or("").to_lowercase();
    let flowchart = first == "flowchart" || first == "graph";
    if flowchart && (END_NODE.test(body) || END_TARGET.test(body)) {
        return Some("Heuristic: `end` is a reserved word and cannot be a flowchart node id; rename the node.");
    }
    if SEMI.test(body) {
        return Some("Heuristic: a semicolon inside a label breaks rendering; rephrase the label.");
    }
    if ANGLE.test(body) {
        return Some("Heuristic: an unescaped angle bracket inside a label breaks rendering; rephrase the label.");
    }
    None
}

/// `degradeInvalidMermaidFences`: (content, how many were turned to text).
pub fn degrade_mermaid(md: &str) -> (String, usize) {
    let errors: Vec<(Fence, &str)> = mermaid_fences(md).into_iter().filter_map(|f| heuristic_error(&f.body).map(|e| (f, e))).collect();
    if errors.is_empty() {
        return (md.to_owned(), 0);
    }
    let mut lines: Vec<String> = md.split('\n').map(str::to_owned).collect();
    for (f, error) in errors.iter().rev() {
        let mut repl = vec![
            format!("{}<!-- openwiki: mermaid parse failed and this diagram was converted to a text fence so it does not break rendering. Fix the diagram source and restore the mermaid fence. Parser error: {error} -->", f.indent),
            format!("{}{}text", f.indent, f.marker),
        ];
        repl.extend(f.body.split('\n').map(str::to_owned));
        repl.push(format!("{}{}", f.indent, f.marker));
        lines.splice(f.open..=f.close, repl);
    }
    (lines.join("\n"), errors.len())
}

/// `validateWikiMermaid`.
pub fn validate_mermaid(b: &Backend) -> R<()> {
    for p in markdown_files(b, ROOT) {
        let original = read_text(b, &p)?;
        let (content, degraded) = degrade_mermaid(&original);
        if degraded == 0 {
            continue;
        }
        b.edit(&p, &original, &content).map_err(|e| format!("Unable to rewrite {p}: {e}"))?;
    }
    Ok(())
}

// ---- Internal links ----

static LINK: LazyLock<Re> = LazyLock::new(|| Re::new(r"\[([^\]]*)\]\(([^)]+)\)", "gu"));
static FENCE: LazyLock<Re> = LazyLock::new(|| Re::new(r"^\s*(`{3,}|~{3,})", "u"));
static INLINE_CODE: LazyLock<Re> = LazyLock::new(|| Re::new(r"(?<!`)(`+)(?!`).*?(?<!`)\1(?!`)", "gu"));
static HEADING: LazyLock<Re> = LazyLock::new(|| Re::new(r"^(#{1,6})\s+(.+?)\s*#*\s*$", "u"));
static LINE_ANCHOR: LazyLock<Re> = LazyLock::new(|| Re::new(r"^L\d+(?:C\d+)?(?:-L\d+(?:C\d+)?)?$", "u"));
static STAMP: LazyLock<Re> = LazyLock::new(|| Re::new(r"^\s*<!--\s*openwiki:\s*broken internal link\b.*?-->\s*$", "u"));
static CRLF: LazyLock<Re> = LazyLock::new(|| Re::new(r"\r?\n", "u"));

fn split_lines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut last = 0;
    for m in CRLF.all(s) {
        out.push(&s[last..m.start()]);
        last = m.end();
    }
    out.push(&s[last..]);
    out
}

struct LinkIssue {
    href: String,
    line: usize,
    message: String,
}

fn mask_code(content: &str, inline: bool) -> Vec<String> {
    let mut fence: Option<(char, usize)> = None;
    split_lines(content)
        .into_iter()
        .map(|line| {
            let marker = FENCE.exec(line).map(|m| m.str(1).to_owned());
            if let Some((ch, len)) = fence {
                if let Some(mk) = &marker {
                    if mk.starts_with(ch) && mk.len() >= len && jsre::trim(line) == mk {
                        fence = None;
                    }
                }
                return String::new();
            }
            if let Some(mk) = marker {
                fence = Some((mk.chars().next().unwrap(), mk.len()));
                return String::new();
            }
            if inline { INLINE_CODE.replace(line, true, |m| " ".repeat(jsre::len16(m.whole()))) } else { line.to_owned() }
        })
        .collect()
}

fn links_of(content: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    for (i, line) in mask_code(content, true).iter().enumerate() {
        for m in LINK.all(line) {
            if m.start() > 0 && line.as_bytes()[m.start() - 1] == b'!' {
                continue;
            }
            out.push((m.str(2).to_owned(), i + 1));
        }
    }
    out
}

fn slug(text: &str) -> String {
    static STRIP: LazyLock<Re> = LazyLock::new(|| Re::new(r"[^\p{L}\p{M}\p{N}\s_-]", "gu"));
    static SPACE: LazyLock<Re> = LazyLock::new(|| Re::new(r"\s", "gu"));
    let lower = jsre::trim(text).to_lowercase();
    let stripped = STRIP.replace(&lower, true, |_| String::new());
    SPACE.replace(&stripped, true, |_| "-".into())
}

fn anchors(content: &str) -> HashSet<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut out = HashSet::new();
    for line in mask_code(content, false) {
        let Some(m) = HEADING.exec(&line) else { continue };
        let base = slug(m.str(2));
        if base.is_empty() {
            continue;
        }
        let c = counts.entry(base.clone()).or_insert(0);
        out.insert(if *c == 0 { base.clone() } else { format!("{base}-{c}") });
        *c += 1;
    }
    out
}

fn decode_anchor(a: &str) -> String {
    decode_component(a).unwrap_or_else(|| a.to_owned())
}

fn path_exists(b: &Backend, target: &str, dir: bool) -> bool {
    if dir { b.ls(target).is_ok() } else { b.read_raw(target).is_ok() }
}

fn check_link(b: &Backend, source: &str, raw: &str, line: usize, own: &HashSet<String>) -> R<Option<LinkIssue>> {
    static EXTERNAL: LazyLock<Re> = LazyLock::new(|| Re::new(r"^(?:[a-z][a-z\d+.-]*:|\/\/)", "iu"));
    static TITLE: LazyLock<Re> = LazyLock::new(|| Re::new(r#"\s+(["']).*\1\s*$"#, "u"));
    let href = jsre::trim(raw).to_owned();
    if href.is_empty() || EXTERNAL.test(&href) {
        return Ok(None);
    }
    let issue = |message: String| Ok(Some(LinkIssue { href: href.clone(), line, message }));
    let without = TITLE.replace(&href, false, |_| String::new());
    let without = jsre::trim(&without);
    let (path, anchor) = match without.find('#') {
        None => (without, None),
        Some(i) => (&without[..i], Some(&without[i + 1..])),
    };
    if path.is_empty() {
        let Some(a) = anchor.filter(|a| !a.is_empty()) else { return Ok(None) };
        if !own.contains(&decode_anchor(a)) {
            return issue(format!("heading anchor \"{a}\" does not exist in {source}"));
        }
        return Ok(None);
    }
    if path.starts_with('/') {
        return issue(format!("link \"{path}\" is root-absolute, which no real consumer resolves against the repository root (not a coding agent reading the page, not GitHub's Markdown renderer, not a local viewer); use a path relative to this file instead"));
    }
    let resolved = posix_normalize(&join(&dirname(source), path));
    if !resolved.starts_with('/') {
        return issue(format!("link \"{path}\" cannot be resolved"));
    }
    let dir = resolved.ends_with('/');
    let target = if dir { resolved.trim_end_matches('/').to_owned() } else { resolved };
    if !path_exists(b, &target, dir) {
        return issue(if dir { format!("directory \"{path}\" does not exist") } else { format!("file \"{path}\" does not exist") });
    }
    let Some(a) = anchor.filter(|a| !a.is_empty()) else { return Ok(None) };
    if dir || ext_lower(&target) != ".md" || LINE_ANCHOR.test(a) {
        return Ok(None);
    }
    let text = read_text(b, &target)?;
    if !anchors(&text).contains(&decode_anchor(a)) {
        return issue(format!("heading anchor \"{a}\" does not exist in \"{path}\""));
    }
    Ok(None)
}

fn stamp(content: &str, issues: &[LinkIssue]) -> String {
    if issues.is_empty() {
        return content.to_owned();
    }
    let mut lines: Vec<String> = split_lines(content).into_iter().map(str::to_owned).collect();
    let mut by_line: Vec<usize> = issues.iter().map(|i| i.line).collect();
    by_line.sort_unstable();
    by_line.dedup();
    for n in by_line.into_iter().rev() {
        let stamps: Vec<String> = issues.iter().filter(|i| i.line == n).map(|i| format!("<!-- openwiki: broken internal link [{}] {}. Fix the href or restore the target, then delete this comment. -->", i.href, i.message)).collect();
        let at = (n - 1).min(lines.len());
        lines.splice(at..at, stamps);
    }
    lines.join("\n")
}

/// `validateWikiInternalLinks`: each page's broken internal links stamped
/// above their line, old stamps removed.
pub fn validate_links(b: &Backend) -> R<()> {
    for source in markdown_files(b, ROOT) {
        let original = read_text(b, &source)?;
        let cleaned = split_lines(&original).into_iter().filter(|l| !STAMP.test(l)).collect::<Vec<_>>().join("\n");
        let own = anchors(&cleaned);
        let mut issues = Vec::new();
        for (href, line) in links_of(&cleaned) {
            if let Some(i) = check_link(b, &source, &href, line, &own)? {
                issues.push(i);
            }
        }
        let stamped = stamp(&cleaned, &issues);
        if stamped == original {
            continue;
        }
        b.edit(&source, &original, &stamped).map_err(|e| format!("Unable to rewrite {source}: {e}"))?;
    }
    Ok(())
}

// ---- Claim sources ----

fn whole_file(resource: &str) -> R<String> {
    let r = parse_resource(resource).map_err(|e| crate::claims::message(&e))?;
    format_resource(&Resource { path: r.path, range: None }).map_err(|e| crate::claims::message(&e))
}

fn source_id(resource: &str) -> String {
    format!("openwiki-source-{}", &crate::store::hex(&Sha256::digest(resource.as_bytes()))[..24])
}

fn source_entries(content: &str) -> Vec<Value> {
    match okf::fields(content).and_then(|f| f.get("sources").cloned()) {
        Some(Value::Array(a)) => a.into_iter().filter(|e| e.is_object() && e.get("resource").and_then(Value::as_str).is_some_and(|r| !jsre::trim(r).is_empty())).collect(),
        _ => Vec::new(),
    }
}

/// `synchronizeClaimSources`: each page's evidence files as its sources
/// (OpenWiki's own entries replaced, others kept).
pub fn sync_sources(b: &Backend, by_page: &[(String, Vec<String>)]) -> R<()> {
    let concepts: HashSet<String> = concept_paths(b)?.into_iter().collect();
    let mut pages: Vec<&(String, Vec<String>)> = by_page.iter().collect();
    pages.sort_by(|a, b| crate::collate::compare(&a.0, &b.0));
    for (page, resources) in pages {
        if !concepts.contains(page) {
            continue;
        }
        let content = match b.read_raw(page) {
            Ok(c) => c,
            Err(ReadError::Error(e)) => return Err(format!("Unable to read {page} while synchronizing OKF sources: {e}")),
            Err(ReadError::Thrown(e)) => return Err(e),
        };
        let repaired = okf::repair(&content, page, "Reference").1;
        let current = source_entries(&repaired);
        let retained: Vec<Value> = current.iter().filter(|e| !e.get("id").and_then(Value::as_str).is_some_and(|i| i.starts_with("openwiki-source-"))).cloned().collect();
        let kept: HashSet<&str> = retained.iter().filter_map(|e| e.get("resource").and_then(Value::as_str)).collect();
        let mut whole: Vec<String> = Vec::new();
        for r in resources {
            let w = whole_file(r)?;
            if !whole.contains(&w) {
                whole.push(w);
            }
        }
        whole.sort_by(|a, b| crate::collate::compare(a, b));
        let mut next = retained.clone();
        next.extend(whole.iter().filter(|r| !kept.contains(r.as_str())).map(|r| json!({"id": source_id(r), "resource": r})));
        let projected = if current == next { repaired } else { okf::repair(&okf::set_sources(&repaired, &next), page, "Reference").1 };
        if projected == content {
            continue;
        }
        b.write(page, &projected).map_err(|e| format!("Unable to synchronize OKF sources for {page}: {e}"))?;
    }
    Ok(())
}

// ---- Generated provenance ----

#[derive(Clone, Debug, PartialEq)]
pub struct Generated {
    pub by: String,
    pub at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub page: String,
    pub body_hash: String,
    pub generated: Option<Generated>,
}

fn body_hash(content: &str) -> String {
    crate::store::hex(&Sha256::digest(okf::split(content).1.as_bytes()))
}

fn generated_of(content: &str) -> Option<Generated> {
    let f = okf::fields(content)?;
    let g = f.get("generated")?.as_object()?;
    let by = g.get("by")?.as_str().filter(|b| !jsre::trim(b).is_empty())?;
    let at = match g.get("at") {
        None => None,
        Some(Value::String(s)) if !jsre::trim(s).is_empty() => Some(s.clone()),
        Some(_) => return None,
    };
    Some(Generated { by: by.to_owned(), at })
}

/// `snapshotGeneratedProvenance`.
pub fn snapshot_provenance(b: &Backend) -> R<Vec<Snapshot>> {
    let mut out = Vec::new();
    for page in concept_paths(b)? {
        let content = match b.read_raw(&page) {
            Ok(c) => c,
            Err(ReadError::Error(e)) => return Err(format!("Unable to read concept {page}: {e}")),
            Err(ReadError::Thrown(e)) => return Err(e),
        };
        out.push(Snapshot { body_hash: body_hash(&content), generated: generated_of(&content), page });
    }
    Ok(out)
}

/// `serializeGeneratedProvenance`.
pub fn serialize_provenance(s: &[Snapshot]) -> Value {
    let mut v: Vec<&Snapshot> = s.iter().collect();
    v.sort_by(|a, b| crate::util::cmp16(&a.page, &b.page));
    // (A Map keeps one entry per page: the last.)
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for e in v.into_iter().rev() {
        if seen.insert(e.page.clone()) {
            out.push(e);
        }
    }
    out.reverse();
    Value::Array(
        out.into_iter()
            .map(|e| {
                let mut o = json!({"page": e.page, "bodyHash": e.body_hash});
                if let Some(g) = &e.generated {
                    let mut gv = json!({"by": g.by});
                    if let Some(at) = g.at.as_ref().filter(|a| !a.is_empty()) {
                        gv["at"] = at.clone().into();
                    }
                    o["generated"] = gv;
                }
                o
            })
            .collect(),
    )
}

/// `deserializeGeneratedProvenance`.
pub fn deserialize_provenance(v: &Value) -> Vec<Snapshot> {
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|e| Snapshot {
                    page: e["page"].as_str().unwrap_or_default().to_owned(),
                    body_hash: e["bodyHash"].as_str().unwrap_or_default().to_owned(),
                    generated: e.get("generated").filter(|g| g.is_object()).map(|g| Generated { by: g["by"].as_str().unwrap_or_default().to_owned(), at: g.get("at").and_then(Value::as_str).map(str::to_owned) }),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `finalizeGeneratedProvenance`: pages whose body changed stamped as
/// generated now, the others' event put back as it was.
pub fn finalize_provenance(b: &Backend, initial: &[Snapshot], now: &str, actor: &str, actors: &HashMap<String, String>) -> R<()> {
    if jsre::trim(actor).is_empty() {
        return Err("Generated provenance requires a non-empty producer actor.".into());
    }
    for (page, a) in actors {
        if jsre::trim(a).is_empty() {
            return Err(format!("Generated provenance requires a non-empty producer actor for {page}."));
        }
    }
    for page in concept_paths(b)? {
        let Ok(content) = b.read_raw(&page) else { continue };
        let init = initial.iter().rev().find(|s| s.page == page);
        let changed = init.is_none_or(|i| i.body_hash != body_hash(&content));
        let candidate = if changed {
            let a = actors.get(&page).map(String::as_str).unwrap_or(actor);
            let c = okf::remove_field(&okf::set_generated(&content, a, Some(now)), "timestamp");
            format!("{}\n", c.trim_end_matches(['\r', '\n']))
        } else {
            match init.and_then(|i| i.generated.as_ref()) {
                Some(g) if generated_of(&content).as_ref() == Some(g) => content.clone(),
                Some(g) => okf::set_generated(&content, &g.by, g.at.as_deref()),
                None => okf::remove_field(&content, "generated"),
            }
        };
        let reconciled = okf::repair(&candidate, &page, "Reference").1;
        if reconciled != content {
            let _ = b.write(&page, &reconciled);
        }
    }
    Ok(())
}

// ---- All of it ----

/// `prepareWikiForAuthoring`: front matter repaired, then each page's body
/// and generated event remembered.
pub fn prepare(b: &Backend, concept_type: &str) -> std::result::Result<Vec<Snapshot>, (&'static str, String)> {
    migrate(b, concept_type).map_err(|e| ("migrate", e))?;
    snapshot_provenance(b).map_err(|e| ("provenance_snapshot", e))
}

/// `finalizeWikiArtifacts`; an error names the step that failed.
pub fn finalize(b: &Backend, language: Option<&str>, prepared: &[Snapshot], at: &str, actor: &str, actors: &HashMap<String, String>, sources: Option<&[(String, Vec<String>)]>) -> std::result::Result<(), (&'static str, String)> {
    if jsre::trim(actor).is_empty() {
        return Err(("", "Wiki finalization requires a non-empty producer actor.".into()));
    }
    validate_mermaid(b).map_err(|e| ("mermaid", e))?;
    sync_indexes(b, index_labels(language), concept_type(language)).map_err(|e| ("index_sync", e))?;
    validate_links(b).map_err(|e| ("link_validation", e))?;
    if let Some(s) = sources {
        sync_sources(b, s).map_err(|e| ("claims_sources", e))?;
    }
    finalize_provenance(b, prepared, at, actor, actors).map_err(|e| ("generated_provenance", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mermaid() {
        let md = "x\n```mermaid\nflowchart TD\n  a[x; y] --> b\n```\n```js\n```mermaid\n```\n";
        let (out, n) = degrade_mermaid(md);
        assert_eq!(n, 1);
        assert!(out.contains("```text\nflowchart TD\n  a[x; y] --> b\n```\n```js"));
        assert_eq!(heuristic_error("graph LR\n a --> end"), Some("Heuristic: `end` is a reserved word and cannot be a flowchart node id; rename the node."));
        assert_eq!(heuristic_error("graph LR\n a[x<y] --> b"), Some("Heuristic: an unescaped angle bracket inside a label breaks rendering; rephrase the label."));
        assert_eq!(heuristic_error("graph LR\n a --> b"), None);
    }

    #[test]
    fn slugs_and_links() {
        assert_eq!(slug("Hello, World! 2"), "hello-world-2");
        let a = anchors("# A\n## A\n```\n# B\n```\n# Ünï code");
        assert!(a.contains("a") && a.contains("a-1") && !a.contains("b") && a.contains("ünï-code"));
        assert_eq!(links_of("see [a](x.md) `[b](y.md)` ![i](p.png)\n```\n[c](z.md)\n```"), vec![("x.md".to_owned(), 1)]);
        assert_eq!(index_labels(Some("pt-BR")), ("Arquivos", "Diretórios"));
        assert_eq!(index_labels(Some("pt-PT")), ("Ficheiros", "Diretórios"));
        assert_eq!(concept_type(Some("de-AT")), "Referenz");
        assert_eq!(concept_type(Some("bg")), "Reference");
    }
}
