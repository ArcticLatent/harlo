//! The `.okf/` folder: its pages, and the index and log files Harlo writes.
//!
//! Every Markdown file under `.okf/` is a page, except the reserved
//! `index.md` and `log.md` (OKF §3.1) and hidden files and folders (the
//! ledger). Indexes are written from the pages' front matter, as OKF §8
//! lays them out: the root `index.md` carries `okf_version` and lists the
//! overview and the folders; each folder's `index.md` lists its pages under
//! the folder's name, then its drafts and its deprecated pages. The log is
//! OKF §9's: `## YYYY-MM-DD` headings, newest first.

// Rust guideline compliant 2026-10-09

use crate::page::{self, Page};
use std::fmt::Write as _;
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

/// The bundle's folder in a repository.
pub const DIR: &str = ".okf";
/// The OKF version the bundle follows.
pub const OKF_VERSION: &str = "0.2";

/// A repository's wiki, as read from its `.okf/` folder.
#[derive(Clone, Debug)]
pub struct Bundle {
    /// The repository.
    pub root: PathBuf,
    pub pages: Vec<Page>,
}

impl Bundle {
    /// The `.okf/` folder.
    pub fn dir(&self) -> PathBuf {
        self.root.join(DIR)
    }

    /// The page at `path` in the bundle.
    pub fn page(&self, path: &str) -> Option<&Page> {
        self.pages.iter().find(|p| p.path == path)
    }
}

/// Whether a repository has a wiki.
pub fn exists(root: &Path) -> bool {
    root.join(DIR).join("index.md").is_file() || root.join(DIR).is_dir()
}

/// Reads the wiki of the repository at `root` (no pages when there's none).
///
/// # Errors
///
/// When a page can't be read (a folder that can't be listed is skipped).
pub fn load(root: &Path) -> io::Result<Bundle> {
    let dir = root.join(DIR);
    let mut paths = Vec::new();
    collect(&dir, "", &mut paths);
    paths.sort();
    let mut pages = Vec::with_capacity(paths.len());
    for rel in paths {
        let text = std::fs::read_to_string(dir.join(&rel))?;
        pages.push(page::parse(&rel, &text));
    }
    Ok(Bundle { root: root.to_owned(), pages })
}

fn collect(dir: &Path, prefix: &str, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else { continue };
        let rel = format!("{prefix}{name}");
        if kind.is_dir() {
            collect(&entry.path(), &format!("{rel}/"), out);
        } else if kind.is_file() && name.ends_with(".md") && name != "index.md" && name != "log.md" {
            out.push(rel);
        }
    }
}

/// A folder's name as a heading (`code-view` → `Code view`).
pub fn folder_title(name: &str) -> String {
    let words = name.replace(['-', '_'], " ");
    let mut chars = words.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

fn entry(title: &str, target: &str, description: Option<&str>) -> String {
    match description.map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => format!("* [{title}]({target}) - {}\n", d.replace('\n', " ")),
        None => format!("* [{title}]({target})\n"),
    }
}

fn title_of(p: &Page) -> String {
    p.front.title.clone().unwrap_or_else(|| {
        let stem = p.path.rsplit('/').next().unwrap_or(&p.path).trim_end_matches(".md");
        folder_title(stem)
    })
}

/// The index files the bundle should have: bundle path → text.
pub fn indexes(pages: &[Page]) -> BTreeMap<String, String> {
    // Folder → (its pages, its subfolders).
    let mut folders: BTreeMap<String, (Vec<&Page>, Vec<String>)> = BTreeMap::new();
    folders.entry(String::new()).or_default();
    for p in pages {
        let folder = p.path.rsplit_once('/').map_or("", |(f, _)| f).to_owned();
        // Every folder up to the root knows its subfolders.
        let mut f = folder.clone();
        while !f.is_empty() {
            let parent = f.rsplit_once('/').map_or("", |(p, _)| p).to_owned();
            let subs = &mut folders.entry(parent.clone()).or_default().1;
            if !subs.contains(&f) {
                subs.push(f.clone());
            }
            f = parent;
        }
        folders.entry(folder).or_default().0.push(p);
    }
    let mut out = BTreeMap::new();
    for (folder, (mut list, mut subs)) in folders {
        list.sort_by_key(|p| (title_of(p).to_lowercase(), p.path.clone()));
        subs.sort();
        let name = |p: &Page| p.path.rsplit('/').next().unwrap_or(&p.path).to_owned();
        let status = |p: &Page| p.front.status.clone().unwrap_or_default();
        let mut text = String::new();
        let current: Vec<&&Page> = list.iter().filter(|p| !matches!(status(p).as_str(), "draft" | "deprecated")).collect();
        if folder.is_empty() {
            let _ = write!(text, "---\nokf_version: \"{OKF_VERSION}\"\n---\n");
            let (start, rest): (Vec<&&Page>, Vec<&&Page>) = current.iter().partition(|p| p.path == "overview.md");
            if !start.is_empty() {
                text.push_str("# Start here\n\n");
                for p in start {
                    text.push_str(&entry(&title_of(p), &name(p), p.front.description.as_deref()));
                }
            }
            if !rest.is_empty() {
                text.push_str(if text.ends_with("---\n") { "# Pages\n\n" } else { "\n# Pages\n\n" });
                for p in rest {
                    text.push_str(&entry(&title_of(p), &name(p), p.front.description.as_deref()));
                }
            }
        } else if !current.is_empty() {
            let last = folder.rsplit('/').next().unwrap_or(&folder);
            let _ = write!(text, "# {}\n\n", folder_title(last));
            for p in &current {
                text.push_str(&entry(&title_of(p), &name(p), p.front.description.as_deref()));
            }
        }
        for (heading, which) in [("Draft", "draft"), ("Deprecated", "deprecated")] {
            let these: Vec<&&Page> = list.iter().filter(|p| status(p) == which).collect();
            if !these.is_empty() {
                let gap = if text.is_empty() || text.ends_with("---\n") { "" } else { "\n" };
                let _ = write!(text, "{gap}# {heading}\n\n");
                for p in these {
                    text.push_str(&entry(&title_of(p), &name(p), p.front.description.as_deref()));
                }
            }
        }
        if !subs.is_empty() {
            let gap = if text.is_empty() || text.ends_with("---\n") { "" } else { "\n" };
            let _ = write!(text, "{gap}# Folders\n\n");
            for s in &subs {
                let short = s.rsplit('/').next().unwrap_or(s);
                text.push_str(&entry(&folder_title(short), &format!("{short}/index.md"), None));
            }
        }
        let path = if folder.is_empty() { "index.md".to_owned() } else { format!("{folder}/index.md") };
        out.insert(path, text);
    }
    out
}

