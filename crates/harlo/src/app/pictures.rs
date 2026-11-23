//! Pictures in a session: the ones the user sent and the ones the agent's
//! tools returned, as thumbnails, in a grid, and one at a time in the viewer.
//!
//! Pictures are decoded off the window's thread: thumbnails once for each
//! picture shown, the whole picture while the viewer shows it.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use harlo_core::server::Picture;
use harlo_core::timeline::Entry;

use super::{App, Message};
use crate::task::{Effect, Task};

/// The longest side a thumbnail is decoded at, in pixels: twice the
/// largest it's drawn, for sharp edges on dense screens.
const THUMB_SIDE: u32 = 240;

/// How many thumbnails are kept before the ones not in use are let go.
const KEPT_THUMBS: usize = 400;

/// Something that happened to pictures.
#[derive(Debug, Clone)]
pub enum PictureEvent {
    /// A thumbnail was decoded, or couldn't be.
    Thumbed(u64, Option<Arc<Pixels>>),
    /// Show the picture with this key in the viewer.
    View(u64),
    /// The viewer's picture was decoded whole, or couldn't be.
    Decoded(u64, Option<Arc<Pixels>>),
    /// Close the viewer, or the grid when the viewer isn't open.
    Close,
    /// Turn the viewer's picture a quarter turn, clockwise or not.
    Turn(bool),
    /// Show the session's pictures in a grid, or hide them.
    ToggleGrid,
    /// Show the viewer's picture in its folder.
    ShowInFolder,
    /// The picture was saved where it can be shown, or why it couldn't be.
    Saved(Result<PathBuf, String>),
}

/// A decoded picture: its size and its pixels, four bytes each.
#[derive(Clone, PartialEq, Eq)]
pub struct Pixels {
    /// Its width, in pixels.
    pub width: u32,
    /// Its height, in pixels.
    pub height: u32,
    /// Red, green, blue and alpha for each pixel, row by row.
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Pixels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pixels({}×{})", self.width, self.height)
    }
}

/// Where a picture's thumbnail stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Thumb {
    /// Being decoded.
    Decoding,
    /// Decoded.
    Ready(Arc<Pixels>),
    /// It couldn't be decoded.
    Failed,
}

/// A picture in a session, with who sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    /// The picture.
    pub picture: Picture,
    /// Whether the user sent it; otherwise a tool returned it.
    pub mine: bool,
}

/// The picture the viewer shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewing {
    /// The picture, with who sent it.
    pub sent: Sent,
    /// The whole picture, once decoded; `None` while it is, or when it
    /// can't be.
    pub pixels: Option<Arc<Pixels>>,
    /// Whether decoding it failed.
    pub failed: bool,
    /// How many quarter turns clockwise it's shown at, from 0 to 3.
    pub turns: u8,
}

/// What the window knows of pictures.
#[derive(Debug)]
pub struct PicturesState {
    /// Where pictures are saved to be shown in their folder; `None` without
    /// a folder for cached files.
    pub folder: Option<PathBuf>,
    /// Each picture's thumbnail, by its key.
    pub thumbs: HashMap<u64, Thumb>,
    /// The picture the viewer shows, if it's open.
    pub viewing: Option<Viewing>,
    /// Whether the session's pictures show in a grid.
    pub grid: bool,
    /// Why the last picture couldn't be shown in its folder.
    pub error: Option<String>,
    /// The session they're of.
    session: Option<String>,
}

impl Default for PicturesState {
    fn default() -> Self {
        Self {
            folder: dirs::cache_dir().map(|cache| cache.join("harlo").join("pictures")),
            thumbs: HashMap::new(),
            viewing: None,
            grid: false,
            error: None,
            session: None,
        }
    }
}

impl App {
    /// The pictures in session `id`, in the order they were sent.
    #[must_use]
    pub fn session_pictures(&self, id: &str) -> Vec<Sent> {
        let Some(timeline) = self.timeline(id) else {
            return Vec::new();
        };
        timeline
            .entries()
            .flat_map(|entry| {
                let (pictures, mine) = match entry {
                    Entry::Prompt { pictures, .. } => (pictures.as_slice(), true),
                    Entry::Tool { pictures, .. } => (pictures.as_slice(), false),
                    _ => (&[][..], false),
                };
                pictures.iter().map(move |picture| Sent {
                    picture: picture.clone(),
                    mine,
                })
            })
            .collect()
    }

    /// Decodes the thumbnails the window is about to show: the open
    /// session's pictures and the draft's.
    pub(super) fn follow_pictures(&mut self) -> Task<Message> {
        // Another session closes the last one's pictures.
        if self.pictures.session != self.active {
            self.pictures.session.clone_from(&self.active);
            self.pictures.viewing = None;
            self.pictures.grid = false;
        }
        let mut shown: Vec<Picture> = (self.draft.attached.iter())
            .filter_map(|a| a.picture.clone())
            .collect();
        if let Some(id) = self.active.clone() {
            shown.extend(self.session_pictures(&id).into_iter().map(|s| s.picture));
        }
        let thumbs = &mut self.pictures.thumbs;
        if thumbs.len() > KEPT_THUMBS {
            thumbs.retain(|key, _| shown.iter().any(|p| p.key == *key));
        }
        let mut seen = std::collections::HashSet::new();
        shown.retain(|picture| !thumbs.contains_key(&picture.key) && seen.insert(picture.key));
        let tasks: Vec<_> = shown
            .into_iter()
            .map(|picture| {
                thumbs.insert(picture.key, Thumb::Decoding);
                let key = picture.key;
                Task::perform(decode(picture.data, Some(THUMB_SIDE)), move |pixels| {
                    Message::Pictures(PictureEvent::Thumbed(key, pixels))
                })
            })
            .collect();
        Task::batch(tasks)
    }

