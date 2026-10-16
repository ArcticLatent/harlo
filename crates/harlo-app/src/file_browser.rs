//! `FileBrowser`: the active session's folder as an expandable tree (a
//! flat list of visible rows). Files open in editor tabs (`TextFile`).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel = crate::qt_base::qobject::QAbstractListModel;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
    }

    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(QString, folder)]
        type FileBrowser = super::FileBrowserRust;
    }

    impl cxx_qt::Threading for FileBrowser {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// Shows `dir` (a session's folder); the tree resets.
        #[qinvokable]
        fn set_root(self: Pin<&mut FileBrowser>, dir: &QString);
        /// Expands or collapses the directory at row `index`.
        #[qinvokable]
        fn toggle(self: Pin<&mut FileBrowser>, index: i32);
        /// Re-reads the tree, keeping expanded folders expanded.
        #[qinvokable]
        fn refresh(self: Pin<&mut FileBrowser>);
    }

    extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(self: Pin<&mut FileBrowser>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut FileBrowser>);
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(self: Pin<&mut FileBrowser>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut FileBrowser>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut FileBrowser>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut FileBrowser>);
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(self: Pin<&mut FileBrowser>, top_left: &QModelIndex, bottom_right: &QModelIndex, roles: &QVector_i32);
        #[inherit]
        fn index(self: &FileBrowser, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qvector.h");
        type QVector_i32 = cxx_qt_lib::QVector<i32>;
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &FileBrowser, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &FileBrowser) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &FileBrowser, parent: &QModelIndex) -> i32;
    }
}

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector};
use harlo_core::files::{self, DirEntry};

use qobject::FileBrowser;

const ROLE_REL: i32 = 0x100;
const ROLE_NAME: i32 = 0x101;
const ROLE_DEPTH: i32 = 0x102;
const ROLE_IS_DIR: i32 = 0x103;
const ROLE_EXPANDED: i32 = 0x104;
const ROLE_ICON: i32 = 0x105;


#[derive(Clone)]
struct Row {
    entry: DirEntry,
    depth: i32,
    expanded: bool,
}

#[derive(Default)]
pub struct FileBrowserRust {
    folder: QString,
    root: PathBuf,
    rows: Vec<Row>,
    /// Bumped on each reload: an older (remote) listing arriving late is
    /// dropped.
    generation: u64,
}

fn build_rows(root: &Path, rel: &str, depth: i32, expanded: &HashSet<String>, out: &mut Vec<Row>) {
    for entry in files::list_dir(root, rel).unwrap_or_default() {
        let open = entry.is_dir && expanded.contains(&entry.rel);
        let rel = entry.rel.clone();
        out.push(Row { entry, depth, expanded: open });
        if open {
            build_rows(root, &rel, depth + 1, expanded, out);
        }
    }
}

impl FileBrowser {
    pub fn set_root(mut self: Pin<&mut Self>, dir: &QString) {
        if &self.folder == dir {
            return;
        }
        self.as_mut().set_folder(dir.clone());
        self.as_mut().rust_mut().root = PathBuf::from(dir.to_string());
        self.as_mut().reload(&HashSet::new());
    }

    fn reload(mut self: Pin<&mut Self>, expanded: &HashSet<String>) {
        self.as_mut().rust_mut().generation += 1;
        let root = self.root.clone();
        // A remote folder: listed over SSH off the UI thread (a slow host
        // mustn't freeze the window); the tree shows when it arrives.
        if harlo_core::remote::is_remote(&root) {
            let (generation, expanded, thread) = (self.generation, expanded.clone(), self.qt_thread());
            crate::runtime().spawn_blocking(move || {
                let mut rows = Vec::new();
                build_rows(&root, "", 0, &expanded, &mut rows);
                let _ = thread.queue(move |mut this| {
                    if this.generation == generation {
                        this.as_mut().set_rows(rows);
                    }
                });
            });
            return;
        }
        let mut rows = Vec::new();
        if !root.as_os_str().is_empty() {
            build_rows(&root, "", 0, expanded, &mut rows);
        }
        self.set_rows(rows);
    }

