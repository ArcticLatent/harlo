//! The Files panel's tree and the editor tab: the open file's text with its
//! line numbers and colours, its picture, or why it doesn't show.
//!
//! The text is coloured again only when it changes, and not at all past a
//! size where that would slow typing down.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use std::rc::Rc;

use harlo_core::files::{Listing, name_of};
use harlo_core::theme::Tokens;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer, SharedString, StyledText, VecModel};

use super::{sync, timeline};
use crate::app::{App, FileBody, OpenFile, Outside, Panel};
use crate::file_icons;
use crate::markdown::escaped;
use crate::ui::{EditorCard, FileRow, State};

/// The most a file can hold and still be coloured as it's typed in.
const COLOURED_BYTES: usize = 256 * 1024;

/// The Files panel's tree and what the editor last showed.
pub struct Files {
    rows: Rc<VecModel<FileRow>>,
    /// The file and text revision the field last loaded.
    loaded: Option<(String, u64)>,
    /// Counts the times the field was given new text.
    revision: i32,
    /// The text last coloured, and the theme's tokens it was coloured
    /// with, so unchanged text isn't coloured again.
    coloured: (String, Option<Tokens>),
    /// The line count the numbers were last written for.
    lines: usize,
    /// The file whose picture shows.
    picture: Option<String>,
}

impl std::fmt::Debug for Files {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Files")
            .field("loaded", &self.loaded)
            .finish_non_exhaustive()
    }
}

impl Files {
    /// Gives the window the tree's list.
    pub fn new(state: &State<'_>) -> Self {
        let rows = Rc::new(VecModel::default());
        state.set_file_rows(Rc::clone(&rows).into());
        Self {
            rows,
            loaded: None,
            revision: 0,
            coloured: (String::new(), None),
            lines: 0,
            picture: None,
        }
    }

    /// Fills the Files panel, when it's on screen.
    pub fn tree(&self, app: &App, state: &State<'_>) {
        if !app.panel_shown(Panel::Files) {
            return;
        }
        let (folder, tree) = app.file_tree();
        let name = folder
            .rsplit('/')
            .find(|part| !part.is_empty())
            .unwrap_or("/");
        state.set_files_folder(name.into());
        let words = match tree.and_then(|tree| tree.root()) {
            None | Some(Listing::Loading) => "Listing the folder…".to_owned(),
            Some(Listing::Failed(error)) => format!("Couldn't list it: {error}"),
            Some(Listing::Loaded(nodes)) if nodes.is_empty() => "This folder is empty.".to_owned(),
            Some(Listing::Loaded(_)) => String::new(),
        };
        state.set_files_note(words.into());
        let shown = app.fs.open.first().map(|f| f.key.as_str());
        let rows = tree
            .map(harlo_core::files::Tree::rows)
            .unwrap_or_default()
            .into_iter()
            .map(|line| {
                let node = line.node;
                let key = std::path::Path::new(&folder).join(&node.path);
                let icon = file_icons::icon(&node.name);
                FileRow {
                    selected: shown == key.to_str(),
                    name: node.name.as_str().into(),
                    depth: i32::try_from(line.depth).unwrap_or(i32::MAX),
                    directory: node.directory,
                    open: line.open,
                    loading: line.loading,
                    ignored: node.ignored,
                    // A plain file shows a plain page.
                    icon: if file_icons::plain(&node.name) {
                        SharedString::new()
                    } else {
                        icon.glyph.to_string().into()
                    },
                    path: node.path.into(),
                }
            })
            .collect();
        sync(&self.rows, rows);
    }

