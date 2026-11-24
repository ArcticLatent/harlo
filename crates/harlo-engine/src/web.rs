//! The web tools: `webfetch` reads a page or file by its URL, and
//! `websearch` asks a search service the person has a key for.
//!
//! A page comes back as Markdown (its main part, without scripts, menus
//! and footers), text and JSON as they are, an image as a picture. A long
//! page is saved to a file and shows its opening and its outline, so the
//! parts wanted can be read from the file.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use futures_util::StreamExt as _;
use markup5ever_rcdom::{Node, NodeData};
use serde_json::{Map, Value, json};

use crate::tools::Output;

/// What's shown of a page at most.
const SHOWN_BYTES: usize = 50 * 1024;

/// What's taken of a response at most.
const MAX_BODY: usize = 10 * 1024 * 1024;

/// A page longer than this shows its outline instead.
const OUTLINE_ABOVE: usize = 16 * 1024;

/// How much of a long page's opening comes with its outline.
const OPENING_BYTES: usize = 2 * 1024;

/// How long a request may take.
const TIMEOUT: Duration = Duration::from_secs(30);

/// The search services: ID, name and the variable that holds the key.
pub const SERVICES: &[(&str, &str, &str)] = &[
    ("exa", "Exa", "EXA_API_KEY"),
    ("firecrawl", "Firecrawl", "FIRECRAWL_API_KEY"),
    ("parallel", "Parallel", "PARALLEL_API_KEY"),
    ("tavily", "Tavily", "TAVILY_API_KEY"),
];

/// A search service and its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Search {
    /// The service's ID, one of [`SERVICES`].
    pub service: String,
    /// Its key.
    pub key: String,
    /// Another address to send searches to, instead of the service's own
    /// (a stand-in, for tests).
    pub base: Option<String>,
}

/// The tools' definitions; `websearch` only when a service is set up.
#[must_use]
pub fn definitions(search: bool) -> Vec<Value> {
    let mut out = vec![json!({
        "name": "webfetch",
        "description": "Fetches a web page or file by its URL. A page comes back as Markdown; a \
                        long one shows its opening and outline and is saved to a file to read \
                        in parts.",
        "parameters": { "type": "object", "properties": {
            "url": { "type": "string", "description": "The address, http or https." },
        }, "required": ["url"] },
    })];
    if search {
        out.push(json!({
            "name": "websearch",
            "description": "Searches the web. Results come back with their links and a part of \
                            each page; fetch one to read all of it.",
            "parameters": { "type": "object", "properties": {
                "query": { "type": "string", "description": "What to find, in words." },
            }, "required": ["query"] },
        }));
    }
    out
}

/// The HTTP client the tools share.
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("harlo/", env!("CARGO_PKG_VERSION")))
        .timeout(TIMEOUT)
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .unwrap_or_default()
}

/// `url` as one to fetch: http or https; a bare host gets https.
fn parse(url: &str) -> Result<reqwest::Url, String> {
    let url = url.trim();
    let whole = if url.contains("://") {
        url.to_owned()
    } else {
        format!("https://{url}")
    };
    let parsed = reqwest::Url::parse(&whole).map_err(|e| format!("{url} isn't an address: {e}"))?;
    if matches!(parsed.scheme(), "http" | "https") {
        Ok(parsed)
    } else {
        Err(format!(
            "Only http and https addresses can be fetched, not {url}."
        ))
    }
}

/// A size, in KB or MB.
#[expect(clippy::cast_precision_loss, reason = "a size shown to one decimal")]
fn size(n: usize) -> String {
    if n >= 1024 * 1024 {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KB", n.div_ceil(1024))
    }
}

/// Whether type `mime` is text.
fn is_text(mime: &str) -> bool {
    mime.starts_with("text/")
        || ["json", "xml", "javascript", "yaml", "toml", "csv", "x-sh"]
            .iter()
            .any(|t| mime.contains(t))
}

/// Text output with no metadata.
fn output(text: String) -> Output {
    Output {
        text,
        metadata: Map::new(),
        changed: Vec::new(),
        pictures: Vec::new(),
    }
}

