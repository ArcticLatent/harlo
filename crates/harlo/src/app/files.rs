//! Files: the Files panel's tree of the current folder, and files open in
//! editor tabs beside the session tabs.
//!
//! Everything goes through the server: it lists folders, reads and writes
//! files, and reports changes to the files it's asked to watch. A file that
//! changes on disk reloads in its tab, unless the tab has unsaved changes;
//! then the tab says so and lets the user choose.

// Rust guideline compliant 2026-10-09

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::Path;
use std::time::Duration;

use harlo_core::files::{self, Kind, Tree};
use harlo_core::git::Repo;
use harlo_core::server::{FileChange, FileMatch};
use harlo_core::workspace::EventKind;
use iced::Task;
use iced::widget::{image, svg};
use iced_code_editor::{CodeEditor, Message as EditorMessage};

use super::{App, Message, Panel};
use crate::style;

/// How often the editor's cursor blinks.
pub const BLINK: Duration = Duration::from_millis(500);

/// The editor's text size.
const EDITOR_TEXT: f32 = 13.0;

/// Something that happened to the Files panel or an open file.
#[derive(Debug, Clone)]
pub enum FilesEvent {
    /// Open or close a folder in the tree, by its path in the current folder.
    Toggle(String),
    /// List the tree's open folders again.
    Refresh,
    /// A folder's entries arrived.
    Listed {
        /// The folder the tree is for.
        root: String,
        /// The listed folder, relative to `root`.
        folder: String,
        /// Its entries, or why it couldn't be listed.
        entries: Result<Vec<FileMatch>, String>,
        /// The entries Git ignores.
        ignored: HashSet<String>,
    },
    /// Open a file from the tree, by its path in the current folder.
    Open(String),
    /// An open file's bytes arrived.
    Read {
        /// The file's key.
        key: String,
        /// Its bytes, or why they couldn't be read.
        bytes: Result<Vec<u8>, String>,
    },
    /// What's on disk now, read after the server said the file changed.
    Reread {
        /// The file's key.
        key: String,
        /// Its bytes, or why they couldn't be read (it was deleted).
        bytes: Result<Vec<u8>, String>,
    },
    /// Show an open file's tab.
    Show(String),
    /// Something for a file's editor.
    Editor(String, EditorMessage),
    /// Save a file.
    Save(String),
    /// A save finished.
    Saved {
        /// The file's key.
        key: String,
        /// The text written.
        text: String,
        /// Whether it worked.
        result: Result<(), String>,
    },
    /// Close a file's tab, asking first when it has unsaved changes.
    Close(String),
    /// Save the file, then close its tab.
    SaveAndClose(String),
    /// Close the tab, dropping unsaved changes.
    Discard(String),
    /// Keep the tab open after all.
    KeepOpen(String),
    /// Take what's on disk, dropping unsaved changes.
    Reload(String),
    /// Keep the unsaved changes over what's on disk.
    KeepMine(String),
    /// Time for the cursor to blink.
    Blink,
}

/// A file open in a tab.
#[derive(Debug)]
pub struct OpenFile {
    /// Its absolute path, which identifies it.
    pub key: String,
    /// The folder it was opened from, which the server reads it in.
    pub folder: String,
    /// Its path in that folder.
    pub path: String,
    /// What the tab shows.
    pub body: Body,
    /// Whether closing it waits for the user to save or drop changes.
    pub closing: bool,
    /// The file to open in its place once it closes, by folder and path.
    pub then_open: Option<(String, String)>,
}

impl OpenFile {
    /// Its name, for the tab.
    #[must_use]
    pub fn name(&self) -> &str {
        files::name_of(&self.path)
    }

    /// Whether it has changes that aren't saved.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        matches!(&self.body, Body::Text(text) if text.dirty)
    }
}

/// What an open file's tab shows.
#[derive(Debug)]
pub enum Body {
    /// Its bytes are on the way.
    Loading,
    /// Text, in an editor.
    Text(Box<TextFile>),
    /// A picture.
    Image(image::Handle),
    /// A vector picture.
    Svg(svg::Handle),
    /// Nothing: it isn't text.
    Binary,
    /// Nothing: it's this many bytes, too large to open.
    TooLarge(usize),
    /// Nothing: it couldn't be read.
    Failed(String),
}

