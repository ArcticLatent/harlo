//! A folder's files as a tree, and how a file's bytes are shown.
//!
//! [`Tree`] keeps what's been listed of a folder and which subfolders are
//! open; [`Tree::rows`] flattens what's visible, folders first. Listing is
//! the caller's: [`Tree::toggle`] says which folder to list, and
//! [`Tree::listed`] takes the result. [`classify`] decides whether a file
//! opens as text, as an image, or not at all.

// Rust guideline compliant 2026-10-09

use std::collections::{HashMap, HashSet};

use crate::server::FileMatch;

/// The folder Git keeps its own data in, which the tree leaves out.
const GIT_DIR: &str = ".git";

/// The largest file opened, in bytes; a bigger one says so instead.
pub const MAX_OPEN: usize = 8 * 1024 * 1024;

/// Extensions of images the viewer shows.
const IMAGES: [&str; 8] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "tiff"];

/// Extension of vector images, which the viewer draws separately.
const SVG: &str = "svg";

/// What a folder holds, as far as it's been listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listing {
    /// Its list is on the way.
    Loading,
    /// Its entries, folders first, then by name.
    Loaded(Vec<Node>),
    /// It couldn't be listed.
    Failed(String),
}

/// A file or folder in the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Its path from the tree's root, with `/` between names.
    pub path: String,
    /// Its own name.
    pub name: String,
    /// Whether it's a folder.
    pub directory: bool,
    /// Whether Git ignores it.
    pub ignored: bool,
}

/// One visible line of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The file or folder.
    pub node: Node,
    /// How many folders it's inside, below the root.
    pub depth: usize,
    /// For a folder, whether it's open.
    pub open: bool,
    /// For an open folder, whether its list is still on the way.
    pub loading: bool,
}

/// A folder's files: what's been listed, and which folders are open.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tree {
    /// Each listed folder's entries, by its path (`""` for the root).
    listings: HashMap<String, Listing>,
    /// Folders the user opened.
    open: HashSet<String>,
}

impl Tree {
    /// Creates a tree with nothing listed yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The root's listing, once asked for.
    #[must_use]
    pub fn root(&self) -> Option<&Listing> {
        self.listings.get("")
    }

    /// Marks `folder` as being listed, and returns it to list.
    ///
    /// A folder listed before keeps its entries until the new list arrives.
    pub fn load(&mut self, folder: &str) -> String {
        if !matches!(self.listings.get(folder), Some(Listing::Loaded(_))) {
            self.listings.insert(folder.to_owned(), Listing::Loading);
        }
        folder.to_owned()
    }

    /// Opens or closes a folder; returns it when it needs listing first.
    pub fn toggle(&mut self, folder: &str) -> Option<String> {
        if self.open.remove(folder) {
            return None;
        }
        self.open.insert(folder.to_owned());
        match self.listings.get(folder) {
            Some(Listing::Loaded(_) | Listing::Loading) => None,
            Some(Listing::Failed(_)) | None => Some(self.load(folder)),
        }
    }

    /// Takes `folder`'s entries, or why it couldn't be listed.
    ///
    /// `ignored` names the entries Git ignores. Git's own folder is left out.
    pub fn listed(
        &mut self,
        folder: &str,
        entries: Result<Vec<FileMatch>, String>,
        ignored: &HashSet<String>,
    ) {
        let listing = match entries {
            Ok(entries) => {
                let mut nodes: Vec<Node> = entries
                    .into_iter()
                    .filter(|e| name_of(&e.path) != GIT_DIR)
                    .map(|e| Node {
                        name: name_of(&e.path).to_owned(),
                        ignored: ignored.contains(&e.path),
                        directory: e.directory,
                        path: e.path,
                    })
                    .collect();
                nodes.sort_by(|a, b| {
                    b.directory
                        .cmp(&a.directory)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                        .then_with(|| a.name.cmp(&b.name))
                });
                Listing::Loaded(nodes)
            }
            Err(error) => Listing::Failed(error),
        };
        self.listings.insert(folder.to_owned(), listing);
    }

    /// The listed folders a change to `path` affects: its own folder, and
    /// the path itself when it's a listed folder.
    #[must_use]
    pub fn affected(&self, path: &str) -> Vec<String> {
        let parent = parent_of(path);
        [parent, path]
            .into_iter()
            .filter(|folder| self.listings.contains_key(*folder))
            .map(str::to_owned)
            .collect()
    }

    /// The folders listed so far, to list again on a refresh.
    #[must_use]
    pub fn listed_folders(&self) -> Vec<String> {
        let mut folders: Vec<String> = self.listings.keys().cloned().collect();
        folders.sort();
        folders
    }

    /// The visible lines: the root's entries and, under each open folder,
    /// its own.
    #[must_use]
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        self.push_rows("", 0, &mut rows);
        rows
    }

    fn push_rows(&self, folder: &str, depth: usize, rows: &mut Vec<Row>) {
        let Some(Listing::Loaded(nodes)) = self.listings.get(folder) else {
            return;
        };
        for node in nodes {
            let open = node.directory && self.open.contains(&node.path);
            let loading = open && matches!(self.listings.get(&node.path), Some(Listing::Loading));
            rows.push(Row {
                node: node.clone(),
                depth,
                open,
                loading,
            });
            if open {
                self.push_rows(&node.path, depth + 1, rows);
            }
        }
    }
}

/// The last name in a path.
#[must_use]
pub fn name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The folder holding `path`, `""` at the root.
#[must_use]
pub fn parent_of(path: &str) -> &str {
    path.rfind('/').map_or("", |at| &path[..at])
}

/// How a file opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// As text, to edit.
    Text(String),
    /// As a picture.
    Image,
    /// As a vector picture.
    Svg,
    /// Not at all: it isn't text.
    Binary,
    /// Not at all: it's larger than [`MAX_OPEN`] bytes.
    TooLarge(usize),
}

/// Decides how a file opens, from its name and bytes.
///
/// Pictures go by extension. Anything else is text when it's UTF-8 without
/// NUL bytes.
#[must_use]
pub fn classify(path: &str, bytes: Vec<u8>) -> Kind {
    if bytes.len() > MAX_OPEN {
        return Kind::TooLarge(bytes.len());
    }
    let extension = extension_of(path);
    if extension == SVG {
        return Kind::Svg;
    }
    if IMAGES.contains(&extension.as_str()) {
        return Kind::Image;
    }
    if bytes.contains(&0) {
        return Kind::Binary;
    }
    String::from_utf8(bytes).map_or(Kind::Binary, Kind::Text)
}

/// A file's extension, lowercased; empty when it has none.
#[must_use]
pub fn extension_of(path: &str) -> String {
    let name = name_of(path);
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => extension.to_lowercase(),
        _ => String::new(),
    }
}

/// The syntax to highlight a file with, as the editor names it: usually the
/// extension, or a name for files known without one.
#[must_use]
pub fn syntax_of(path: &str) -> String {
    match name_of(path) {
        "Makefile" | "makefile" | "GNUmakefile" => "makefile".to_owned(),
        "Dockerfile" => "dockerfile".to_owned(),
        "CMakeLists.txt" => "cmake".to_owned(),
        ".bashrc" | ".zshrc" | ".profile" | "PKGBUILD" => "sh".to_owned(),
        _ => match extension_of(path) {
            ext if ext.is_empty() => "txt".to_owned(),
            ext => ext,
        },
    }
}
