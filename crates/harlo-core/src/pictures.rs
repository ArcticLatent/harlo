//! Pictures in a conversation: ones that came in a message (an image you
//! pasted, a screenshot a tool returned, one a model made), kept as files
//! in harlo's cache so they can be shown like any other; and the image
//! files a message names (a Markdown image, a path an agent wrote to).

use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use base64::Engine;
use serde_json::Value;

/// File suffixes taken for pictures when a message names a file.
pub const SUFFIXES: [&str; 12] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "avif", "heic", "jxl", "tif", "tiff"];

#[cfg(test)]
thread_local! {
    /// (Tests keep their pictures in a folder of their own.)
    pub static TEST_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Where pictures that came inline are kept (made again from the session
/// when they're gone).
pub fn cache_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(dir) = TEST_DIR.with(|d| d.borrow().clone()) {
        return dir;
    }
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("harlo/pictures")
}

/// An image block's picture as a file in the cache (written the first time):
/// pi's `{data, mimeType}` or Anthropic's `{source: {data, media_type}}`.
pub fn inline_file(block: &Value) -> Option<String> {
    let data = block["data"].as_str().or_else(|| block["source"]["data"].as_str()).filter(|d| !d.is_empty())?;
    let mime = block["mimeType"].as_str().or_else(|| block["source"]["media_type"].as_str()).unwrap_or("image/png");
    let ext = match mime {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        "image/svg+xml" => "svg",
        _ => "png",
    };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    let a = h.finish();
    data.len().hash(&mut h);
    let file = cache_dir().join(format!("{a:016x}{:016x}.{ext}", h.finish()));
    if !file.exists() {
        let bytes = base64::engine::general_purpose::STANDARD.decode(data.trim()).ok()?;
        std::fs::create_dir_all(cache_dir()).ok()?;
        // (Written whole, then named: a half-written file is never shown.)
        let part = file.with_extension("part");
        std::fs::write(&part, bytes).ok()?;
        std::fs::rename(&part, &file).ok()?;
    }
    Some(file.to_string_lossy().into_owned())
}

/// A name ending in a picture's suffix.
pub fn is_picture_name(s: &str) -> bool {
    let name = s.rsplit('/').next().unwrap_or(s);
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => SUFFIXES.iter().any(|x| ext.eq_ignore_ascii_case(x)),
        _ => false,
    }
}

/// The local pictures of Markdown images (`![alt](path)`), in order; web
/// addresses are left out.
pub fn markdown_images(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("![") {
        rest = &rest[at + 2..];
        let Some(close) = rest.find("](") else { break };
        let after = &rest[close + 2..];
        let Some(end) = after.find(')') else { break };
        // (`<path with spaces>`, or a path then an optional "title".)
        let inner = after[..end].trim();
        let src = match inner.strip_prefix('<') {
            Some(s) => s.split('>').next().unwrap_or_default(),
            None => inner.split_whitespace().next().unwrap_or_default(),
        };
        let src = src.strip_prefix("file://").unwrap_or(src);
        if !src.is_empty() && !src.contains("://") && !src.starts_with("data:") && !out.iter().any(|o| o == src) {
            out.push(src.to_owned());
        }
        rest = &after[end..];
    }
    out
}

/// Paths of pictures a text names (`out/cat.png`, `/tmp/a b.png` in
/// backticks), in order, without repeats. Whether they exist is the
/// caller's to check.
pub fn mentioned(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: &str| {
        let s = s.trim_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | '*' | '"' | '\'' | '(' | ')' | '[' | ']' | '<' | '>' | '{' | '}'));
        let s = s.strip_prefix("file://").unwrap_or(s);
        if is_picture_name(s) && !s.contains("://") && !out.iter().any(|o| o == s) {
            out.push(s.to_owned());
        }
    };
    // In backticks: the whole span (a path may have spaces there); a
    // multi-line span (a code block) and the rest, a word at a time.
    for (i, part) in text.split('`').enumerate() {
        if i % 2 == 1 && !part.contains('\n') {
            push(part.trim());
            continue;
        }
        for word in part.split(|c: char| c.is_whitespace() || c == '=' || c == '|') {
            // (A Markdown link's or image's target: the part in brackets.)
            let word = word.rsplit("](").next().unwrap_or(word);
            push(word);
        }
    }
    out
}

