//! `fetch`: a web page or file by its URL. A page comes back as Markdown
//! (its main part: `<main>` or `<article>` when it has one, without
//! scripts, menus and footers), text and JSON as they are, an image as an
//! image; another kind of file is saved for a command to open. A long page
//! is saved to a file and shows its opening and its outline (its headings,
//! with their lines), to read the parts wanted
//! (`HARLO_AGENT_PAGE_OUTLINE=off`: its first 50 KB instead); without
//! headings to outline, it shows its start. Measured on three questions
//! about long documentation pages: the same answers with 77% fewer input
//! tokens (40k against 174k, two runs each).

use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use futures_util::StreamExt;
use markup5ever_rcdom::NodeData;

use crate::tools::Outcome;

/// What's shown at most (as `read`), and what's taken at most.
const SHOWN_BYTES: usize = 50 * 1024;
const SHOWN_LINES: usize = 2000;
const MAX_BODY: usize = 10 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);
/// A page longer than this shows its outline instead (about 4,000 tokens).
const OUTLINE_ABOVE: usize = 16 * 1024;
/// How much of a long page's opening comes with its outline.
const OPENING_BYTES: usize = 2 * 1024;

fn page_outlines() -> bool {
    std::env::var("HARLO_AGENT_PAGE_OUTLINE").map_or(true, |v| v.trim() != "off")
}

fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("harlo-agent/", env!("CARGO_PKG_VERSION")))
            .timeout(TIMEOUT)
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .unwrap_or_default()
    })
}

/// Whether fetching is possible here (the sandbox may be set to no
/// network).
pub fn possible() -> bool {
    let p = crate::sandbox::policy();
    !p.on || p.network
}

