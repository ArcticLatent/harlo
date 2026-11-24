//! The web tools: `webfetch` reads a page or file by its URL, and
//! `websearch` asks a search service, with the person's key or without.
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
/// Each also searches without a key, within its free limits.
pub const SERVICES: &[(&str, &str, &str)] = &[
    ("exa", "Exa", "EXA_API_KEY"),
    ("firecrawl", "Firecrawl", "FIRECRAWL_API_KEY"),
    ("parallel", "Parallel", "PARALLEL_API_KEY"),
    ("tavily", "Tavily", "TAVILY_API_KEY"),
    ("tinyfish", "TinyFish", "TINYFISH_API_KEY"),
];

/// A search service, and its key when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Search {
    /// The service's ID, one of [`SERVICES`].
    pub service: String,
    /// Its key; none searches without one.
    pub key: Option<String>,
    /// Another address to send searches to, instead of the service's own
    /// (a stand-in, for tests).
    pub base: Option<String>,
}

/// Why a search failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchFailure {
    /// What went wrong, for the model.
    pub text: String,
    /// When the service limits how often it's asked: how long it wants
    /// left alone.
    pub limited: Option<Duration>,
}

impl SearchFailure {
    /// A failure that isn't a limit.
    fn said(text: String) -> Self {
        Self {
            text,
            limited: None,
        }
    }
}

/// Where a turn's web searches go: the service the person chose, asked
/// for on the first search.
pub trait Searcher: Send + Sync + std::fmt::Debug {
    /// Searches `query`.
    ///
    /// # Errors
    ///
    /// With a message for the model when no service could answer, or the
    /// person turned web search off.
    fn search<'a>(
        &'a self,
        query: &'a str,
    ) -> futures_util::future::BoxFuture<'a, Result<Output, String>>;
}

/// The tools' definitions; `websearch` only when web search isn't off.
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

/// What a search asks for: how many results.
const RESULTS: u64 = 8;

/// `own`, a service's address, sent to `base` instead when there is one.
fn at(own: &str, base: Option<&str>) -> String {
    let Some(base) = base else {
        return own.to_owned();
    };
    let path = own
        .strip_prefix("https://")
        .and_then(|rest| rest.find('/').map(|i| &rest[i..]))
        .unwrap_or("/");
    format!("{}{path}", base.trim_end_matches('/'))
}

/// How long a service that answered 429 wants left alone: its
/// `Retry-After`, in seconds or as a date, else a minute.
fn retry_after(headers: &reqwest::header::HeaderMap) -> Duration {
    const MINUTE: Duration = Duration::from_secs(60);
    let Some(value) = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
    else {
        return MINUTE;
    };
    if let Ok(seconds) = value.parse::<f64>() {
        return if seconds >= 0.0 && seconds.is_finite() {
            Duration::from_secs_f64(seconds)
        } else {
            MINUTE
        };
    }
    httpdate::parse_http_date(value).map_or(MINUTE, |when| {
        when.duration_since(std::time::SystemTime::now())
            .unwrap_or_default()
    })
}

/// Sends `request`, refusing anything but success; a 429 says how long
/// to wait.
async fn send(
    request: reqwest::RequestBuilder,
    service: &str,
) -> Result<reqwest::Response, SearchFailure> {
    let response = request
        .send()
        .await
        .map_err(|e| SearchFailure::said(format!("Couldn't reach {service}: {e}")))?;
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let limited =
        (status == reqwest::StatusCode::TOO_MANY_REQUESTS).then(|| retry_after(response.headers()));
    let body = response.text().await.unwrap_or_default();
    let body: String = body.chars().take(500).collect();
    Err(SearchFailure {
        text: format!("{service} refused the search ({status}): {body}"),
        limited,
    })
}

/// The result of an MCP tool call's response `text`: JSON, or a stream of
/// `data:` lines.
fn rpc_result(text: &str, service: &str) -> Result<Value, SearchFailure> {
    let messages: Vec<Value> = if text.trim_start().starts_with('{') {
        serde_json::from_str(text).into_iter().collect()
    } else {
        text.lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .filter_map(|d| serde_json::from_str(d.trim()).ok())
            .collect()
    };
    for message in messages {
        if let Some(error) = message.get("error") {
            let said = error["message"].as_str().unwrap_or("an error");
            return Err(SearchFailure::said(format!("{service} failed: {said}")));
        }
        if let Some(result) = message.get("result") {
            if result["isError"] == true {
                return Err(SearchFailure::said(format!(
                    "{service} failed: {}",
                    first_text(result)
                )));
            }
            return Ok(result.clone());
        }
    }
    Err(SearchFailure::said(format!(
        "{service} sent no answer to the search."
    )))
}