/// A text file being edited.
pub struct TextFile {
    /// The editor.
    pub editor: CodeEditor,
    /// The text as last read or saved.
    pub saved: String,
    /// Whether the editor's text differs from [`TextFile::saved`].
    pub dirty: bool,
    /// Whether a save is on the way.
    pub saving: bool,
    /// Whether to close the tab once the save lands.
    pub close_after_save: bool,
    /// What happened on disk while the tab had unsaved changes.
    pub outside: Option<Outside>,
    /// Why the last save failed.
    pub error: Option<String>,
}

impl fmt::Debug for TextFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextFile")
            .field("saved", &self.saved.len())
            .field("dirty", &self.dirty)
            .field("saving", &self.saving)
            .field("outside", &self.outside)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

/// A change on disk to a file with unsaved changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outside {
    /// It now holds this text.
    Changed(String),
    /// It was deleted.
    Removed,
}

/// The Files panel's trees, one per folder, and the open files.
#[derive(Debug, Default)]
pub struct Files {
    /// Each folder's tree, by the folder's path.
    pub trees: HashMap<String, Tree>,
    /// Open files, in tab order.
    pub open: Vec<OpenFile>,
    /// The open file, while its tab is the current document.
    pub shown: Option<String>,
    /// Folders whose trees may be out of date: an agent changed files there,
    /// or the window was away.
    pub stale: HashSet<String>,
    /// Whether the Files panel was on screen at the last update.
    pub visible: bool,
}

impl App {
    /// The tree of the folder the Files panel shows.
    #[must_use]
    pub fn file_tree(&self) -> (String, Option<&Tree>) {
        let folder = self.folder();
        let tree = self.fs.trees.get(&folder);
        (folder, tree)
    }

    /// The open file, while its tab is the current document.
    #[must_use]
    pub fn shown_file(&self) -> Option<&OpenFile> {
        let key = self.fs.shown.as_deref()?;
        self.fs.open.iter().find(|f| f.key == key)
    }

    /// An open file's text, as the editor holds it.
    #[must_use]
    pub fn file_text(&self, key: &str) -> Option<String> {
        match &self.file(key)?.body {
            Body::Text(text) => Some(text.editor.content()),
            _ => None,
        }
    }

    fn file(&self, key: &str) -> Option<&OpenFile> {
        self.fs.open.iter().find(|f| f.key == key)
    }

    fn file_mut(&mut self, key: &str) -> Option<&mut OpenFile> {
        self.fs.open.iter_mut().find(|f| f.key == key)
    }

    fn text_mut(&mut self, key: &str) -> Option<&mut TextFile> {
        match &mut self.file_mut(key)?.body {
            Body::Text(text) => Some(text),
            _ => None,
        }
    }

    /// Lists the current folder's tree while the Files panel is on screen:
    /// its root the first time, and its open folders again when it's opened
    /// or out of date.
    pub(super) fn follow_files(&mut self) -> Task<Message> {
        // Nothing can be listed before the server connects; listing then
        // counts as opening the panel.
        if self.client.is_none() {
            self.fs.visible = false;
            return Task::none();
        }
        let visible = self.panel_shown(Panel::Files);
        let opened = visible && !self.fs.visible;
        self.fs.visible = visible;
        if !visible {
            return Task::none();
        }
        let root = self.folder();
        let stale = self.fs.stale.remove(&root);
        let listed = self
            .fs
            .trees
            .get(&root)
            .is_some_and(|tree| tree.root().is_some());
        if listed && !opened && !stale {
            return Task::none();
        }
        self.relist(&root)
    }

    /// Lists every folder of `root`'s tree listed so far again, or its root.
    fn relist(&mut self, root: &str) -> Task<Message> {
        let tree = self.fs.trees.entry(root.to_owned()).or_default();
        let mut folders = tree.listed_folders();
        if folders.is_empty() {
            folders.push(String::new());
        }
        let folders: Vec<String> = folders.iter().map(|folder| tree.load(folder)).collect();
        Task::batch(
            folders
                .into_iter()
                .map(|folder| self.list(root.to_owned(), folder)),
        )
    }

    /// Marks every tree out of date, as when the window regains focus.
    pub(super) fn files_stale(&mut self) {
        self.fs.stale.extend(self.fs.trees.keys().cloned());
    }