/// A row's pictures to show: its own (a relative path is in the session's
/// `folder`) and the picture files it names that exist. (On another
/// machine, only those kept here.)
pub fn shown(row: &crate::Row, folder: &std::path::Path) -> Vec<String> {
    if row.images.is_empty() && row.mentions.is_empty() {
        return Vec::new();
    }
    let remote = crate::remote::host_of(folder).is_some();
    let whole = |p: &str| -> Option<PathBuf> {
        let path = if let Some(rest) = p.strip_prefix("~/") {
            PathBuf::from(std::env::var_os("HOME")?).join(rest)
        } else {
            PathBuf::from(p)
        };
        if path.is_absolute() {
            Some(path)
        } else if remote || folder.as_os_str().is_empty() {
            None
        } else {
            Some(folder.join(path))
        }
    };
    let mut out: Vec<String> = Vec::new();
    let mut add = |p: PathBuf| {
        let p = p.to_string_lossy().into_owned();
        if !out.contains(&p) {
            out.push(p);
        }
    };
    for p in row.images.iter().filter_map(|p| whole(p)) {
        // (A cache file is local even for another machine's session.)
        if !remote || p.starts_with(cache_dir()) {
            add(p);
        }
    }
    for p in row.mentions.iter().filter_map(|p| whole(p)).filter(|p| !remote && p.is_file()) {
        add(p);
    }
    out
}

/// Where a picture of a row came from, in words: a project's file by its
/// path in the project, one kept inline by who gave it ("You sent", "From
/// Read shot.png", "In a reply"), another file by its path (~ for home).
pub fn origin(row: &crate::Row, path: &str, folder: &std::path::Path) -> String {
    let p = std::path::Path::new(path);
    if p.starts_with(cache_dir()) {
        return match row.kind {
            crate::RowKind::User => "You sent".into(),
            crate::RowKind::Tool => {
                let mut name = row.title.clone();
                if let Some(c) = name.get(..1) {
                    name = c.to_uppercase() + &name[1..];
                }
                format!("From {name} {}", row.subtitle).trim_end().to_owned()
            }
            _ => "In a reply".into(),
        };
    }
    match p.strip_prefix(folder) {
        Ok(rel) if !folder.as_os_str().is_empty() && !rel.as_os_str().is_empty() => rel.to_string_lossy().into_owned(),
        _ => crate::sidebar::tilde(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_where_a_picture_came_from() {
        let dir = std::env::temp_dir().join(format!("harlo-origins-{}", std::process::id()));
        TEST_DIR.with(|d| *d.borrow_mut() = Some(dir.clone()));
        let cached = dir.join("ab0acd98b.png").to_string_lossy().into_owned();
        let row = |kind, title: &str, subtitle: &str| crate::Row { kind, title: title.into(), subtitle: subtitle.into(), ..Default::default() };
        let project = std::path::Path::new("/w/app");
        assert_eq!(origin(&row(crate::RowKind::User, "", ""), &cached, project), "You sent");
        assert_eq!(origin(&row(crate::RowKind::Tool, "read", "shot.png"), &cached, project), "From Read shot.png");
        assert_eq!(origin(&row(crate::RowKind::Images, "", ""), &cached, project), "In a reply");
        assert_eq!(origin(&row(crate::RowKind::Images, "", ""), "/w/app/art/sun.png", project), "art/sun.png");
        assert_eq!(origin(&row(crate::RowKind::Images, "", ""), "/elsewhere/x.png", project), "/elsewhere/x.png");
    }

    #[test]
    fn finds_markdown_images() {
        let t = "Here: ![a cat](out/cat.png) and ![x](<my dir/b.jpg> \"t\") and ![web](https://x/y.png).";
        assert_eq!(markdown_images(t), ["out/cat.png", "my dir/b.jpg"]);
    }

    #[test]
    fn finds_mentioned_pictures() {
        let t = "Saved it to out/cat.png. Also `/tmp/a b.webp`, see (docs/shot.JPG); not notes.txt or .png or https://x/y.png";
        assert_eq!(mentioned(t), ["out/cat.png", "/tmp/a b.webp", "docs/shot.JPG"]);
        assert_eq!(mentioned("python gen.py --out=art/sun.png"), ["art/sun.png"]);
    }

    #[test]
    fn keeps_inline_pictures_as_files() {
        let dir = std::env::temp_dir().join(format!("harlo-pictures-{}", std::process::id()));
        TEST_DIR.with(|d| *d.borrow_mut() = Some(dir.clone()));
        let block = serde_json::json!({ "type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png" });
        let a = inline_file(&block).unwrap();
        assert!(a.ends_with(".png") && std::fs::read(&a).unwrap().starts_with(b"\x89PNG"));
        let anthropic = serde_json::json!({ "type": "image", "source": { "type": "base64", "data": "iVBORw0KGgo=", "media_type": "image/png" } });
        assert_eq!(inline_file(&anthropic).unwrap(), a, "the same picture, the same file");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
