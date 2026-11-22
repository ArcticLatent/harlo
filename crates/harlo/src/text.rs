//! Editable text kept on the Rust side: the composer's, a note's.
//!
//! The window's text field owns what the user types and reports each edit
//! with [`TextBox::edited`]. When the app changes the text itself (a menu
//! pick, a paste, clearing after a send) the [revision](TextBox::revision)
//! goes up, and the window copies the text back into the field. Typing never
//! bumps it, so the field isn't overwritten under the user's hands.

// Rust guideline compliant 2026-10-09

/// A place in the text: a line and a byte offset in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    /// The line, from 0.
    pub line: usize,
    /// The byte offset in the line.
    pub column: usize,
}

/// The cursor, and where a selection started if there is one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursor {
    /// Where the cursor is.
    pub position: Position,
    /// The other end of the selection.
    pub selection: Option<Position>,
}

/// An edit the window reports: the whole text, and the cursor's and the
/// selection anchor's byte offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// The text after the edit.
    pub text: String,
    /// The cursor's byte offset in it.
    pub cursor: usize,
    /// The selection's other end; the cursor's offset when nothing is selected.
    pub anchor: usize,
}

impl Edit {
    /// An edit leaving the cursor at `cursor`, with nothing selected.
    #[must_use]
    pub fn at(text: impl Into<String>, cursor: usize) -> Self {
        Self {
            text: text.into(),
            cursor,
            anchor: cursor,
        }
    }
}

/// Text being edited, with its cursor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextBox {
    text: String,
    /// The cursor's byte offset.
    cursor: usize,
    /// The selection's other end, as a byte offset.
    anchor: Option<usize>,
    /// Counts the app's own changes.
    revision: u64,
}

impl TextBox {
    /// Empty text.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `text`, with the cursor at its end.
    #[must_use]
    pub fn with_text(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            cursor: text.len(),
            anchor: None,
            revision: 1,
        }
    }

    /// The text.
    #[must_use]
    pub fn text(&self) -> String {
        self.text.clone()
    }

    /// The text, borrowed.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Line `index`, without its line break.
    #[must_use]
    pub fn line(&self, index: usize) -> Option<&str> {
        self.text.split('\n').nth(index)
    }

    /// The cursor's byte offset in the whole text.
    #[must_use]
    pub fn offset(&self) -> usize {
        self.cursor
    }

    /// The selection's other end as a byte offset; the cursor's when
    /// nothing is selected.
    #[must_use]
    pub fn anchor(&self) -> usize {
        self.anchor.unwrap_or(self.cursor)
    }

    /// The cursor and selection, by line and column.
    #[must_use]
    pub fn cursor(&self) -> Cursor {
        Cursor {
            position: self.position(self.cursor),
            selection: self.anchor.map(|a| self.position(a)),
        }
    }

    /// Counts the app's own changes; the window copies the text when it
    /// moves on.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Takes an edit the user made in the window.
    pub fn edited(&mut self, edit: Edit) {
        self.cursor = floor_char(&edit.text, edit.cursor);
        let anchor = floor_char(&edit.text, edit.anchor);
        self.anchor = (anchor != self.cursor).then_some(anchor);
        self.text = edit.text;
    }

    /// The selected text, if any.
    #[must_use]
    pub fn selection(&self) -> Option<String> {
        let anchor = self.anchor?;
        let (start, end) = (anchor.min(self.cursor), anchor.max(self.cursor));
        self.text.get(start..end).map(str::to_owned)
    }

    /// Replaces the text, putting the cursor at its end.
    pub fn set(&mut self, text: &str) {
        text.clone_into(&mut self.text);
        self.cursor = text.len();
        self.anchor = None;
        self.revision += 1;
    }

    /// Moves the cursor, and the selection's other end.
    pub fn move_to(&mut self, cursor: Cursor) {
        self.cursor = self.offset_of(cursor.position);
        self.anchor = cursor.selection.map(|p| self.offset_of(p));
        self.revision += 1;
    }

    /// Moves the cursor to the end of the text.
    pub fn move_to_end(&mut self) {
        self.cursor = self.text.len();
        self.anchor = None;
        self.revision += 1;
    }

    /// Puts `insert` in place of the selection, or at the cursor, leaving the
    /// cursor after it.
    pub fn paste(&mut self, insert: &str) {
        let (start, end) = match self.anchor {
            Some(a) => (a.min(self.cursor), a.max(self.cursor)),
            None => (self.cursor, self.cursor),
        };
        self.text.replace_range(start..end, insert);
        self.cursor = start + insert.len();
        self.anchor = None;
        self.revision += 1;
    }

    /// The line and column of byte offset `at`.
    fn position(&self, at: usize) -> Position {
        let before = &self.text[..at.min(self.text.len())];
        let line = before.matches('\n').count();
        let column = before
            .rfind('\n')
            .map_or(before.len(), |nl| before.len() - nl - 1);
        Position { line, column }
    }

    /// The byte offset of a line and column, kept inside the text.
    fn offset_of(&self, position: Position) -> usize {
        let mut start = 0;
        for (index, line) in self.text.split('\n').enumerate() {
            if index == position.line {
                return floor_char(&self.text, start + position.column.min(line.len()));
            }
            start += line.len() + 1;
        }
        self.text.len()
    }
}

/// `at`, moved back to the start of the character it falls in.
fn floor_char(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}