    /// Lists a folder of `root`'s tree, with which entries Git ignores.
    fn list(&self, root: String, folder: String) -> Task<Message> {
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        Task::perform(
            async move {
                let entries = client
                    .list_dir(&root, &folder)
                    .await
                    .map_err(|e| e.to_string());
                // Only folders on this computer can be asked about.
                let ignored = match &entries {
                    Ok(entries) if Path::new(&root).is_dir() => {
                        let paths: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
                        Repo::new(&root).ignored(&paths).await.unwrap_or_default()
                    }
                    _ => HashSet::new(),
                };
                FilesEvent::Listed {
                    root,
                    folder,
                    entries,
                    ignored,
                }
            },
            Message::Files,
        )
    }

    /// Handles a [`FilesEvent`].
    #[expect(clippy::too_many_lines, reason = "one short arm per event")]
    pub(super) fn files_action(&mut self, event: FilesEvent) -> Task<Message> {
        match event {
            FilesEvent::Toggle(path) => {
                let root = self.folder();
                let tree = self.fs.trees.entry(root.clone()).or_default();
                if let Some(folder) = tree.toggle(&path) {
                    return self.list(root, folder);
                }
            }
            FilesEvent::Refresh => return self.relist(&self.folder()),
            FilesEvent::Listed {
                root,
                folder,
                entries,
                ignored,
            } => {
                if let Some(tree) = self.fs.trees.get_mut(&root) {
                    tree.listed(&folder, entries, &ignored);
                }
            }
            FilesEvent::Open(path) => return self.open_file(self.folder(), path),
            FilesEvent::Read { key, bytes } => return self.read(&key, bytes),
            FilesEvent::Reread { key, bytes } => return self.reread(&key, bytes),
            FilesEvent::Show(key) => {
                if let Some(Body::Text(text)) = self.file(&key).map(|f| &f.body) {
                    text.editor.request_focus();
                }
                self.fs.shown = Some(key);
            }
            FilesEvent::Editor(key, EditorMessage::WriteRequested) => {
                return self.files_action(FilesEvent::Save(key));
            }
            FilesEvent::Editor(key, message) => {
                let Some(text) = self.text_mut(&key) else {
                    return Task::none();
                };
                let task = text.editor.update(&message);
                if !matches!(message, EditorMessage::Tick) {
                    text.dirty = text.editor.content() != text.saved;
                }
                return task.map(move |m| Message::Files(FilesEvent::Editor(key.clone(), m)));
            }
            FilesEvent::Save(key) => return self.save(&key, false),
            FilesEvent::SaveAndClose(key) => return self.save(&key, true),
            FilesEvent::Saved { key, text, result } => {
                let Some(file) = self.text_mut(&key) else {
                    return Task::none();
                };
                file.saving = false;
                match result {
                    Ok(()) => {
                        file.dirty = file.editor.content() != text;
                        file.saved = text;
                        file.outside = None;
                        file.error = None;
                        if file.close_after_save && !file.dirty {
                            return self.finish_closing(&key);
                        }
                    }
                    Err(error) => file.error = Some(error),
                }
                file.close_after_save = false;
            }
            FilesEvent::Close(key) => {
                let dirty = self.file(&key).is_some_and(OpenFile::is_dirty);
                if !dirty {
                    return self.close_file(&key);
                }
                if let Some(file) = self.file_mut(&key) {
                    file.closing = true;
                }
                self.fs.shown = Some(key);
            }
            FilesEvent::Discard(key) => return self.finish_closing(&key),
            FilesEvent::KeepOpen(key) => {
                if let Some(file) = self.file_mut(&key) {
                    file.closing = false;
                    file.then_open = None;
                }
            }
            FilesEvent::Reload(key) => {
                let Some(file) = self.text_mut(&key) else {
                    return Task::none();
                };
                match file.outside.take() {
                    Some(Outside::Changed(text)) => {
                        let task = file.editor.reset(&text);
                        file.saved = text;
                        file.dirty = false;
                        return task
                            .map(move |m| Message::Files(FilesEvent::Editor(key.clone(), m)));
                    }
                    Some(Outside::Removed) | None => {}
                }
            }
            FilesEvent::KeepMine(key) => {
                if let Some(file) = self.text_mut(&key) {
                    file.outside = None;
                }
            }
            FilesEvent::Blink => {
                if let Some(key) = self.fs.shown.clone()
                    && let Some(text) = self.text_mut(&key)
                {
                    return text
                        .editor
                        .update(&EditorMessage::Tick)
                        .map(move |m| Message::Files(FilesEvent::Editor(key.clone(), m)));
                }
            }
        }
        Task::none()
    }