/// Fetches `url`; long pages are saved under `spill`.
///
/// # Errors
///
/// When the address is wrong, can't be reached, or answers with an error.
pub async fn fetch(url: &str, spill: &Path) -> Result<Output, String> {
    let url = parse(url)?;
    let response = client()
        .get(url.clone())
        .header(
            "Accept",
            "text/markdown, text/html;q=0.9, text/plain;q=0.8, application/json;q=0.8, */*;q=0.5",
        )
        .send()
        .await
        .map_err(|e| format!("Can't fetch {url}: {e}"))?;
    let status = response.status();
    let at = response.url().clone();
    let mime = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    let mut cut = false;
    while let Some(chunk) = stream.next().await {
        body.extend_from_slice(&chunk.map_err(|e| format!("Fetching {at} broke off: {e}"))?);
        if body.len() > MAX_BODY {
            body.truncate(MAX_BODY);
            cut = true;
            break;
        }
    }
    if !status.is_success() {
        let start: String = String::from_utf8_lossy(&body).chars().take(1500).collect();
        return Err(format!("{at} answered {status}.\n\n{start}")
            .trim_end()
            .to_owned());
    }
    if mime.starts_with("image/") && mime != "image/svg+xml" {
        let mut out = output(format!("[Image: {at}, {}.]", size(body.len())));
        out.pictures.push(crate::attachment::File::new(
            &body,
            &mime,
            json!({ "type": "uri", "uri": at.as_str() }),
            None,
        ));
        return Ok(out);
    }
    let html = mime.contains("html")
        || (mime.is_empty()
            && String::from_utf8_lossy(&body[..body.len().min(512)])
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("<!doctype html"));
    if !html && !is_text(&mime) && body[..body.len().min(8192)].contains(&0) {
        return Ok(output(format!(
            "[{at} is {} ({}), not text, and isn't shown.]",
            if mime.is_empty() {
                "a binary file"
            } else {
                &mime
            },
            size(body.len())
        )));
    }
    let raw = String::from_utf8_lossy(&body).into_owned();
    let (title, text) = if html {
        let base = at.clone();
        tokio::task::spawn_blocking(move || markdown(&raw, &base))
            .await
            .map_err(|e| format!("{at}'s page couldn't be read: {e}"))?
    } else {
        (None, raw)
    };
    Ok(output(shown(
        &at,
        title.as_deref(),
        text.trim(),
        cut,
        spill,
    )))
}

/// What the model sees of page `text` from `at`.
fn shown(at: &reqwest::Url, title: Option<&str>, text: &str, cut: bool, spill: &Path) -> String {
    let head = title.map_or_else(|| format!("[{at}]"), |t| format!("[{at}: {t}]"));
    if text.is_empty() {
        return format!("{head}\n\n(Nothing to read: the page may need a browser to show it.)");
    }
    let cut_note = if cut {
        format!(" It was cut at {}.", size(MAX_BODY))
    } else {
        String::new()
    };
    if text.len() <= OUTLINE_ABOVE {
        return format!("{head}\n\n{text}{cut_note}");
    }
    let lines = text.lines().count();
    let saved = save(text, spill, at);
    let outline = outline(text);
    match saved {
        Some(path) if !outline.is_empty() => format!(
            "{head}\n\n{}\n\n[The page is {} ({lines} lines), more than is worth showing whole.\
             {cut_note} All of it is in {}. Its outline follows (line: heading): read the parts \
             you need with offset and limit, or grep it.]\n{outline}",
            opening(text),
            size(text.len()),
            path.display()
        ),
        saved => {
            let start: String = text.chars().take(SHOWN_BYTES).collect();
            let rest = saved.map_or_else(
                || "The rest couldn't be saved.".to_owned(),
                |p| format!("All of it is in {}.", p.display()),
            );
            format!(
                "{head}\n\n{start}\n\n[The page is {} ({lines} lines); this is its start.{cut_note} {rest}]",
                size(text.len())
            )
        }
    }
}

/// Saves page `text` from `at` under `spill`; where it went.
fn save(text: &str, spill: &Path, at: &reqwest::Url) -> Option<PathBuf> {
    std::fs::create_dir_all(spill).ok()?;
    let name: String = at
        .path_segments()
        .and_then(|mut s| s.rfind(|p| !p.is_empty()))
        .unwrap_or("page")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(60)
        .collect();
    let path = spill.join(format!("{}-{name}.md", crate::id::ascending_id("web")));
    std::fs::write(&path, text).ok()?;
    Some(path)
}