    /// Handles something that happened to pictures.
    pub(super) fn pictures_event(&mut self, event: PictureEvent) -> Task<Message> {
        let state = &mut self.pictures;
        match event {
            PictureEvent::Thumbed(key, pixels) => {
                let thumb = pixels.map_or(Thumb::Failed, Thumb::Ready);
                state.thumbs.insert(key, thumb);
            }
            PictureEvent::View(key) => {
                let sent = self.active.clone().and_then(|id| {
                    let pictures = self.session_pictures(&id);
                    pictures.into_iter().find(|s| s.picture.key == key)
                });
                let sent = sent.or_else(|| {
                    (self.draft.attached.iter())
                        .filter_map(|a| a.picture.clone())
                        .find(|p| p.key == key)
                        .map(|picture| Sent {
                            picture,
                            mine: true,
                        })
                });
                let Some(sent) = sent else {
                    return Task::none();
                };
                let data = Arc::clone(&sent.picture.data);
                let state = &mut self.pictures;
                state.error = None;
                state.viewing = Some(Viewing {
                    sent,
                    pixels: None,
                    failed: false,
                    turns: 0,
                });
                return Task::perform(decode(data, None), move |pixels| {
                    Message::Pictures(PictureEvent::Decoded(key, pixels))
                });
            }
            PictureEvent::Decoded(key, pixels) => {
                if let Some(viewing) = &mut state.viewing
                    && viewing.sent.picture.key == key
                {
                    viewing.failed = pixels.is_none();
                    viewing.pixels = pixels;
                }
            }
            PictureEvent::Turn(clockwise) => {
                if let Some(viewing) = &mut state.viewing {
                    viewing.turns = (viewing.turns + if clockwise { 1 } else { 3 }) % 4;
                }
            }
            PictureEvent::Close => {
                self.close_pictures();
            }
            PictureEvent::ToggleGrid => {
                state.grid = !state.grid;
                state.viewing = None;
            }
            PictureEvent::ShowInFolder => {
                let Some(viewing) = &state.viewing else {
                    return Task::none();
                };
                let picture = viewing.sent.picture.clone();
                let Some(folder) = state.folder.clone() else {
                    state.error = Some("This system has no folder for cached files.".to_owned());
                    return Task::none();
                };
                return Task::perform(save(folder, picture), |saved| {
                    Message::Pictures(PictureEvent::Saved(saved))
                });
            }
            PictureEvent::Saved(Ok(path)) => {
                state.error = None;
                return Task::effect(Effect::ShowInFolder(path));
            }
            PictureEvent::Saved(Err(why)) => state.error = Some(why),
        }
        Task::none()
    }

    /// Whether pictures show over the session: the viewer or the grid.
    #[must_use]
    pub fn pictures_shown(&self) -> bool {
        self.pictures.viewing.is_some() || self.pictures.grid
    }

    /// Closes what pictures show over the session: the viewer first, then
    /// the grid.
    fn close_pictures(&mut self) {
        let state = &mut self.pictures;
        if state.viewing.take().is_none() {
            state.grid = false;
        }
    }
}

/// Decodes `data`, fitted within `side` pixels when given, off the window's
/// thread; `None` when it isn't a picture this build reads.
async fn decode(data: Arc<[u8]>, side: Option<u32>) -> Option<Arc<Pixels>> {
    tokio::task::spawn_blocking(move || {
        let picture = image::load_from_memory(&data).ok()?;
        let picture = match side {
            Some(side) if picture.width().max(picture.height()) > side => {
                picture.thumbnail(side, side)
            }
            _ => picture,
        };
        let rgba = picture.into_rgba8();
        Some(Arc::new(Pixels {
            width: rgba.width(),
            height: rgba.height(),
            rgba: rgba.into_raw(),
        }))
    })
    .await
    .ok()
    .flatten()
}

/// Saves `picture` under `folder`, in a folder of its own and under its
/// own name, and returns where; a picture saved before is found where it was.
async fn save(folder: PathBuf, picture: Picture) -> Result<PathBuf, String> {
    let folder = folder.join(format!("{:016x}", picture.key));
    let name = std::path::Path::new(&picture.name)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("picture")
        .to_owned();
    let path = folder.join(name);
    tokio::task::spawn_blocking(move || {
        if !path.exists() {
            std::fs::create_dir_all(&folder)
                .and_then(|()| std::fs::write(&path, &picture.data))
                .map_err(|e| format!("Couldn't save the picture: {e}"))?;
        }
        Ok(path)
    })
    .await
    .map_err(|e| format!("Couldn't save the picture: {e}"))?
}