    /// Opens `path` in `folder` in the editor tab, in place of the file
    /// there; one with unsaved changes asks first.
    fn open_file(&mut self, folder: String, path: String) -> Task<Message> {
        let key = Path::new(&folder)
            .join(&path)
            .to_string_lossy()
            .into_owned();
        if let Some(file) = self.file(&key) {
            if let Body::Text(text) = &file.body {
                text.editor.request_focus();
            }
            self.fs.shown = Some(key);
            return Task::none();
        }
        // There's one editor tab: the file in it makes way.
        let mut closed = Task::none();
        if let Some(index) = self.fs.open.iter().position(|f| f.key != key) {
            let current = &mut self.fs.open[index];
            if current.is_dirty() {
                current.closing = true;
                current.then_open = Some((folder, path));
                self.fs.shown = Some(current.key.clone());
                return Task::none();
            }
            let current = current.key.clone();
            closed = self.close_file(&current);
        }
        self.fs.shown = Some(key.clone());
        self.fs.open.push(OpenFile {
            key: key.clone(),
            folder: folder.clone(),
            path: path.clone(),
            body: Body::Loading,
            closing: false,
            then_open: None,
        });
        let Some(client) = self.client.clone() else {
            return closed;
        };
        let watcher = client.clone();
        let (watch_folder, watch_path) = (folder.clone(), path.clone());
        Task::batch([
            Task::perform(
                async move {
                    let bytes = client
                        .read_file(&folder, &path)
                        .await
                        .map_err(|e| e.to_string());
                    FilesEvent::Read { key, bytes }
                },
                Message::Files,
            ),
            // Without the watch, the tab wouldn't hear of the agent's edits.
            Task::future(async move {
                let _ = watcher.watch_file(&watch_folder, &watch_path).await;
            })
            .discard(),
            closed,
        ])
    }

    /// Fills a tab with the file it opened.
    fn read(&mut self, key: &str, bytes: Result<Vec<u8>, String>) -> Task<Message> {
        let theme = self.theme.clone();
        let Some(file) = self.file_mut(key) else {
            return Task::none();
        };
        file.body = match bytes {
            Err(error) => Body::Failed(error),
            Ok(bytes) => match files::classify(&file.path, bytes.clone()) {
                Kind::Text(text) => {
                    let mut editor = CodeEditor::new(&text, &files::syntax_of(&file.path));
                    editor.set_theme(iced_code_editor::from_iced_theme(&theme));
                    editor.set_font(style::MONO);
                    editor.set_font_size(EDITOR_TEXT, true);
                    editor.set_reveal_in_file_manager_enabled(false);
                    editor.request_focus();
                    Body::Text(Box::new(TextFile {
                        editor,
                        saved: text,
                        dirty: false,
                        saving: false,
                        close_after_save: false,
                        outside: None,
                        error: None,
                    }))
                }
                Kind::Image => Body::Image(image::Handle::from_bytes(bytes)),
                Kind::Svg => Body::Svg(svg::Handle::from_memory(bytes)),
                Kind::Binary => Body::Binary,
                Kind::TooLarge(size) => Body::TooLarge(size),
            },
        };
        Task::none()
    }

    /// Takes what's on disk after a change: quietly when the tab has no
    /// unsaved changes, otherwise by saying so.
    fn reread(&mut self, key: &str, bytes: Result<Vec<u8>, String>) -> Task<Message> {
        let Some(file) = self.file(key) else {
            return Task::none();
        };
        if !matches!(file.body, Body::Text(_)) {
            // A picture or a file that couldn't open is simply read again.
            return self.read(key, bytes);
        }
        let now = match bytes {
            Ok(bytes) => match files::classify(&file.path, bytes) {
                Kind::Text(text) => Outside::Changed(text),
                _ => return Task::none(),
            },
            Err(_) => Outside::Removed,
        };
        let key = key.to_owned();
        let Some(text) = self.text_mut(&key) else {
            return Task::none();
        };
        match now {
            // Its own save, coming back.
            Outside::Changed(now) if now == text.saved => {}
            Outside::Changed(now) if !text.dirty => {
                let task = text.editor.reset(&now);
                text.saved = now;
                return task.map(move |m| Message::Files(FilesEvent::Editor(key.clone(), m)));
            }
            outside => text.outside = Some(outside),
        }
        Task::none()
    }