/// Fetches `url` (`spill_dir`: where long pages and other files go).
pub async fn run(url: &str, spill_dir: &Path) -> Outcome {
    let fail = |t: String| Outcome::new(t, true);
    if !possible() {
        return fail("The sandbox is set to no network here, so nothing can be fetched.".into());
    }
    let url = match parse(url) {
        Ok(u) => u,
        Err(why) => return fail(why),
    };
    let res = client()
        .get(url.clone())
        .header("Accept", "text/markdown, text/html;q=0.9, text/plain;q=0.8, application/json;q=0.8, */*;q=0.5")
        .send()
        .await;
    let res = match res {
        Ok(r) => r,
        Err(e) if e.is_timeout() => return fail(format!("{url} didn't answer within {} s.", TIMEOUT.as_secs())),
        Err(e) => return fail(format!("Can't fetch {url}: {}", reason(&e))),
    };
    let status = res.status();
    let at = res.url().clone();
    let kind = res.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or_default().to_ascii_lowercase();
    let mut body = Vec::new();
    let mut cut = false;
    let mut stream = res.bytes_stream();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(c) => {
                body.extend_from_slice(&c);
                if body.len() > MAX_BODY {
                    body.truncate(MAX_BODY);
                    cut = true;
                    break;
                }
            }
            Err(e) => return fail(format!("Fetching {at} broke off: {}", reason(&e))),
        }
    }
    let moved = if at != url { format!(" (from {url})") } else { String::new() };
    let mime = kind.split(';').next().unwrap_or_default().trim().to_owned();
    if (mime.starts_with("image/") && mime != "image/svg+xml") || (mime.is_empty() && crate::images::mime_of(&body).is_some()) {
        return match crate::images::fit(&body) {
            Ok(f) if status.is_success() => {
                let mut out = Outcome::new(format!("[Image: {at}{moved}, {}.]", f.describe()), false);
                out.images.push(f.block());
                out
            }
            Ok(_) => fail(format!("{at} answered {status}.")),
            Err(why) => fail(format!("{at} is an image, but it can't be shown: {why}.")),
        };
    }
    let html = mime.contains("html") || (mime.is_empty() && looks_like_html(&body));
    let texty = html || is_text(&mime) || (mime.is_empty() && !body[..body.len().min(8192)].contains(&0));
    if !texty {
        if !status.is_success() {
            return fail(format!("{at} answered {status}."));
        }
        let saved = save(&body, spill_dir, &file_name(&at));
        return Outcome::new(
            match saved {
                Some(p) => format!("[{at}{moved} is {} ({}), not text: saved to {p}. Open it with a command.]", if mime.is_empty() { "a binary file" } else { &mime }, size(body.len())),
                None => format!("[{at}{moved} is {} ({}), not text, and it couldn't be saved.]", if mime.is_empty() { "a binary file" } else { &mime }, size(body.len())),
            },
            false,
        );
    }
    let raw = String::from_utf8_lossy(&body).into_owned();
    let (title, text) = if html {
        let base = at.clone();
        match tokio::task::spawn_blocking(move || markdown(&raw, &base)).await {
            Ok(r) => r,
            Err(_) => return fail(format!("{at}'s page couldn't be read.")),
        }
    } else {
        (None, raw)
    };
    let text = text.trim().to_owned();
    if !status.is_success() {
        let start: String = text.chars().take(1500).collect();
        return fail(format!("{at} answered {status}.{}", if start.is_empty() { String::new() } else { format!("\n\n{start}") }));
    }
    let head = match &title {
        Some(t) => format!("[{at}{moved}: {t}]"),
        None => format!("[{at}{moved}]"),
    };
    if text.is_empty() {
        return Outcome::new(format!("{head}\n\n(nothing to read: the page may need a browser to show it)"), false);
    }
    let cut_note = if cut { format!(" It was cut at {}.", size(MAX_BODY)) } else { String::new() };
    let lines = text.lines().count();
    if text.len() > OUTLINE_ABOVE && page_outlines()
        && let Some(outline) = crate::outline::of(Path::new("page.md"), &text)
        && let Some(p) = save(text.as_bytes(), spill_dir, &format!("{}.md", file_name(&at).trim_end_matches(".html")))
    {
        let opening = opening_of(&text);
        return Outcome::new(
            format!(
                "{head}\n\n{opening}\n\n[The page is {} ({lines} lines), more than is worth showing whole.{cut_note} All of it is in {p}. Its outline follows (line: heading): read the parts you need with offset and limit, or grep it.]\n{outline}",
                size(text.len())
            ),
            false,
        );
    }
    if text.len() <= SHOWN_BYTES && lines <= SHOWN_LINES {
        return Outcome::new(format!("{head}\n\n{text}{}", if cut { format!("\n\n[{}]", cut_note.trim()) } else { String::new() }), false);
    }
    let shown = start_of(&text);
    let n = shown.lines().count();
    let saved = save(text.as_bytes(), spill_dir, &format!("{}.md", file_name(&at).trim_end_matches(".html")));
    let rest = match saved {
        Some(p) => format!("All of it is in {p}: read it on with offset={}, or grep it.", n + 1),
        None => "The rest couldn't be saved.".to_owned(),
    };
    Outcome::new(format!("{head}\n\n{shown}\n\n[The page is {} ({lines} lines); these are its first {n} lines.{cut_note} {rest}]", size(text.len())), false)
}

/// `url` as one to fetch: http or https (a bare host gets https).
fn parse(url: &str) -> Result<reqwest::Url, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("Give the URL.".into());
    }
    let full = if url.contains("://") { url.to_owned() } else { format!("https://{url}") };
    let u = reqwest::Url::parse(&full).map_err(|e| format!("{url} isn't a URL ({e})."))?;
    match u.scheme() {
        "http" | "https" => Ok(u),
        other => Err(format!("Only http and https can be fetched, not {other}: for a file, use read.")),
    }
}

fn reason(e: &reqwest::Error) -> String {
    let mut s = e.to_string();
    let mut src = std::error::Error::source(e);
    while let Some(inner) = src {
        s = format!("{s}: {inner}");
        src = inner.source();
    }
    s
}

fn is_text(mime: &str) -> bool {
    mime.starts_with("text/")
        || ["json", "xml", "svg", "javascript", "ecmascript", "yaml", "toml", "x-sh", "x-python", "csv", "graphql", "sql"].iter().any(|t| mime.contains(t))
}

