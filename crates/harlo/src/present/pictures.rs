//! Pictures in the window: thumbnails in one shared list that rows point
//! into, the session's grid of pictures and the viewer.
//!
//! Rows name a thumbnail by its place in `State.thumbs`, so a thumbnail
//! decoded after its row was built fills in without the row changing.

// Rust guideline compliant 2026-10-09

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use slint::{Image, Model as _, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};

use crate::app::{App, Pixels, Sent, Thumb as Decoded};
use crate::ui::{State, Thumb};

/// How many thumbnails the list holds before it starts again.
const SLOTS: usize = 300;

/// The thumbnails' list, with each picture's place in it.
pub struct Slots {
    images: Rc<VecModel<Image>>,
    places: HashMap<u64, usize>,
    /// The pictures whose thumbnails are in their place.
    filled: HashSet<u64>,
    /// Counts fresh starts; rows built before one point at nothing.
    generation: u64,
}

impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slots")
            .field("places", &self.places.len())
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl Default for Slots {
    fn default() -> Self {
        Self {
            images: Rc::new(VecModel::default()),
            places: HashMap::new(),
            filled: HashSet::new(),
            generation: 0,
        }
    }
}

impl Slots {
    /// The list the window draws thumbnails from.
    #[must_use]
    pub fn model(&self) -> ModelRc<Image> {
        ModelRc::from(Rc::clone(&self.images))
    }

    /// Counts fresh starts: rows built in an earlier one need building again.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Starts again when the list is full, before rows are built.
    pub fn trim(&mut self) {
        if self.places.len() >= SLOTS {
            self.images.set_vec(Vec::new());
            self.places.clear();
            self.filled.clear();
            self.generation += 1;
        }
    }

    /// The place of the picture with `key`, made when it has none yet.
    pub fn place(&mut self, key: u64) -> i32 {
        let next = self.places.len();
        let place = *self.places.entry(key).or_insert_with(|| {
            self.images.push(Image::default());
            next
        });
        i32::try_from(place).unwrap_or(-1)
    }

    /// A row's thumbnail of `picture`, sent by the user or not.
    pub fn thumb(&mut self, picture: &harlo_core::server::Picture, mine: bool) -> Thumb {
        Thumb {
            key: picture.key.to_string().into(),
            slot: self.place(picture.key),
            name: picture.name.as_str().into(),
            mine,
        }
    }

    /// Puts each thumbnail decoded since into its place.
    pub fn fill(&mut self, thumbs: &HashMap<u64, Decoded>) {
        for (key, place) in &self.places {
            if self.filled.contains(key) {
                continue;
            }
            if let Some(Decoded::Ready(pixels)) = thumbs.get(key) {
                self.images.set_row_data(*place, image(pixels));
                self.filled.insert(*key);
            }
        }
    }
}

/// `pixels` as an image the window draws.
#[must_use]
pub fn image(pixels: &Pixels) -> Image {
    Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        &pixels.rgba,
        pixels.width,
        pixels.height,
    ))
}

/// `pixels` turned `turns` quarter turns clockwise, as an image.
fn turned(pixels: &Pixels, turns: u8) -> Image {
    if turns == 0 {
        return image(pixels);
    }
    let Some(picture) =
        image::RgbaImage::from_raw(pixels.width, pixels.height, pixels.rgba.clone())
    else {
        return image(pixels);
    };
    let picture = match turns {
        1 => image::imageops::rotate90(&picture),
        2 => image::imageops::rotate180(&picture),
        _ => image::imageops::rotate270(&picture),
    };
    Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        picture.as_raw(),
        picture.width(),
        picture.height(),
    ))
}

/// The session's grid and the viewer, kept between presents.
#[derive(Default)]
pub struct Pictures {
    grid: Rc<VecModel<Thumb>>,
    /// The grid's pictures, by key, as last shown.
    listed: Vec<u64>,
    /// The viewer's picture, decoded whole and turned, by key and turns.
    shown: Option<((u64, u8), Image)>,
}

impl std::fmt::Debug for Pictures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pictures")
            .field("listed", &self.listed.len())
            .finish_non_exhaustive()
    }
}

impl Pictures {
    /// Gives `state` the grid's list.
    pub fn bind(&self, state: &State<'_>) {
        state.set_session_pictures(ModelRc::from(Rc::clone(&self.grid)));
    }

    /// Shows the open session's grid and the viewer as `app` has them.
    pub fn show(&mut self, app: &App, state: &State<'_>, slots: &mut Slots) {
        let pictures = &app.pictures;
        let sent: Vec<Sent> = (app.active.as_deref())
            .map(|id| app.session_pictures(id))
            .unwrap_or_default();
        let keys: Vec<u64> = sent.iter().map(|s| s.picture.key).collect();
        if keys != self.listed || self.grid.row_count() != keys.len() {
            let thumbs: Vec<Thumb> = (sent.iter())
                .map(|s| slots.thumb(&s.picture, s.mine))
                .collect();
            self.grid.set_vec(thumbs);
            self.listed = keys;
        }
        state.set_pictures_count(i32::try_from(sent.len()).unwrap_or(i32::MAX));
        state.set_pictures_grid(pictures.grid);

        let Some(viewing) = &pictures.viewing else {
            state.set_viewer_open(false);
            self.shown = None;
            return;
        };
        let key = viewing.sent.picture.key;
        let look = (key, viewing.turns);
        let image = match (&self.shown, &viewing.pixels) {
            (Some((shown, image)), _) if *shown == look => image.clone(),
            (_, Some(pixels)) => {
                let image = turned(pixels, viewing.turns);
                self.shown = Some((look, image.clone()));
                image
            }
            // The thumbnail stands in while the whole picture decodes.
            (_, None) => match pictures.thumbs.get(&key) {
                Some(Decoded::Ready(pixels)) => image(pixels),
                _ => Image::default(),
            },
        };
        let size = (viewing.pixels.as_ref())
            .map(|p| {
                if viewing.turns % 2 == 1 {
                    (p.height, p.width)
                } else {
                    (p.width, p.height)
                }
            })
            .unwrap_or_default();
        state.set_viewer_open(true);
        state.set_viewer_image(image);
        state.set_viewer_width(i32::try_from(size.0).unwrap_or(0));
        state.set_viewer_height(i32::try_from(size.1).unwrap_or(0));
        state.set_viewer_name(viewing.sent.picture.name.as_str().into());
        state.set_viewer_from(
            if viewing.sent.mine {
                "You sent"
            } else {
                "The agent sent"
            }
            .into(),
        );
        let note = if viewing.failed {
            "This picture can't be shown."
        } else if let Some(why) = &pictures.error {
            why.as_str()
        } else {
            ""
        };
        state.set_viewer_note(note.into());
    }
}