    fn set_rows(mut self: Pin<&mut Self>, rows: Vec<Row>) {
        unsafe {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().rows = rows;
            self.as_mut().end_reset_model();
        }
    }

    /// A folder's entries under the row `rel` (once it's listed).
    fn insert_children(mut self: Pin<&mut Self>, rel: &str, entries: Vec<DirEntry>) {
        let root = QModelIndex::default();
        // (Still there, still open, not filled yet.)
        let Some(i) = self.rows.iter().position(|r| r.entry.rel == rel && r.expanded) else { return };
        let depth = self.rows[i].depth;
        if self.rows.get(i + 1).is_some_and(|r| r.depth > depth) {
            return;
        }
        let children: Vec<Row> = entries.into_iter().map(|entry| Row { entry, depth: depth + 1, expanded: false }).collect();
        if !children.is_empty() {
            unsafe {
                self.as_mut().begin_insert_rows(&root, (i + 1) as i32, (i + children.len()) as i32);
                self.as_mut().rust_mut().rows.splice(i + 1..i + 1, children);
                self.as_mut().end_insert_rows();
            }
        }
    }

    pub fn refresh(self: Pin<&mut Self>) {
        let expanded: HashSet<String> = self.rows.iter().filter(|r| r.expanded).map(|r| r.entry.rel.clone()).collect();
        self.reload(&expanded);
    }

    pub fn toggle(mut self: Pin<&mut Self>, index: i32) {
        let i = index.max(0) as usize;
        let Some(row) = self.rows.get(i).cloned() else { return };
        if !row.entry.is_dir {
            return;
        }
        let root = QModelIndex::default();
        if row.expanded {
            let end = self.rows[i + 1..].iter().position(|r| r.depth <= row.depth).map_or(self.rows.len(), |p| i + 1 + p);
            if end > i + 1 {
                unsafe {
                    self.as_mut().begin_remove_rows(&root, (i + 1) as i32, (end - 1) as i32);
                    self.as_mut().rust_mut().rows.drain(i + 1..end);
                    self.as_mut().end_remove_rows();
                }
            }
            self.as_mut().rust_mut().rows[i].expanded = false;
        } else {
            self.as_mut().rust_mut().rows[i].expanded = true;
            let (dir, rel) = (self.root.clone(), row.entry.rel.clone());
            if harlo_core::remote::is_remote(&dir) {
                // Over SSH, off the UI thread: the arrow turns now, the
                // entries follow.
                let (generation, thread) = (self.generation, self.qt_thread());
                crate::runtime().spawn_blocking(move || {
                    let entries = files::list_dir(&dir, &rel).unwrap_or_default();
                    let _ = thread.queue(move |mut this| {
                        if this.generation == generation {
                            this.as_mut().insert_children(&rel, entries);
                        }
                    });
                });
            } else {
                let entries = files::list_dir(&dir, &rel).unwrap_or_default();
                self.as_mut().insert_children(&rel, entries);
            }
        }
        let idx = self.index(i as i32, 0, &root);
        self.as_mut().data_changed(&idx, &idx, &QVector::<i32>::default());
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(r) = self.rows.get(index.row() as usize) else { return QVariant::default() };
        match role {
            ROLE_REL => QVariant::from(&QString::from(&r.entry.rel)),
            ROLE_NAME => QVariant::from(&QString::from(&r.entry.name)),
            ROLE_DEPTH => QVariant::from(&r.depth),
            ROLE_IS_DIR => QVariant::from(&r.entry.is_dir),
            ROLE_EXPANDED => QVariant::from(&r.expanded),
            ROLE_ICON => QVariant::from(&if r.entry.is_dir { QString::default() } else { QString::from(&crate::file_icons::url_for(&r.entry.name)) }),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [(ROLE_REL, "rel"), (ROLE_NAME, "name"), (ROLE_DEPTH, "depth"), (ROLE_IS_DIR, "isDir"), (ROLE_EXPANDED, "expanded"), (ROLE_ICON, "fileIcon")] {
            roles.insert(role, QByteArray::from(name));
        }
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rows.len() as i32
    }
}