/// What happened to a page, for the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Creation,
    Update,
    Deprecation,
}

impl Event {
    fn word(self) -> &'static str {
        match self {
            Event::Creation => "Creation",
            Event::Update => "Update",
            Event::Deprecation => "Deprecation",
        }
    }
}

/// The log `existing` with a line per event added under `date` (YYYY-MM-DD).
///
/// Events of one kind share a line; the date's heading is made when it
/// isn't there, above the older ones.
pub fn log(existing: &str, date: &str, events: &[(Event, String, String)]) -> String {
    let existing = if existing.trim().is_empty() { "# Log\n" } else { existing };
    let mut lines: Vec<String> = Vec::new();
    for kind in [Event::Creation, Event::Update, Event::Deprecation] {
        let links: Vec<String> = events.iter().filter(|(k, _, _)| *k == kind).map(|(_, title, path)| format!("[{title}](/{path})")).collect();
        if !links.is_empty() {
            lines.push(format!("* **{}**: {}", kind.word(), links.join(", ")));
        }
    }
    if lines.is_empty() {
        return existing.to_owned();
    }
    let heading = format!("## {date}");
    let mut out: Vec<String> = existing.lines().map(str::to_owned).collect();
    if let Some(at) = out.iter().position(|l| l.trim() == heading) {
        let mut insert = at + 1;
        while insert < out.len() && out[insert].trim().is_empty() {
            insert += 1;
        }
        for (n, l) in lines.into_iter().enumerate() {
            out.insert(insert + n, l);
        }
    } else {
        let first_date = out.iter().position(|l| l.starts_with("## ")).unwrap_or(out.len());
        let mut block = vec![heading, String::new()];
        block.extend(lines);
        block.push(String::new());
        // (A blank line before it, after the title.)
        if first_date > 0 && !out[first_date - 1].trim().is_empty() {
            block.insert(0, String::new());
        }
        out.splice(first_date..first_date, block);
    }
    let mut text = out.join("\n");
    while text.ends_with("\n\n") || text.ends_with('\n') {
        text.pop();
    }
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(path: &str, title: &str, status: Option<&str>) -> Page {
        let status = status.map_or(String::new(), |s| format!("status: {s}\n"));
        page::parse(path, &format!("---\ntype: Concept\ntitle: {title}\ndescription: About {title}.\n{status}---\n# {title}\n"))
    }

    #[test]
    fn writes_indexes() {
        let pages = vec![page("overview.md", "Overview", None), page("architecture/wiki.md", "Wiki engine", None), page("architecture/old.md", "Old engine", Some("deprecated")), page("architecture/deep/x.md", "X", Some("draft"))];
        let idx = indexes(&pages);
        assert_eq!(idx["index.md"], "---\nokf_version: \"0.2\"\n---\n# Start here\n\n* [Overview](overview.md) - About Overview.\n\n# Folders\n\n* [Architecture](architecture/index.md)\n");
        assert_eq!(idx["architecture/index.md"], "# Architecture\n\n* [Wiki engine](wiki.md) - About Wiki engine.\n\n# Deprecated\n\n* [Old engine](old.md) - About Old engine.\n\n# Folders\n\n* [Deep](deep/index.md)\n");
        assert_eq!(idx["architecture/deep/index.md"], "# Draft\n\n* [X](x.md) - About X.\n");
    }

    #[test]
    fn writes_the_log_newest_first() {
        let one = log("", "2026-11-17", &[(Event::Creation, "Wiki".into(), "wiki.md".into()), (Event::Creation, "Runs".into(), "runs.md".into())]);
        assert_eq!(one, "# Log\n\n## 2026-11-17\n\n* **Creation**: [Wiki](/wiki.md), [Runs](/runs.md)\n");
        let two = log(&one, "2026-11-18", &[(Event::Update, "Runs".into(), "runs.md".into())]);
        assert_eq!(two, "# Log\n\n## 2026-11-18\n\n* **Update**: [Runs](/runs.md)\n\n## 2026-11-17\n\n* **Creation**: [Wiki](/wiki.md), [Runs](/runs.md)\n");
        let same_day = log(&two, "2026-11-18", &[(Event::Deprecation, "Wiki".into(), "wiki.md".into())]);
        assert!(same_day.starts_with("# Log\n\n## 2026-11-18\n\n* **Deprecation**: [Wiki](/wiki.md)\n* **Update**"), "{same_day}");
    }
}