/// The first text of an MCP tool result.
fn first_text(result: &Value) -> String {
    result["content"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|c| c["text"].as_str().filter(|t| !t.is_empty()))
        .unwrap_or_default()
        .to_owned()
}

/// Calls tool `tool` of the MCP server at `url` with `arguments`.
async fn call_tool(
    request: reqwest::RequestBuilder,
    service: &str,
    tool: &str,
    arguments: Value,
) -> Result<Value, SearchFailure> {
    let request = request
        .header(
            reqwest::header::ACCEPT,
            "application/json, text/event-stream",
        )
        .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                       "params": { "name": tool, "arguments": arguments } }));
    let text = send(request, service)
        .await?
        .text()
        .await
        .map_err(|e| SearchFailure::said(format!("{service}'s answer broke off: {e}")))?;
    rpc_result(&text, service)
}

/// Opens a session with the `TinyFish` MCP server, which a key needs.
async fn tinyfish_session(
    http: &reqwest::Client,
    url: &str,
    key: &str,
) -> Result<String, SearchFailure> {
    let accept = "application/json, text/event-stream";
    let response = send(
        http.post(url)
            .header("X-API-Key", key)
            .header(reqwest::header::ACCEPT, accept)
            .json(
                &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                "protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": { "name": "harlo", "version": env!("CARGO_PKG_VERSION") },
            } }),
            ),
        "tinyfish",
    )
    .await?;
    let session = response
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .ok_or_else(|| SearchFailure::said("TinyFish opened no session.".to_owned()))?;
    send(
        http.post(url)
            .header("X-API-Key", key)
            .header("Mcp-Session-Id", &session)
            .header(reqwest::header::ACCEPT, accept)
            .json(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })),
        "tinyfish",
    )
    .await?;
    Ok(session)
}

/// Exa's results: blocks of `URL:`, `Title:` and `Text:` lines.
fn exa_results(text: &str) -> Vec<Found> {
    let line = |block: &str, name: &str| {
        block
            .lines()
            .find_map(|l| l.strip_prefix(name))
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty() && v != "N/A")
    };
    text.split("\n\n---\n\n")
        .filter_map(|block| {
            let url = line(block, "URL:")?;
            let content = ["Highlights:", "Text:"]
                .iter()
                .find_map(|name| block.find(name).map(|i| &block[i + name.len()..]))
                .unwrap_or_default()
                .trim()
                .to_owned();
            Some(Found {
                url,
                title: line(block, "Title:").unwrap_or_default(),
                content,
            })
        })
        .collect()
}

/// The results in `list`, each with fields `fields` (address, title,
/// text or a list of excerpts).
fn listed(list: Option<&Value>, fields: [&str; 3]) -> Vec<Found> {
    list.and_then(Value::as_array)
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
        .collect()
}

/// The JSON a tool result's text holds.
fn text_json(result: &Value) -> Value {
    serde_json::from_str(&first_text(result)).unwrap_or(Value::Null)
}