fn looks_like_html(body: &[u8]) -> bool {
    let start = String::from_utf8_lossy(&body[..body.len().min(512)]).trim_start().to_ascii_lowercase();
    start.starts_with("<!doctype html") || start.starts_with("<html")
}

/// A page as Markdown: its title, and its main part (`base`: where it is,
/// for its links).
fn markdown(html: &str, base: &reqwest::Url) -> (Option<String>, String) {
    let opts = htmd::options::Options {
        bullet_list_marker: htmd::options::BulletListMarker::Dash,
        ul_bullet_spacing: 1,
        ol_number_spacing: 1,
        ..Default::default()
    };
    let conv = htmd::HtmlToMarkdown::builder()
        .options(opts)
        .skip_tags(vec!["script", "style", "noscript", "template", "nav", "footer", "aside", "svg", "canvas", "form", "button", "select", "iframe", "dialog", "head"])
        .build();
    let Ok(doc) = conv.html_to_tree(html) else { return (None, String::new()) };
    let title = find(&doc, "title").map(|t| text_of(&t)).map(|t| t.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|t| !t.is_empty());
    let main = find(&doc, "main").or_else(|| find(&doc, "article")).filter(|m| text_of(m).trim().len() >= 200);
    let md = conv.tree_to_markdown(main.as_ref().unwrap_or(&doc));
    (title, links(&tidy(&md), base))
}

/// Links as ones to fetch: relative ones made whole, without their hover
/// titles (`[a](u "title")` → `[a](u)`); a link within the page is kept as
/// it is.
fn links(md: &str, base: &reqwest::Url) -> String {
    let mut out = String::with_capacity(md.len());
    let mut rest = md;
    while let Some(i) = rest.find("](") {
        out.push_str(&rest[..i + 2]);
        rest = &rest[i + 2..];
        let Some(end) = rest.find([' ', ')', '\n']) else { break };
        let (url, after) = rest.split_at(end);
        let close = after.strip_prefix(" \"").and_then(|t| t.find("\")")).map_or(0, |j| 2 + j + 1);
        if !after.starts_with(')') && close == 0 {
            continue;
        }
        let whole = if url.is_empty() || url.starts_with('#') || url.contains("://") || url.starts_with("mailto:") {
            url.to_owned()
        } else {
            match base.join(url) {
                // (This page's own part: just its anchor.)
                Ok(u) if u.fragment().is_some() && { let (mut a, mut b) = (u.clone(), base.clone()); a.set_fragment(None); b.set_fragment(None); a == b } => format!("#{}", u.fragment().unwrap_or_default()),
                Ok(u) => u.to_string(),
                Err(_) => url.to_owned(),
            }
        };
        out.push_str(&whole);
        rest = &after[close..];
    }
    out.push_str(rest);
    out
}

/// The first element named `name`, depth first.
fn find(node: &Rc<markup5ever_rcdom::Node>, name: &str) -> Option<Rc<markup5ever_rcdom::Node>> {
    if let NodeData::Element { name: n, .. } = &node.data
        && &*n.local == name
    {
        return Some(node.clone());
    }
    node.children.borrow().iter().find_map(|c| find(c, name))
}

fn text_of(node: &Rc<markup5ever_rcdom::Node>) -> String {
    let mut out = String::new();
    fn walk(n: &Rc<markup5ever_rcdom::Node>, out: &mut String) {
        if let NodeData::Text { contents } = &n.data {
            out.push_str(&contents.borrow());
        }
        if let NodeData::Element { name, .. } = &n.data
            && matches!(&*name.local, "script" | "style" | "noscript" | "template")
        {
            return;
        }
        for c in n.children.borrow().iter() {
            walk(c, out);
        }
    }
    walk(node, &mut out);
    out
}

