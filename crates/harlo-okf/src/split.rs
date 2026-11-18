//! A proposed split of uncovered files into pages, by folder.
//!
//! Create starts from this: a folder whose code fits one page is one group;
//! a bigger one splits into its subfolders, with its own files as another
//! group. The writer may merge or rename them; it only has to cover every
//! file somewhere.

// Rust guideline compliant 2026-10-09

use std::collections::BTreeMap;
use std::path::Path;

/// More lines than this make a folder more than one page.
const PAGE_LINES: usize = 6000;

/// A cluster smaller than this joins its folder's other small files.
const CLUSTER_LINES: usize = PAGE_LINES / 6;

/// Files that would make one page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// What it is, in words: `crates/x/`, `crates/x/ (its own files)`,
    /// `qml/Workflow*`, `qml/ (the rest)`.
    pub name: String,
    pub files: Vec<String>,
    pub lines: usize,
}

impl Group {
    /// About how many pages its code makes.
    pub fn pages(&self) -> usize {
        self.lines.div_ceil(PAGE_LINES).max(1)
    }
}

#[derive(Default)]
struct Folder {
    files: Vec<(String, usize)>,
    subs: BTreeMap<String, Folder>,
}

impl Folder {
    fn lines(&self) -> usize {
        self.files.iter().map(|(_, n)| n).sum::<usize>() + self.subs.values().map(Folder::lines).sum::<usize>()
    }

    fn all_files(&self, out: &mut Vec<String>) {
        out.extend(self.files.iter().map(|(f, _)| f.clone()));
        for s in self.subs.values() {
            s.all_files(out);
        }
    }

    fn split(&self, path: &str, out: &mut Vec<Group>) {
        let lines = self.lines();
        let shown = if path.is_empty() { "(top)" } else { path };
        if lines <= PAGE_LINES || self.subs.is_empty() {
            if self.subs.is_empty() && lines > PAGE_LINES {
                clusters(shown, "", &self.files, out);
            } else {
                let mut files = Vec::new();
                self.all_files(&mut files);
                out.push(Group { name: shown.to_owned(), files, lines });
            }
            return;
        }
        if !self.files.is_empty() {
            clusters(shown, " (its own files)", &self.files, out);
        }
        for (name, sub) in &self.subs {
            sub.split(&format!("{path}{name}/"), out);
        }
    }
}

/// A file name's first word: `Workflow` of `WorkflowCard.qml`, `tool` of `tool_calls.rs`.
fn first_word(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let stem = name.split('.').next().unwrap_or(name);
    let mut end = stem.find(['_', '-']).unwrap_or(stem.len());
    if let Some(upper) = stem.char_indices().skip(1).find(|(_, c)| c.is_uppercase()).map(|(i, _)| i) {
        end = end.min(upper);
    }
    stem[..end].to_owned()
}

/// A folder's own `files` as groups: one when they fit a page, else by
/// their names' first word, the small clusters together.
fn clusters(folder: &str, own: &str, files: &[(String, usize)], out: &mut Vec<Group>) {
    let total: usize = files.iter().map(|(_, n)| n).sum();
    let group = |name: String, list: &[&(String, usize)]| Group { name, files: list.iter().map(|(f, _)| f.clone()).collect(), lines: list.iter().map(|(_, n)| n).sum() };
    let all: Vec<&(String, usize)> = files.iter().collect();
    if total <= PAGE_LINES || files.len() < 2 {
        out.push(group(format!("{folder}{own}"), &all));
        return;
    }
    let mut by_word: BTreeMap<String, Vec<&(String, usize)>> = BTreeMap::new();
    for f in files {
        by_word.entry(first_word(&f.0)).or_default().push(f);
    }
    let mut rest = Vec::new();
    let mut found = Vec::new();
    for (word, list) in by_word {
        if list.len() > 1 && list.iter().map(|(_, n)| n).sum::<usize>() >= CLUSTER_LINES {
            found.push(group(format!("{folder}{word}*"), &list));
        } else {
            rest.extend(list);
        }
    }
    if found.is_empty() {
        out.push(group(format!("{folder}{own}"), &all));
        return;
    }
    out.extend(found);
    if !rest.is_empty() {
        out.push(group(format!("{folder} (the rest{})", if own.is_empty() { "" } else { " of its own files" }), &rest));
    }
}

/// The groups `files` (paths in the repository at `root`) fall into.
pub fn propose(root: &Path, files: &[String]) -> Vec<Group> {
    let mut top = Folder::default();
    for f in files {
        let lines = std::fs::read_to_string(root.join(f)).map_or(0, |t| t.lines().count());
        let mut at = &mut top;
        let mut parts: Vec<&str> = f.split('/').collect();
        parts.pop();
        for part in parts {
            at = at.subs.entry(part.to_owned()).or_default();
        }
        at.files.push((f.clone(), lines));
    }
    let mut out = Vec::new();
    top.split("", &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_big_folders_by_subfolder() {
        let dir = std::env::temp_dir().join(format!("harlo-okf-split-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let big = "x\n".repeat(4000);
        let files = [("crates/a/src/lib.rs", big.as_str()), ("crates/a/src/run.rs", "x\n"), ("crates/b/src/lib.rs", big.as_str()), ("main.rs", "fn main() {}\n")];
        for (p, t) in files {
            let f = dir.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, t).unwrap();
        }
        let names: Vec<String> = files.iter().map(|(p, _)| (*p).to_owned()).collect();
        let groups = propose(&dir, &names);
        std::fs::remove_dir_all(&dir).unwrap();
        let shown: Vec<(&str, usize, usize)> = groups.iter().map(|g| (g.name.as_str(), g.files.len(), g.lines)).collect();
        assert_eq!(shown, [("(top) (its own files)", 1, 1), ("crates/a/", 2, 4001), ("crates/b/", 1, 4000)]);
    }

    #[test]
    fn splits_a_big_flat_folder_by_name() {
        let n = |f: &str, lines: usize| (f.to_owned(), lines);
        let files = [n("qml/WorkflowCard.qml", 3000), n("qml/WorkflowEditor.qml", 2000), n("qml/Settings.qml", 1500), n("qml/SettingsPage.qml", 900), n("qml/Main.qml", 400)];
        let mut out = Vec::new();
        clusters("qml/", "", &files, &mut out);
        let shown: Vec<(&str, usize)> = out.iter().map(|g| (g.name.as_str(), g.files.len())).collect();
        assert_eq!(shown, [("qml/Settings*", 2), ("qml/Workflow*", 2), ("qml/ (the rest)", 1)]);
        assert_eq!(first_word("src/tool_calls.rs"), "tool");
    }
}