/// A Markdown text's headings, with their line numbers.
fn outline(text: &str) -> String {
    let mut fenced = false;
    text.lines()
        .enumerate()
        .filter_map(|(i, line)| {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
            }
            (!fenced && line.starts_with('#')).then(|| format!("{}: {line}", i + 1))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A long page's opening: from its title, up to about 2 KB, stopping at
/// the next heading.
fn opening(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .take(200)
        .position(|l| l.starts_with("# "))
        .unwrap_or(0);
    let mut out = String::new();
    let mut headings = 0;
    for line in &lines[start..] {
        if line.starts_with('#') {
            headings += 1;
            if headings > 1 && !out.trim().is_empty() {
                break;
            }
        }
        if out.len() + line.len() + 1 > OPENING_BYTES {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    if out
        .lines()
        .filter(|l| l.trim_start().starts_with("```"))
        .count()
        % 2
        == 1
    {
        out.push_str("```\n");
    }
    out.trim_end().to_owned()
}

/// Page `html` as Markdown: its title, and its main part, with links made
/// whole against `base`.
fn markdown(html: &str, base: &reqwest::Url) -> (Option<String>, String) {
    let options = htmd::options::Options {
        bullet_list_marker: htmd::options::BulletListMarker::Dash,
        ..Default::default()
    };
    let converter = htmd::HtmlToMarkdown::builder()
        .options(options)
        .skip_tags(vec![
            "script", "style", "noscript", "template", "nav", "footer", "aside", "svg", "canvas",
            "form", "button", "select", "iframe", "dialog", "head",
        ])
        .build();
    let Ok(document) = converter.html_to_tree(html) else {
        return (None, String::new());
    };
    let title = find(&document, "title")
        .map(|t| text_of(&t).split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty());
    let main = find(&document, "main")
        .or_else(|| find(&document, "article"))
        .filter(|m| text_of(m).trim().len() >= 200);
    let text = converter.tree_to_markdown(main.as_ref().unwrap_or(&document));
    (title, links(&tidy(&text), base))
}

/// The first element named `name`, depth first.
fn find(node: &Rc<Node>, name: &str) -> Option<Rc<Node>> {
    if let NodeData::Element { name: n, .. } = &node.data
        && &*n.local == name
    {
        return Some(Rc::clone(node));
    }
    node.children.borrow().iter().find_map(|c| find(c, name))
}

/// The text inside `node`, without scripts and styles.
fn text_of(node: &Rc<Node>) -> String {
    fn walk(node: &Rc<Node>, out: &mut String) {
        match &node.data {
            NodeData::Text { contents } => out.push_str(&contents.borrow()),
            NodeData::Element { name, .. }
                if matches!(&*name.local, "script" | "style" | "noscript" | "template") =>
            {
                return;
            }
            _ => {}
        }
        for child in node.children.borrow().iter() {
            walk(child, out);
        }
    }
    let mut out = String::new();
    walk(node, &mut out);
    out
}

/// No runs of blank lines and no trailing spaces.
fn tidy(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank = 0;
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Relative links made whole against `base`, without their hover titles.
fn links(text: &str, base: &reqwest::Url) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("](") {
        out.push_str(&rest[..i + 2]);
        rest = &rest[i + 2..];
        let Some(end) = rest.find([' ', ')', '\n']) else {
            break;
        };
        let (url, after) = rest.split_at(end);
        let close = after
            .strip_prefix(" \"")
            .and_then(|t| t.find("\")"))
            .map_or(0, |j| j + 3);
        if !after.starts_with(')') && close == 0 {
            continue;
        }
        let whole = if url.is_empty()
            || url.starts_with('#')
            || url.contains("://")
            || url.starts_with("mailto:")
        {
            url.to_owned()
        } else {
            base.join(url)
                .map_or_else(|_| url.to_owned(), |u| u.to_string())
        };
        out.push_str(&whole);
        rest = &after[close..];
    }
    out.push_str(rest);
    out
}

/// One search result.
struct Found {
    url: String,
    title: String,
    content: String,
}

/// Searches `query` with `search`'s service.
///
/// # Errors
///
/// When the service can't be reached or refuses.
pub async fn search(search: &Search, query: &str) -> Result<Output, String> {
    let base = |own: &str| {
        search
            .base
            .as_deref()
            .map_or_else(|| own.to_owned(), |b| b.trim_end_matches('/').to_owned())
    };
    let http = client();
    let (request, list, fields): (_, &str, [&str; 3]) = match search.service.as_str() {
        "exa" => (
            http.post(format!("{}/search", base("https://api.exa.ai")))
                .header("x-api-key", &search.key)
                .json(&json!({ "query": query, "numResults": 8,
                               "contents": { "text": { "maxCharacters": 1500 } } })),
            "/results",
            ["url", "title", "text"],
        ),
        "tavily" => (
            http.post(format!("{}/search", base("https://api.tavily.com")))
                .bearer_auth(&search.key)
                .json(&json!({ "query": query, "max_results": 8 })),
            "/results",
            ["url", "title", "content"],
        ),
        "firecrawl" => (
            http.post(format!("{}/v2/search", base("https://api.firecrawl.dev")))
                .bearer_auth(&search.key)
                .json(&json!({ "query": query, "limit": 8 })),
            "/data/web",
            ["url", "title", "description"],
        ),
        "parallel" => (
            http.post(format!("{}/v1beta/search", base("https://api.parallel.ai")))
                .header("x-api-key", &search.key)
                .json(&json!({ "objective": query, "search_queries": [query], "max_results": 8 })),
            "/results",
            ["url", "title", "excerpts"],
        ),
        other => {
            return Err(format!(
                "The search service {other} isn't one Harlo can use."
            ));
        }
    };
    let response = request
        .send()
        .await
        .map_err(|e| format!("Couldn't reach {}: {e}", search.service))?;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        return Err(format!(
            "{} refused the search ({status}): {body}",
            search.service
        ));
    }
    let found: Vec<Found> = body
        .pointer(list)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|r| Found {
            url: r[fields[0]].as_str().unwrap_or_default().to_owned(),
            title: r[fields[1]].as_str().unwrap_or_default().to_owned(),
            content: match &r[fields[2]] {
                Value::Array(parts) => parts
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" … "),
                other => other.as_str().unwrap_or_default().to_owned(),
            },
        })
        .filter(|f| !f.url.is_empty())
        .collect();
    if found.is_empty() {
        return Ok(output(format!("The search for \"{query}\" found nothing.")));
    }
    let text = found
        .iter()
        .map(|f| {
            let content: String = f.content.chars().take(1500).collect();
            format!("## [{}]({})\n{}", f.title, f.url, content.trim())
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut out = output(text);
    out.metadata
        .insert("provider".to_owned(), json!(search.service));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    #[test]
    fn a_page_reads_as_markdown_with_whole_links() {
        let base = reqwest::Url::parse("https://example.com/docs/a.html").expect("URL");
        let html = "<html><head><title> The  Page </title><script>x()</script></head><body>\
                    <nav>menu</nav><h1>Hello</h1><p>See <a href=\"b.html\" title=\"B\">b</a>.</p>\
                    </body></html>";
        let (title, text) = markdown(html, &base);
        assert_eq!(title.as_deref(), Some("The Page"));
        assert!(text.contains("# Hello"), "{text}");
        assert!(
            text.contains("[b](https://example.com/docs/b.html)"),
            "{text}"
        );
        assert!(!text.contains("menu") && !text.contains("x()"));
    }

    #[test]
    fn a_long_page_shows_its_outline_and_is_saved() {
        let dir = tempfile::tempdir().expect("scratch");
        let at = reqwest::Url::parse("https://example.com/guide").expect("URL");
        let mut text = "# Guide\n\nIntro.\n\n".to_owned();
        for i in 0..1000 {
            let _ = write!(text, "## Part {i}\n\nSome words about part {i}.\n\n");
        }
        let seen = shown(&at, Some("Guide"), &text, false, dir.path());
        assert!(seen.contains("[https://example.com/guide: Guide]"));
        assert!(seen.contains("1: # Guide"));
        assert!(seen.contains("5: ## Part 0"));
        assert!(seen.len() < text.len() / 2);
        let saved = std::fs::read_dir(dir.path()).expect("saved").count();
        assert_eq!(saved, 1);
    }

    #[test]
    fn only_web_addresses_are_fetched() {
        assert_eq!(parse("example.com").expect("https").scheme(), "https");
        parse("file:///etc/passwd").unwrap_err();
    }
}