/// No runs of blank lines, no trailing spaces (but a Markdown line break's).
fn tidy(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut blank = 0;
    for l in md.lines() {
        let l = if l.ends_with("  ") && !l.trim().is_empty() { l } else { l.trim_end() };
        if l.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(l);
        out.push('\n');
    }
    out
}

/// A long page's opening: from its title (its first `# ` heading, when
/// one comes early: menus can come before it), up to about 2 KB, stopping
/// at the next heading (the outline has the rest).
fn opening_of(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().take(200).position(|l| l.starts_with("# ")).unwrap_or(0);
    let mut out = String::new();
    let mut headings = 0;
    for l in &lines[start..] {
        if l.starts_with('#') {
            headings += 1;
            if headings > 1 && !out.trim().is_empty() {
                break;
            }
        }
        if out.len() + l.len() + 1 > OPENING_BYTES {
            if out.trim().is_empty() {
                out = l.chars().take(OPENING_BYTES).collect();
            }
            break;
        }
        out.push_str(l);
        out.push('\n');
    }
    // (Cut inside a code block: closed, so what follows isn't code.)
    if out.lines().filter(|l| l.trim_start().starts_with("```")).count() % 2 == 1 {
        out.push_str("```\n");
    }
    out.trim_end().to_owned()
}

/// The text's start, as much as is shown (whole lines).
fn start_of(text: &str) -> String {
    let mut out = String::new();
    for (i, l) in text.lines().enumerate() {
        if i >= SHOWN_LINES || out.len() + l.len() + 1 > SHOWN_BYTES {
            break;
        }
        out.push_str(l);
        out.push('\n');
    }
    if out.is_empty() {
        out = text.chars().take(SHOWN_BYTES / 4).collect();
    }
    out.trim_end().to_owned()
}

fn size(n: usize) -> String {
    if n >= 1024 * 1024 { format!("{:.1} MB", n as f64 / (1024.0 * 1024.0)) } else { format!("{} KB", n.div_ceil(1024)) }
}

/// A name for what's fetched from `url` (its last part, made safe).
fn file_name(url: &reqwest::Url) -> String {
    let last = url.path_segments().and_then(|mut s| s.next_back()).filter(|s| !s.is_empty()).unwrap_or("index.html");
    let safe: String = last.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).take(80).collect();
    let safe = safe.trim_start_matches('.').to_owned();
    if safe.is_empty() { "index.html".into() } else { safe }
}