/// Asks `search`'s service for `query`.
async fn results(search: &Search, query: &str) -> Result<Vec<Found>, SearchFailure> {
    let http = client();
    let base = search.base.as_deref();
    let key = search.key.as_deref();
    let bearer = |request: reqwest::RequestBuilder| match key {
        Some(k) => request.bearer_auth(k),
        None => request,
    };
    match search.service.as_str() {
        "exa" => {
            let mut url = reqwest::Url::parse(&at("https://mcp.exa.ai/mcp", base))
                .map_err(|e| SearchFailure::said(format!("Exa's address is wrong: {e}")))?;
            if let Some(k) = key {
                url.query_pairs_mut().append_pair("exaApiKey", k);
            }
            let args = json!({ "query": query, "numResults": RESULTS });
            let result = call_tool(http.post(url), "exa", "web_search_exa", args).await?;
            Ok(exa_results(&first_text(&result)))
        }
        "firecrawl" => {
            let url = at("https://mcp.firecrawl.dev/v2/mcp", base);
            let args = json!({ "query": query, "limit": RESULTS });
            let result = call_tool(
                bearer(http.post(url)),
                "firecrawl",
                "firecrawl_search",
                args,
            )
            .await?;
            let data = text_json(&result);
            Ok(listed(
                data.pointer("/data/web"),
                ["url", "title", "description"],
            ))
        }
        "parallel" => {
            let url = at("https://search.parallel.ai/mcp", base);
            let args = json!({ "objective": query, "search_queries": [query] });
            let result = call_tool(bearer(http.post(url)), "parallel", "web_search", args).await?;
            Ok(listed(
                result.pointer("/structuredContent/results"),
                ["url", "title", "excerpts"],
            ))
        }
        "tavily" => {
            let request = http
                .post(at("https://api.tavily.com/search", base))
                .json(&json!({ "query": query, "search_depth": "basic",
                               "chunks_per_source": 3, "max_results": RESULTS }));
            let request = match key {
                Some(k) => request.bearer_auth(k),
                None => request.header("X-Tavily-Access-Mode", "keyless"),
            };
            let body: Value = send(request, "tavily")
                .await?
                .json()
                .await
                .unwrap_or(Value::Null);
            Ok(listed(body.get("results"), ["url", "title", "content"]))
        }
        "tinyfish" => {
            let url = at("https://agent.tinyfish.ai/mcp", base);
            let request = match key {
                Some(k) => {
                    let session = tinyfish_session(&http, &url, k).await?;
                    http.post(&url)
                        .header("X-API-Key", k)
                        .header("Mcp-Session-Id", session)
                }
                None => http.post(&url).header("X-TinyFish-Access-Mode", "keyless"),
            };
            let result =
                call_tool(request, "tinyfish", "search", json!({ "query": query })).await?;
            let data = text_json(&result);
            Ok(listed(data.get("results"), ["url", "title", "snippet"]))
        }
        other => Err(SearchFailure::said(format!(
            "The search service {other} isn't one Harlo can use."
        ))),
    }
}

/// Searches `query` with `search`'s service.
///
/// # Errors
///
/// When the service can't be reached or refuses; a 429 says how long it
/// wants left alone.
pub async fn search(search: &Search, query: &str) -> Result<Output, SearchFailure> {
    let found: Vec<Found> = results(search, query)
        .await?
        .into_iter()
        .filter(|f| !f.url.is_empty())
        .collect();
    let mut out = if found.is_empty() {
        output(format!("The search for \"{query}\" found nothing."))
    } else {
        output(
            found
                .iter()
                .map(|f| {
                    let content: String = f.content.chars().take(1500).collect();
                    let title = if f.title.is_empty() { &f.url } else { &f.title };
                    format!("## [{title}]({})\n{}", f.url, content.trim())
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
        )
    };
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
    fn search_answers_read_as_json_or_streams() {
        let streamed = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\
                        \"result\":{\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}}\n\n";
        let result = rpc_result(streamed, "exa").expect("a result");
        assert_eq!(first_text(&result), "hi");
        let failed = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-1,"message":"no"}}"#;
        assert_eq!(
            rpc_result(failed, "exa").expect_err("an error").text,
            "exa failed: no"
        );

        let found = exa_results(
            "Title: One\nURL: https://a.example\nPublished: N/A\nText: First.\n\n---\n\n\
             Title: N/A\nURL: https://b.example\nHighlights:\nSecond.",
        );
        assert_eq!(found.len(), 2);
        assert_eq!(
            (found[0].title.as_str(), found[0].content.as_str()),
            ("One", "First.")
        );
        assert_eq!(
            (found[1].title.as_str(), found[1].content.as_str()),
            ("", "Second.")
        );
    }

    #[test]
    fn stand_ins_take_the_services_paths() {
        assert_eq!(
            at("https://mcp.firecrawl.dev/v2/mcp", None),
            "https://mcp.firecrawl.dev/v2/mcp"
        );
        assert_eq!(
            at(
                "https://mcp.firecrawl.dev/v2/mcp",
                Some("http://127.0.0.1:9/")
            ),
            "http://127.0.0.1:9/v2/mcp"
        );
        let mut headers = reqwest::header::HeaderMap::new();
        assert_eq!(retry_after(&headers), Duration::from_secs(60));
        headers.insert(reqwest::header::RETRY_AFTER, "7".parse().expect("header"));
        assert_eq!(retry_after(&headers), Duration::from_secs(7));
        headers.insert(
            reqwest::header::RETRY_AFTER,
            "Wed, 21 Oct 2015 07:28:00 GMT".parse().expect("header"),
        );
        assert_eq!(retry_after(&headers), Duration::ZERO, "a date gone by");
    }

    #[test]
    fn only_web_addresses_are_fetched() {
        assert_eq!(parse("example.com").expect("https").scheme(), "https");
        parse("file:///etc/passwd").unwrap_err();
    }
}