    /// Writes an open file's text.
    fn save(&mut self, key: &str, then_close: bool) -> Task<Message> {
        let client = self.client.clone();
        let Some(file) = self.file_mut(key) else {
            return Task::none();
        };
        let (folder, path) = (file.folder.clone(), file.path.clone());
        let Body::Text(text) = &mut file.body else {
            return Task::none();
        };
        let Some(client) = client else {
            text.error = Some("Not connected to the server.".to_owned());
            return Task::none();
        };
        if text.saving {
            return Task::none();
        }
        text.saving = true;
        text.close_after_save = then_close;
        let content = text.editor.content();
        let key = key.to_owned();
        Task::perform(
            async move {
                let result = client
                    .write_file(&folder, &path, content.clone().into_bytes())
                    .await
                    .map_err(|e| e.to_string());
                FilesEvent::Saved {
                    key,
                    text: content,
                    result,
                }
            },
            Message::Files,
        )
    }

    /// Closes a file's tab once the user saved or dropped its changes, and
    /// opens the file that was waiting for it.
    fn finish_closing(&mut self, key: &str) -> Task<Message> {
        let next = self.file_mut(key).and_then(|f| f.then_open.take());
        let closed = self.close_file(key);
        match next {
            Some((folder, path)) => Task::batch([closed, self.open_file(folder, path)]),
            None => closed,
        }
    }

    /// Closes a file's tab; the file beside it, or the session, takes its place.
    fn close_file(&mut self, key: &str) -> Task<Message> {
        let Some(index) = self.fs.open.iter().position(|f| f.key == key) else {
            return Task::none();
        };
        let file = self.fs.open.remove(index);
        if self.fs.shown.as_deref() == Some(key) {
            let next = index.min(self.fs.open.len().saturating_sub(1));
            self.fs.shown = self.fs.open.get(next).map(|f| f.key.clone());
            if let Some(Body::Text(text)) = self.shown_file().map(|f| &f.body) {
                text.editor.request_focus();
            }
        }
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        Task::future(async move {
            let _ = client.unwatch_file(&file.folder, &file.path).await;
        })
        .discard()
    }

    /// Follows changes on disk: lists affected folders again, and rereads
    /// open files that changed.
    pub(super) fn files_event(&mut self, kind: &EventKind) -> Task<Message> {
        // The server reports changes only to open files; a file an agent
        // adds or deletes shows when its tool ends.
        let touches = match kind {
            EventKind::ToolEnded { files, .. } => !files.is_empty(),
            EventKind::Done { .. }
            | EventKind::RevertCommitted { .. }
            | EventKind::RevertCleared { .. } => true,
            _ => false,
        };
        if touches
            && let Some(session) = kind.session_id().and_then(|id| self.workspace.session(id))
            && self.fs.trees.contains_key(&session.directory)
        {
            self.fs.stale.insert(session.directory.clone());
        }
        let EventKind::FileChanged {
            directory,
            file,
            change,
        } = kind
        else {
            return Task::none();
        };
        let absolute = if Path::new(file).is_absolute() {
            file.clone()
        } else {
            Path::new(directory)
                .join(file)
                .to_string_lossy()
                .into_owned()
        };
        let mut tasks = Vec::new();
        // The tree lists additions and removals; changed content doesn't move it.
        if *change != FileChange::Changed {
            for (root, tree) in &mut self.fs.trees {
                let Ok(relative) = Path::new(&absolute).strip_prefix(root) else {
                    continue;
                };
                let relative = relative.to_string_lossy().into_owned();
                for folder in tree.affected(&relative) {
                    let folder = tree.load(&folder);
                    tasks.push((root.clone(), folder));
                }
            }
        }
        let mut tasks: Vec<Task<Message>> =
            tasks.into_iter().map(|(r, f)| self.list(r, f)).collect();
        if let (Some(open), Some(client)) = (self.file(&absolute), self.client.clone()) {
            let (folder, path, key) = (open.folder.clone(), open.path.clone(), open.key.clone());
            tasks.push(Task::perform(
                async move {
                    let bytes = client
                        .read_file(&folder, &path)
                        .await
                        .map_err(|e| e.to_string());
                    FilesEvent::Reread { key, bytes }
                },
                Message::Files,
            ));
        }
        Task::batch(tasks)
    }
}