/// Saves `bytes` under `dir/fetched/`, its name kept after a random part.
fn save(bytes: &[u8], dir: &Path, name: &str) -> Option<String> {
    let dir = dir.join("fetched");
    std::fs::create_dir_all(&dir).ok()?;
    let mut b = [0u8; 4];
    getrandom::fill(&mut b).ok()?;
    let tag: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let path = dir.join(format!("{tag}-{name}"));
    std::fs::write(&path, bytes).ok()?;
    Some(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A server on a loopback port that answers each path as `pages` says:
    /// (path, status line, content type, body).
    async fn serve(pages: Vec<(&'static str, &'static str, &'static str, Vec<u8>)>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else { return };
                let pages = pages.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let path = req.split_whitespace().nth(1).unwrap_or("/").to_owned();
                    let (status, kind, body) = pages.iter().find(|p| p.0 == path).map(|p| (p.1, p.2, p.3.clone())).unwrap_or(("404 Not Found", "text/html", b"<html><body><h1>Gone</h1></body></html>".to_vec()));
                    let extra = if status.starts_with("301") { "Location: /docs\r\n" } else { "" };
                    let head = format!("HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n", body.len());
                    let _ = s.write_all(head.as_bytes()).await;
                    let _ = s.write_all(&body).await;
                });
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn pages_come_back_as_markdown() {
        let page = format!(
            "<!doctype html><html><head><title>The  Docs</title><style>p{{}}</style></head><body><nav><a href=/>Home</a> Menu</nav>\
             <main><h1>Install</h1><p>Run <code>make</code> and see <a href=\"https://x.org/a\">the guide</a>.</p><ul><li>one</li><li>two</li></ul>\
             <pre><code class=\"language-sh\">make install\n</code></pre><p>{}</p><script>alert(1)</script></main><footer>© someone</footer></body></html>",
            "More words. ".repeat(30)
        );
        let base = serve(vec![
            ("/docs", "200 OK", "text/html; charset=utf-8", page.into_bytes()),
            ("/old", "301 Moved Permanently", "text/html", Vec::new()),
            ("/a.json", "200 OK", "application/json", b"{\"a\": 1}".to_vec()),
            ("/f.pdf", "200 OK", "application/pdf", b"%PDF-1.4 \x00\x01".to_vec()),
            ("/long", "200 OK", "text/html", format!(
                "<html><head><title>Long</title></head><body><main><h1>Guide</h1><p>What it is, briefly.</p>{}</main></body></html>",
                (1..=40).map(|i| format!("<h2>Part {i}</h2><p>{}</p>", format!("Words about part {i}. ").repeat(30))).collect::<String>()
            ).into_bytes()),
            ("/big.txt", "200 OK", "text/plain", (1..=5000).map(|i| format!("line {i}\n")).collect::<String>().into_bytes()),
        ])
        .await;
        let dir = std::env::temp_dir().join(format!("harlo-agent-fetch-{}", std::process::id()));
        let o = run(&format!("{base}/old"), &dir).await;
        assert!(!o.failed, "{}", o.text);
        assert!(o.text.starts_with(&format!("[{base}/docs (from {base}/old): The Docs]\n\n# Install\n\nRun `make` and see [the guide](https://x.org/a).\n\n- one\n- two\n\n```sh\nmake install\n```")), "{}", o.text);
        assert!(!o.text.contains("Menu") && !o.text.contains("alert") && !o.text.contains("someone"), "{}", o.text);
        assert_eq!(run(&format!("{base}/a.json"), &dir).await.text, format!("[{base}/a.json]\n\n{{\"a\": 1}}"));
        let pdf = run(&format!("{base}/f.pdf"), &dir).await;
        assert!(!pdf.failed && pdf.text.contains("is application/pdf (1 KB), not text: saved to"), "{}", pdf.text);
        let saved = pdf.text.split("saved to ").nth(1).unwrap().split(". Open").next().unwrap();
        assert!(saved.ends_with("-f.pdf") && std::fs::read(saved).unwrap().starts_with(b"%PDF"));
        let big = run(&format!("{base}/big.txt"), &dir).await;
        assert!(big.text.contains("these are its first 2000 lines") && big.text.contains("read it on with offset=2001"), "{}", &big.text[big.text.len() - 300..]);
        let long = run(&format!("{base}/long"), &dir).await;
        assert!(!long.failed && long.text.starts_with(&format!("[{base}/long: Long]\n\n# Guide\n\nWhat it is, briefly.\n\n[The page is ")), "{}", &long.text[..long.text.len().min(400)]);
        assert!(long.text.contains("Its outline follows (line: heading)") && long.text.contains(": ## Part 1") && long.text.contains(": ## Part 40") && !long.text.contains("Words about part"), "{}", long.text);
        let saved = long.text.split("All of it is in ").nth(1).unwrap().split(". Its outline").next().unwrap();
        assert!(std::fs::read_to_string(saved).unwrap().contains("Words about part 40"));
        let gone = run(&format!("{base}/nope"), &dir).await;
        assert!(gone.failed && gone.text.starts_with(&format!("{base}/nope answered 404 Not Found.\n\n# Gone")), "{}", gone.text);
        assert!(run("file:///etc/passwd", &dir).await.text.contains("Only http and https"));
        let base = reqwest::Url::parse("https://d.rs/x/sync/struct.M.html").unwrap();
        assert_eq!(
            links("[sync](index.html)::[M](struct.M.html#m \"method M\") [S](https://x.org/s \"trait S\") [top](#top) ![i](../i.png) a](b c", &base),
            "[sync](https://d.rs/x/sync/index.html)::[M](#m) [S](https://x.org/s) [top](#top) ![i](https://d.rs/x/i.png) a](b c"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