    /// Fills the editor tab, when it shows a file.
    ///
    /// `timeline` lends its code colourer.
    pub fn editor(&mut self, app: &App, state: &State<'_>, timeline: &mut timeline::Rows) {
        let Some(file) = app.fs.open.first() else {
            self.loaded = None;
            self.picture = None;
            return;
        };
        state.set_editor_card(card(file));
        match &file.body {
            FileBody::Text(text) => {
                let now = (file.key.clone(), text.editor.revision());
                if self.loaded.as_ref() != Some(&now) {
                    self.loaded = Some(now);
                    self.revision = self.revision.wrapping_add(1);
                    state.set_editor_text(text.editor.as_str().into());
                    state.set_editor_cursor(offset(text.editor.offset()));
                    state.set_editor_anchor(offset(text.editor.anchor()));
                    state.set_editor_revision(self.revision);
                } else if state.get_editor_text().as_str() != text.editor.as_str() {
                    // Typed text stays in the field without a reload, but a
                    // field made afresh starts from these.
                    state.set_editor_text(text.editor.as_str().into());
                    state.set_editor_cursor(offset(text.editor.offset()));
                    state.set_editor_anchor(offset(text.editor.anchor()));
                }
                let source = text.editor.as_str();
                if source != self.coloured.0 || self.coloured.1.as_ref() != Some(&app.tokens) {
                    source.clone_into(&mut self.coloured.0);
                    self.coloured.1 = Some(app.tokens);
                    state.set_editor_colored(colour(source, &file.path, &app.tokens, timeline));
                }
                let lines = source.split('\n').count();
                if lines != self.lines {
                    self.lines = lines;
                    state.set_editor_numbers(numbers(lines).into());
                }
                self.picture = None;
            }
            FileBody::Image(bytes) | FileBody::Svg(bytes) => {
                if self.picture.as_deref() != Some(file.key.as_str()) {
                    self.picture = Some(file.key.clone());
                    let svg = matches!(file.body, FileBody::Svg(_));
                    state.set_editor_picture(picture(bytes, svg));
                }
            }
            _ => self.picture = None,
        }
    }
}

/// What the editor says about `file`, and what it shows.
fn card(file: &OpenFile) -> EditorCard {
    let mut card = EditorCard {
        path: file.path.as_str().into(),
        ..EditorCard::default()
    };
    match &file.body {
        FileBody::Loading => card.note = "Opening…".into(),
        FileBody::Text(text) => {
            card.body = 1;
            card.text = true;
            card.can_save = text.dirty && !text.saving;
            if let Some(error) = &text.error {
                card.status = format!("Couldn't save: {error}").into();
                card.failed = true;
            } else if text.saving {
                card.status = "Saving…".into();
            } else if text.dirty {
                card.status = "Unsaved changes".into();
            }
            card.outside = match text.outside {
                Some(Outside::Changed(_)) => 1,
                Some(Outside::Removed) => 2,
                None => 0,
            };
            if file.closing {
                card.closing = match &file.then_open {
                    Some((_, next)) => format!(
                        "Save your changes to {} before opening {}?",
                        file.name(),
                        name_of(next)
                    ),
                    None => format!("Save your changes to {} before closing?", file.name()),
                }
                .into();
            }
        }
        FileBody::Image(_) | FileBody::Svg(_) => card.body = 2,
        FileBody::Binary => card.note = "This file isn't text, so it can't be edited here.".into(),
        FileBody::TooLarge(bytes) => {
            card.note = format!(
                "This file is {}, too large to open here.",
                megabytes(*bytes)
            )
            .into();
        }
        FileBody::Failed(error) => card.note = format!("Couldn't open it: {error}").into(),
    }
    card
}

/// `source` coloured for the language its path names; plain when it's
/// large, or has tabs, whose width the coloured copy can't match.
fn colour(source: &str, path: &str, t: &Tokens, timeline: &mut timeline::Rows) -> StyledText {
    let language = path.rsplit_once('.').map_or("", |(_, extension)| extension);
    let markup = if source.len() > COLOURED_BYTES || source.contains('\t') {
        escaped(source)
    } else {
        timeline.highlighter(t).markup(language, source)
    };
    StyledText::from_markdown(&markup).unwrap_or_else(|_| StyledText::from_plain_text(source))
}

/// The line numbers from 1 to `lines`, one a line.
fn numbers(lines: usize) -> String {
    let mut out = String::with_capacity(lines * 5);
    for n in 1..=lines {
        if n > 1 {
            out.push('\n');
        }
        let _ = write!(out, "{n}");
    }
    out
}

/// A picture from its file's bytes, or an empty one when it can't be read.
fn picture(bytes: &[u8], svg: bool) -> Image {
    if svg {
        return Image::load_from_svg_data(bytes).unwrap_or_default();
    }
    let Ok(decoded) = image::load_from_memory(bytes) else {
        return Image::default();
    };
    let rgba = decoded.into_rgba8();
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        rgba.as_raw(),
        rgba.width(),
        rgba.height(),
    );
    Image::from_rgba8(buffer)
}

/// A size in megabytes, such as "12.5 MB".
fn megabytes(bytes: usize) -> String {
    /// Bytes in a megabyte.
    const MB: usize = 1024 * 1024;
    let tenths = bytes * 10 / MB;
    format!("{}.{} MB", tenths / 10, tenths % 10)
}

/// A byte offset as the UI's `int`.
fn offset(at: usize) -> i32 {
    i32::try_from(at).unwrap_or(i32::MAX)
}
