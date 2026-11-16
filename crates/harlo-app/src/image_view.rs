//! The image viewer: `ImageView` (one image, zoomed, panned and turned on
//! the GPU), `ImageThumb` (a folder grid's thumbnail) and `Images` (a
//! folder's images, the caches). Pixels come from worker threads through
//! Qt's image readers (cpp/harlo_image.h) and go up to the GPU once; after
//! that, zooming and panning only move them.
//!
//! - Full images: decoded at full size (an animation's frames all kept),
//!   about 768 MB of them cached, the next and previous ones decoded ahead.
//!   A still much bigger than the screen is kept at the screen's size; its
//!   own pixels are decoded for a view zoomed in past that, and let go when
//!   it's zoomed back out.
//! - Thumbnails: kept in memory (about 192 MB) and on disk under
//!   `$XDG_CACHE_HOME/harlo/thumbnails`, made on every core, the newest
//!   asked for (the ones coming into view) first.

// Rust guideline compliant 2026-10-09

#![expect(clippy::needless_pass_by_value, reason = "the code cxx-qt generates for this file's bridge passes property values by value")]
#![expect(clippy::semicolon_outside_block, reason = "clippy misreads a function the cxx bridge declares as a block")]

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qimage.h");
        type QImage = cxx_qt_lib::QImage;
        include!("cxx-qt-lib/qbytearray.h");
        type QByteArray = cxx_qt_lib::QByteArray;
    }

    #[namespace = "harlo"]
    unsafe extern "C++Qt" {
        include!("harlo_image.h");
        #[qobject]
        type ImageItem;
    }

    #[namespace = "harlo"]
    unsafe extern "C++" {
        fn image_size(path: &QString, width: &mut i32, height: &mut i32);
        fn decode_image(path: &QString, max_side: i32) -> QImage;
        fn decode_thumbnail(path: &QString, side: i32) -> QImage;
        fn load_cached(path: &QString) -> QImage;
        fn save_thumbnail(image: &QImage, path_without_suffix: &QString) -> bool;
        fn encode_for_phone(image: &QImage, quality: i32, png: &mut bool) -> QByteArray;
        fn readable_suffixes() -> QString;

        type FrameReader;
        fn open_frames(path: &QString) -> UniquePtr<FrameReader>;
        fn animated(self: &FrameReader) -> bool;
        fn frame_count(self: &FrameReader) -> i32;
        fn next_frame(self: Pin<&mut FrameReader>, delay_ms: &mut i32) -> QImage;
    }

    extern "RustQt" {
        #[qobject]
        #[base = ImageItem]
        #[qml_element]
        /// The file shown (a local path).
        #[qproperty(QString, path, READ, WRITE = set_path, NOTIFY)]
        /// "" (none), "loading", "preview" (a smaller copy, more on its way:
        /// the picture, or its own pixels for the zoom), "ready" or "error".
        #[qproperty(QString, status)]
        #[qproperty(QString, error)]
        /// The image's size (a photo's rotation applied); -1 unknown.
        #[qproperty(i32, image_width, cxx_name = "imageWidth")]
        #[qproperty(i32, image_height, cxx_name = "imageHeight")]
        /// Screen pixels to an image pixel: 1 is actual size.
        #[qproperty(f64, zoom)]
        /// Fitted to the view (follows its size); never larger than actual size.
        #[qproperty(bool, fit)]
        /// Turned clockwise: 0, 90, 180 or 270.
        #[qproperty(i32, turn)]
        /// An animation's frames (1 for a still), the one shown, and
        /// whether it plays.
        #[qproperty(i32, frame_count, cxx_name = "frameCount")]
        #[qproperty(i32, frame)]
        #[qproperty(bool, playing, READ, WRITE = set_playing, NOTIFY)]
        /// Zoomed in far enough that pixels show as squares.
        #[qproperty(bool, pixelated)]
        /// From asking for the image to it being whole on screen (ms).
        #[qproperty(f64, load_ms, cxx_name = "loadMs")]
        /// From asking to anything on screen (a preview or the whole, ms).
        #[qproperty(f64, first_ms, cxx_name = "firstMs")]
        /// The picture's own pixels are on screen, not a smaller copy (a
        /// picture much bigger than the screen shows a copy at the
        /// screen's size until it's zoomed in past what that shows).
        #[qproperty(bool, whole)]
        /// Being dragged.
        #[qproperty(bool, dragging)]
        type ImageView = super::ImageViewRust;

        #[qobject]
        #[base = ImageItem]
        #[qml_element]
        #[qproperty(QString, path, READ, WRITE = set_path, NOTIFY)]
        /// "", "loading", "ready" or "error".
        #[qproperty(QString, status)]
        /// Drawn at the bottom of the item, not its middle (a grid's,
        /// over its name).
        #[qproperty(bool, bottom_aligned, cxx_name = "bottomAligned", READ, WRITE = set_bottom_aligned, NOTIFY)]
        type ImageThumb = super::ImageThumbRust;

        #[qobject]
        #[qml_element]
        #[qml_singleton]
        type Images = super::ImagesRust;
    }

    impl cxx_qt::Threading for ImageView {}
    impl cxx_qt::Constructor<()> for ImageView {}
    impl cxx_qt::Threading for ImageThumb {}
    impl cxx_qt::Constructor<()> for ImageThumb {}

    extern "RustQt" {
        #[cxx_name = "setPath"]
        fn set_path(self: Pin<&mut ImageView>, path: QString);
        #[cxx_name = "setPlaying"]
        fn set_playing(self: Pin<&mut ImageView>, on: bool);
        #[cxx_name = "setPath"]
        fn set_path(self: Pin<&mut ImageThumb>, path: QString);
        #[cxx_name = "setBottomAligned"]
        fn set_bottom_aligned(self: Pin<&mut ImageThumb>, on: bool);
    }

    extern "RustQt" {
        /// Zooms by `factor` about item point (x, y) (the view's centre
        /// when x < 0), smoothly unless `now`.
        #[qinvokable]
        #[cxx_name = "zoomBy"]
        fn zoom_by(self: Pin<&mut ImageView>, factor: f64, x: f64, y: f64, now: bool);
        #[qinvokable]
        #[cxx_name = "zoomIn"]
        fn zoom_in(self: Pin<&mut ImageView>);
        #[qinvokable]
        #[cxx_name = "zoomOut"]
        fn zoom_out(self: Pin<&mut ImageView>);
        /// Actual size (one image pixel to a screen pixel), about the centre.
        #[qinvokable]
        #[cxx_name = "actualSize"]
        fn actual_size(self: Pin<&mut ImageView>);
        /// Fits the view (and keeps fitting it).
        #[qinvokable]
        #[cxx_name = "fitView"]
        fn fit_view(self: Pin<&mut ImageView>);
        /// Turns a quarter clockwise (`clockwise`) or back.
        #[qinvokable]
        fn rotate(self: Pin<&mut ImageView>, clockwise: bool);
        /// Moves the image by (dx, dy) item pixels.
        #[qinvokable]
        fn pan(self: Pin<&mut ImageView>, dx: f64, dy: f64);
        /// Shows the next frame of an animation (stopped).
        #[qinvokable]
        #[cxx_name = "stepFrame"]
        fn step_frame(self: Pin<&mut ImageView>, forward: bool);
        /// Reads the file again (it changed on disk), keeping the view.
        #[qinvokable]
        fn reload(self: Pin<&mut ImageView>);
        /// Decodes these (JSON list of paths) ahead, for stepping to them.
        #[qinvokable]
        fn prefetch(self: Pin<&mut ImageView>, paths: &QString);
        /// The checkerboard's colours (under transparency).
        #[qinvokable]
        #[cxx_name = "setBackdrop"]
        fn set_backdrop(self: Pin<&mut ImageView>, light: &QString, dark: &QString);
        /// The image pixel under item point (x, y), as JSON `[x, y]`
        /// (self-tests).
        #[qinvokable]
        #[cxx_name = "imagePointAt"]
        fn image_point_at(self: &ImageView, x: f64, y: f64) -> QString;
    }

    unsafe extern "RustQt" {
        #[cxx_override]
        #[cxx_name = "pointerHook"]
        fn pointer_hook(self: Pin<&mut ImageView>, kind: i32, x: f64, y: f64, button: i32, modifiers: i32);
        #[cxx_override]
        #[cxx_name = "wheelHook"]
        fn wheel_hook(self: Pin<&mut ImageView>, dx: f64, dy: f64, x: f64, y: f64, modifiers: i32, pixels: bool) -> bool;
        #[cxx_override]
        #[cxx_name = "pinchHook"]
        fn pinch_hook(self: Pin<&mut ImageView>, factor: f64, x: f64, y: f64);
        #[cxx_override]
        #[cxx_name = "sizeHook"]
        fn size_hook(self: Pin<&mut ImageView>);
        #[cxx_override]
        #[cxx_name = "timerHook"]
        fn timer_hook(self: Pin<&mut ImageView>, id: i32);

        #[cxx_override]
        #[cxx_name = "sizeHook"]
        fn size_hook(self: Pin<&mut ImageThumb>);
    }

    unsafe extern "RustQt" {
        #[inherit]
        fn width(self: &ImageView) -> f64;
        #[inherit]
        fn height(self: &ImageView) -> f64;
        #[inherit]
        #[cxx_name = "pixelRatio"]
        fn pixel_ratio(self: &ImageView) -> f64;
        #[inherit]
        #[cxx_name = "screenSide"]
        fn screen_side(self: &ImageView) -> i32;
        #[inherit]
        #[cxx_name = "showImage"]
        fn show_image(self: Pin<&mut ImageView>, image: &QImage);
        #[inherit]
        #[cxx_name = "clearImage"]
        fn clear_image(self: Pin<&mut ImageView>);
        #[inherit]
        #[cxx_name = "setView"]
        fn set_view(self: Pin<&mut ImageView>, m11: f64, m12: f64, m21: f64, m22: f64, dx: f64, dy: f64);
        #[inherit]
        #[cxx_name = "setSampling"]
        fn set_sampling(self: Pin<&mut ImageView>, nearest: bool);
        #[inherit]
        #[cxx_name = "setChecker"]
        fn set_checker(self: Pin<&mut ImageView>, on: bool, light: u32, dark: u32);
        #[inherit]
        #[cxx_name = "startTicks"]
        fn start_ticks(self: Pin<&mut ImageView>, ms: i32) -> i32;
        #[inherit]
        #[cxx_name = "stopTicks"]
        fn stop_ticks(self: Pin<&mut ImageView>, id: i32);
        #[inherit]
        #[cxx_name = "setGrabbing"]
        fn set_grabbing(self: Pin<&mut ImageView>, on: bool);

        #[inherit]
        fn width(self: &ImageThumb) -> f64;
        #[inherit]
        fn height(self: &ImageThumb) -> f64;
        #[inherit]
        #[cxx_name = "pixelRatio"]
        fn pixel_ratio(self: &ImageThumb) -> f64;
        #[inherit]
        #[cxx_name = "showImage"]
        fn show_image(self: Pin<&mut ImageThumb>, image: &QImage);
        #[inherit]
        #[cxx_name = "clearImage"]
        fn clear_image(self: Pin<&mut ImageThumb>);
        #[inherit]
        #[cxx_name = "setView"]
        fn set_view(self: Pin<&mut ImageThumb>, m11: f64, m12: f64, m21: f64, m22: f64, dx: f64, dy: f64);
        #[inherit]
        #[cxx_name = "takeNoInput"]
        fn take_no_input(self: Pin<&mut ImageThumb>);
    }

    extern "RustQt" {
        /// The images in `folder` (JSON list of paths), in natural order
        /// ("2" before "10").
        #[qinvokable]
        fn list(self: &Images, folder: &QString) -> QString;
        /// `path` names an image harlo can show (by its suffix).
        #[qinvokable]
        #[cxx_name = "isImage"]
        fn is_image(self: &Images, path: &QString) -> bool;
        /// The most common shape (height over width) among `paths` (a JSON
        /// list), read from their headers (some of them, in a big folder),
        /// for a grid's cells: 1 when none can be read.
        #[qinvokable]
        #[cxx_name = "commonShape"]
        fn common_shape(self: &Images, paths: &QString) -> f64;
        /// The caches' counts, as JSON (self-tests).
        #[qinvokable]
        fn stats(self: &Images) -> QString;
        /// Forgets the thumbnails and images kept in memory (the disk cache
        /// stays) (self-tests).
        #[qinvokable]
        #[cxx_name = "forgetMemory"]
        fn forget_memory(self: &Images);
    }
}

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard};
use std::time::{Instant, UNIX_EPOCH};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;

// ---- pixels that cross threads -----------------------------------------------

mod pixels {
    //! A decoded image that the decoding threads and the views share.
    //!
    //! Only `&QImage` leaves this module, never `&mut`: that is what makes
    //! sharing the image between threads sound (see the impls below).

    use cxx_qt_lib::QImage;

    /// A decoded image, never changed after it is wrapped.
    #[derive(Clone)]
    pub(super) struct Pixels(QImage);

    // SAFETY: QImage keeps its pixels in shared data with an atomic reference
    // count, and Qt documents its implicitly shared classes as safe to copy
    // across threads; an image belongs to no thread, so this one may be
    // moved, copied and dropped on any.
    unsafe impl Send for Pixels {}

    // SAFETY: through `&Pixels` only QImage's const member functions are
    // reached (its size, copy construction, reading its pixels to save or
    // encode them), because this module hands out `&QImage` and never
    // `&mut`. The image therefore never detaches or changes once wrapped,
    // and concurrent const calls only read it.
    unsafe impl Sync for Pixels {}

    impl Pixels {
        /// Wraps a decoded image.
        pub(super) fn new(image: QImage) -> Self {
            Self(image)
        }

        /// The image, to read or copy.
        pub(super) fn image(&self) -> &QImage {
            &self.0
        }

        /// What its pixels take in memory (RGBA).
        pub(super) fn bytes(&self) -> usize {
            usize::try_from(self.0.width().max(0)).unwrap_or(0) * usize::try_from(self.0.height().max(0)).unwrap_or(0) * 4
        }

        /// Whether it decoded to an image with pixels.
        pub(super) fn ok(&self) -> bool {
            self.0.width() > 0 && self.0.height() > 0
        }
    }
}

use pixels::Pixels;

/// A decoded image: a still, an animation's frames and how long each
/// shows, or (a still much bigger than the screen) a copy at the screen's
/// size, with the picture's own size.
enum Decoded {
    Still(Pixels),
    Frames(Vec<(Pixels, i32)>),
    Copy { pixels: Pixels, width: i32, height: i32 },
    /// An animation too long to keep: its first frame (and how long it
    /// shows) and how many there are; the rest decoded as it plays.
    Stream { first: Pixels, delay: i32, count: i32 },
}

impl Decoded {
    fn bytes(&self) -> usize {
        match self {
            Decoded::Still(p) | Decoded::Copy { pixels: p, .. } | Decoded::Stream { first: p, .. } => p.bytes(),
            Decoded::Frames(f) => f.iter().map(|(p, _)| p.bytes()).sum(),
        }
    }
    fn first(&self) -> &Pixels {
        match self {
            Decoded::Still(p) | Decoded::Copy { pixels: p, .. } | Decoded::Stream { first: p, .. } => p,
            Decoded::Frames(f) => &f[0].0,
        }
    }
    /// The picture's own size.
    fn size(&self) -> (i32, i32) {
        match self {
            Decoded::Copy { width, height, .. } => (*width, *height),
            _ => (self.first().image().width(), self.first().image().height()),
        }
    }
}

/// The longest side a still is kept at (device pixels): the largest screen
/// a viewer has been on, at least 2048. Bigger ones are kept and shown at
/// this size, their own pixels decoded only for a view zoomed in past it.
static COPY_SIDE: AtomicI32 = AtomicI32::new(2048);

/// Kept as a copy: a still at least half as big again as the copy's side
/// (a little bigger isn't worth the second decode when zoomed in).
fn kept_as_copy(width: i32, height: i32) -> bool {
    i64::from(width.max(height)) * 2 > i64::from(COPY_SIDE.load(Ordering::Relaxed)) * 3
}

fn qs(s: &str) -> QString {
    QString::from(s)
}

/// A file's modification time and length: a changed file is decoded again.
fn stamp(path: &str) -> Option<(u128, u64)> {
    let m = std::fs::metadata(path).ok()?;
    let t = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    Some((t, m.len()))
}

// ---- the decoding threads ----------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lane {
    /// The image being looked at: first.
    Now,
    /// Thumbnails, newest first (the ones coming into view).
    Thumb,
    /// Images decoded ahead.
    Ahead,
}

struct Job {
    /// What it decodes (to find a queued one again).
    key: String,
    cancel: Option<Arc<AtomicBool>>,
    work: Box<dyn FnOnce() + Send>,
}

#[derive(Default)]
struct Queues {
    now: VecDeque<Job>,
    thumbs: Vec<Job>,
    ahead: VecDeque<Job>,
    started: bool,
}

struct Pool {
    queues: Mutex<Queues>,
    ready: Condvar,
}

static POOL: LazyLock<Pool> = LazyLock::new(|| Pool { queues: Mutex::default(), ready: Condvar::new() });

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn submit(lane: Lane, job: Job) {
    let mut q = lock(&POOL.queues);
    if !q.started {
        q.started = true;
        let n = std::thread::available_parallelism().map_or(4, std::num::NonZero::get).saturating_sub(1).clamp(2, 8);
        for i in 0..n {
            let _ = std::thread::Builder::new().name(format!("harlo-images-{i}")).spawn(worker);
        }
    }
    match lane {
        Lane::Now => q.now.push_back(job),
        Lane::Thumb => q.thumbs.push(job),
        Lane::Ahead => q.ahead.push_back(job),
    }
    drop(q);
    POOL.ready.notify_one();
}

/// Moves a job decoding ahead to the front (it's wanted now).
fn hurry(key: &str) {
    let mut q = lock(&POOL.queues);
    if let Some(i) = q.ahead.iter().position(|j| j.key == key) {
        let job = q.ahead.remove(i).unwrap();
        q.now.push_front(job);
    }
}

fn worker() {
    loop {
        let job = {
            let mut q = lock(&POOL.queues);
            loop {
                if let Some(j) = q.now.pop_front().or_else(|| q.thumbs.pop()).or_else(|| q.ahead.pop_front()) {
                    break j;
                }
                q = POOL.ready.wait(q).unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        };
        if job.cancel.as_ref().is_some_and(|c| c.load(Ordering::Acquire)) {
            continue;
        }
        (job.work)();
    }
}

// ---- full images ----------------------------------------------------------------

/// Decoded images kept while a viewer is open (to step back and forth
/// quickly); the last viewer closing lets them all go.
const FULL_BUDGET: usize = 768 << 20;
/// Not decoded ahead past this (a decode that big is only worth doing when asked).
const AHEAD_LIMIT: usize = 256 << 20;
/// An animation's frames kept at most: a longer one is decoded as it plays.
const FRAMES_BUDGET: usize = 128 << 20;
/// Frames decoded ahead of the one shown, for one decoded as it plays.
const FRAMES_AHEAD: usize = 3;
/// A frame stepped to this few ahead of its player's next is waited for
/// there rather than decoded again from the start.
const STEP_AHEAD: i32 = 16;

struct FullEntry {
    path: String,
    stamp: Option<(u128, u64)>,
    pixels: Arc<Decoded>,
    used: u64,
}

type Waiter = Box<dyn FnOnce(anyhow::Result<Arc<Decoded>>) + Send>;

#[derive(Default)]
struct Full {
    entries: Vec<FullEntry>,
    /// Being decoded, and who's waiting for each.
    decoding: HashMap<String, Vec<Waiter>>,
    clock: u64,
}

static FULL: LazyLock<Mutex<Full>> = LazyLock::new(Mutex::default);

/// Image views showing a picture: none, and no decoded image is kept.
static VIEWS: AtomicU64 = AtomicU64::new(0);

/// A view stopped showing a picture (it closed, or was emptied): the last
/// one lets the decoded pictures go (a thumbnail or the disk cache brings
/// them back quickly).
fn view_emptied() {
    if VIEWS.fetch_sub(1, Ordering::AcqRel) == 1 {
        let gone = std::mem::take(&mut lock(&FULL).entries);
        // (Off the window's thread: trimming a big heap takes a moment.)
        std::thread::spawn(move || {
            drop(gone);
            harlo_harness::store::give_back();
        });
    }
}

static DECODED: AtomicU64 = AtomicU64::new(0);
static THUMBS_MADE: AtomicU64 = AtomicU64::new(0);
static THUMBS_FROM_DISK: AtomicU64 = AtomicU64::new(0);
static THUMBS_FROM_MEMORY: AtomicU64 = AtomicU64::new(0);

/// The decoded image for `path`, if it's cached and the file is unchanged.
fn cached_full(path: &str) -> Option<Arc<Decoded>> {
    let now = stamp(path);
    let mut full = lock(&FULL);
    full.clock += 1;
    let clock = full.clock;
    let e = full.entries.iter_mut().find(|e| e.path == path)?;
    if e.stamp != now {
        return None;
    }
    e.used = clock;
    Some(Arc::clone(&e.pixels))
}

fn forget_full(path: &str) {
    lock(&FULL).entries.retain(|e| e.path != path);
}

/// Decodes `path` whole (unless it's cached or on its way), calling
/// `deliver` (on a decoding thread) when it is.
fn fetch_full(path: &str, lane: Lane, deliver: Option<Waiter>) {
    {
        let mut full = lock(&FULL);
        if let Some(waiters) = full.decoding.get_mut(path) {
            waiters.extend(deliver);
            drop(full);
            if lane == Lane::Now {
                hurry(path);
            }
            return;
        }
        full.decoding.insert(path.to_owned(), deliver.into_iter().collect())
    };
    let owned = path.to_owned();
    submit(
        lane,
        Job {
            key: owned.clone(),
            cancel: None,
            work: Box::new(move || {
                let ahead = lane == Lane::Ahead;
                let result = decode_full(&owned, ahead);
                let waiters = {
                    let mut full = lock(&FULL);
                    // (Decoded for viewers that have all closed since: not
                    // kept, ahead or asked for. Checked under the lock the
                    // last one empties the cache under, so it can't refill it.)
                    let wanted = VIEWS.load(Ordering::Acquire) > 0;
                    if let (Ok(pixels), true) = (&result, wanted) {
                        full.clock += 1;
                        let used = full.clock;
                        full.entries.retain(|e| e.path != owned);
                        full.entries.push(FullEntry { path: owned.clone(), stamp: stamp(&owned), pixels: Arc::clone(pixels), used });
                        // Over budget: the least recently used go (not this one).
                        let mut total: usize = full.entries.iter().map(|e| e.pixels.bytes()).sum();
                        while total > FULL_BUDGET && full.entries.len() > 1 {
                            let (i, _) = full.entries.iter().enumerate().filter(|(_, e)| e.path != owned).min_by_key(|(_, e)| e.used).unwrap();
                            total -= full.entries.remove(i).pixels.bytes();
                        }
                    }
                    full.decoding.remove(&owned).unwrap_or_default()
                };
                for w in waiters {
                    w(result.as_ref().map(Arc::clone).map_err(|e| anyhow::anyhow!("{e:#}")));
                }
            }),
        },
    );
}

fn decode_full(path: &str, ahead: bool) -> anyhow::Result<Arc<Decoded>> {
    let q = qs(path);
    let (mut w, mut h) = (0, 0);
    qobject::image_size(&q, &mut w, &mut h);
    if ahead && usize::try_from(w).unwrap_or(0) * usize::try_from(h).unwrap_or(0) * 4 > AHEAD_LIMIT {
        return Err(anyhow::Error::msg("too big to decode ahead"));
    }
    DECODED.fetch_add(1, Ordering::Relaxed);
    let mut reader = qobject::open_frames(&q);
    if let Some(mut r) = reader.as_mut().filter(|r| r.animated()) {
        let said = r.frame_count();
        let mut frames: Vec<(Pixels, i32)> = Vec::new();
        let (mut bytes, mut count, mut streamed) = (0, 0, false);
        loop {
            let mut delay = 0;
            let img = Pixels::new(r.as_mut().next_frame(&mut delay));
            if !img.ok() {
                break;
            }
            count += 1;
            bytes += img.bytes();
            // Too long to keep (said so, or found so): only its first
            // frame stays, the rest counted.
            if !streamed && (bytes > FRAMES_BUDGET || (count == 1 && said > 1 && img.bytes() * usize::try_from(said).unwrap_or(0) > FRAMES_BUDGET)) {
                streamed = true;
                frames.truncate(1);
                if count > 1 && said > 1 {
                    break;
                }
            }
            if !streamed || frames.is_empty() {
                // (Browsers show frames asking for 10 ms or less for 100 ms.)
                frames.push((img, frame_delay(delay)));
            }
            if streamed && said > 1 {
                break;
            }
        }
        if streamed {
            let (first, delay) = frames.pop().expect("its first frame");
            return Ok(Arc::new(Decoded::Stream { first, delay, count: if said > 1 { said } else { count } }));
        }
        match frames.len() {
            0 => {}
            1 => return Ok(Arc::new(Decoded::Still(frames.pop().unwrap().0))),
            _ => return Ok(Arc::new(Decoded::Frames(frames))),
        }
    }
    drop(reader);
    if kept_as_copy(w, h) {
        let img = Pixels::new(qobject::decode_image(&q, COPY_SIDE.load(Ordering::Relaxed)));
        return if img.ok() { Ok(Arc::new(Decoded::Copy { pixels: img, width: w, height: h })) } else { Err(anyhow::Error::msg("Could not read this image.")) };
    }
    let img = Pixels::new(qobject::decode_image(&q, 0));
    if img.ok() { Ok(Arc::new(Decoded::Still(img))) } else { Err(anyhow::Error::msg("Could not read this image.")) }
}

/// How long a frame shows: browsers show frames asking for 10 ms or less for 100 ms.
fn frame_delay(asked: i32) -> i32 {
    if asked <= 10 { 100 } else { asked }
}

/// A frame decoded as it plays: its number, pixels and how long it shows.
type Played = (i32, Pixels, i32);

/// An animation decoded as it plays, on a thread of its own: the frames
/// it has sent, and the number of the next it will.
struct Player {
    rx: std::sync::mpsc::Receiver<Played>,
    stop: Arc<AtomicBool>,
    next: i32,
}

impl Drop for Player {
    fn drop(&mut self) {
        // (Its thread stops at the next frame, even while it's still
        // decoding its way to where it starts.)
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Plays `path` from frame `from` on a thread of its own, a few frames
/// ahead, from the start again after its last; it stops when the
/// [`Player`] is dropped.
fn play_from(path: String, from: i32) -> Player {
    let (tx, rx) = std::sync::mpsc::sync_channel(FRAMES_AHEAD);
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = Arc::clone(&stop);
    let _ = std::thread::Builder::new().name("harlo-frames".into()).spawn(move || {
        let q = qs(&path);
        let (mut from, mut i) = (from, 0);
        let mut reader = qobject::open_frames(&q);
        loop {
            if stopped.load(Ordering::Relaxed) {
                return;
            }
            let Some(mut r) = reader.as_mut() else { return };
            let mut delay = 0;
            let img = Pixels::new(r.as_mut().next_frame(&mut delay));
            if !img.ok() {
                if i == 0 {
                    return;
                }
                (reader, i, from) = (qobject::open_frames(&q), 0, 0);
                continue;
            }
            i += 1;
            if i - 1 < from {
                continue;
            }
            if tx.send((i - 1, img, frame_delay(delay))).is_err() {
                return;
            }
        }
    });
    Player { rx, stop, next: from }
}

// ---- thumbnails -------------------------------------------------------------------

/// Thumbnail sizes (the longer side, in pixels): the smallest that's big
/// enough is made.
const THUMB_SIDES: [i32; 3] = [128, 256, 512];
/// Thumbnails kept in memory (more are on disk: reading one back is quick).
const THUMB_BUDGET: usize = 64 << 20;

/// A thumbnail, the file stamp it was made from, and when it was last used.
type Thumb = (Pixels, Option<(u128, u64)>, u64);

#[derive(Default)]
struct Thumbs {
    map: HashMap<(String, i32), Thumb>,
    bytes: usize,
    clock: u64,
}

static THUMBS: LazyLock<Mutex<Thumbs>> = LazyLock::new(Mutex::default);

fn thumb_side(device_px: f64) -> i32 {
    THUMB_SIDES.into_iter().find(|&s| f64::from(s) >= device_px).unwrap_or(512)
}

/// A thumbnail kept in memory, of `side` (or, `any_side`, whichever is biggest).
fn cached_thumb(path: &str, side: i32, any_side: bool, stamp_now: Option<(u128, u64)>) -> Option<Pixels> {
    let mut t = lock(&THUMBS);
    t.clock += 1;
    let clock = t.clock;
    let sides: Vec<i32> = if any_side { THUMB_SIDES.into_iter().rev().collect() } else { vec![side] };
    for s in sides {
        if let Some(e) = t.map.get_mut(&(path.to_owned(), s))
            && e.1 == stamp_now {
                e.2 = clock;
                return Some(e.0.clone());
            }
    }
    None
}

fn keep_thumb(path: &str, side: i32, pixels: &Pixels, stamp: Option<(u128, u64)>) {
    let mut t = lock(&THUMBS);
    t.clock += 1;
    let clock = t.clock;
    if let Some(old) = t.map.insert((path.to_owned(), side), (pixels.clone(), stamp, clock)) {
        t.bytes -= old.0.bytes();
    }
    t.bytes += pixels.bytes();
    if t.bytes > THUMB_BUDGET {
        // The least recently used tenth goes.
        let mut by_use: Vec<((String, i32), u64)> = t.map.iter().map(|(k, v)| (k.clone(), v.2)).collect();
        by_use.sort_by_key(|(_, used)| *used);
        for (k, _) in by_use.into_iter().take(t.map.len() / 10 + 1) {
            if let Some(old) = t.map.remove(&k) {
                t.bytes -= old.0.bytes();
            }
        }
    }
}

fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("harlo/thumbnails")
}

/// The thumbnail's file on disk, without its suffix (.jpg, or .png with
/// transparency).
fn disk_name(path: &str, side: i32, stamp: (u128, u64)) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (path, side, stamp).hash(&mut h);
    let a = h.finish();
    (a, "thumb").hash(&mut h);
    cache_dir().join(format!("{a:016x}{:016x}", h.finish()))
}

/// Makes (or reads from disk) the thumbnail for `path`, on a decoding thread.
fn make_thumb(path: &str, side: i32) -> Option<Pixels> {
    let stamp = stamp(path)?;
    if let Some(p) = cached_thumb(path, side, false, Some(stamp)) {
        return Some(p);
    }
    let base = disk_name(path, side, stamp);
    for suffix in ["jpg", "png"] {
        let file = base.with_extension(suffix);
        if file.exists() {
            let p = Pixels::new(qobject::load_cached(&qs(&file.to_string_lossy())));
            if p.ok() {
                THUMBS_FROM_DISK.fetch_add(1, Ordering::Relaxed);
                keep_thumb(path, side, &p, Some(stamp));
                return Some(p);
            }
        }
    }
    let p = Pixels::new(qobject::decode_thumbnail(&qs(path), side));
    if !p.ok() {
        return None;
    }
    THUMBS_MADE.fetch_add(1, Ordering::Relaxed);
    keep_thumb(path, side, &p, Some(stamp));
    let _ = std::fs::create_dir_all(cache_dir());
    qobject::save_thumbnail(p.image(), &qs(&base.to_string_lossy()));
    Some(p)
}

// ---- pictures for the phone ---------------------------------------------------------

/// The longest side of a picture opened on the phone (a phone's screen).
const PHONE_SIDE: i32 = 2048;
/// The thumbnail a phone shows in a conversation (its tiles are about
/// 110 points, 330 pixels).
const PHONE_THUMB: i32 = 512;

/// Pictures kept lossless on the phone (screenshots, drawings: text in a
/// JPEG goes soft), when they fit under `PHONE_BYTES`.
const LOSSLESS: [&str; 6] = ["png", "gif", "bmp", "svg", "tif", "tiff"];
/// Well under a relay's 4 MB frame, once in base64.
const PHONE_BYTES: usize = 2_500_000;

/// A picture as the phone gets it: a thumbnail (from the thumbnail cache)
/// or a copy fitted to a phone's screen; its bytes (JPEG; PNG when it has
/// transparency, or on screen when it was lossless and fits), type and
/// size. (Blocking: a decoding thread.)
pub fn for_phone(path: &str, thumb: bool) -> Option<(Vec<u8>, &'static str, i32, i32)> {
    let img = if thumb { make_thumb(path, PHONE_THUMB)? } else { Pixels::new(qobject::decode_image(&qs(path), PHONE_SIDE)) };
    if !img.ok() {
        return None;
    }
    let (w, h) = (img.image().width(), img.image().height());
    let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    let mut png = !thumb && LOSSLESS.contains(&ext.as_str());
    let mut bytes: Vec<u8> = (&qobject::encode_for_phone(img.image(), 82, &mut png)).into();
    // (Too big as a PNG: a JPEG after all; big photos: a lower quality.)
    if png && bytes.len() > PHONE_BYTES {
        png = false;
        bytes = (&qobject::encode_for_phone(img.image(), 82, &mut png)).into();
    }
    if !png && bytes.len() > PHONE_BYTES {
        bytes = (&qobject::encode_for_phone(img.image(), 65, &mut png)).into();
    }
    (!bytes.is_empty()).then_some((bytes, if png { "image/png" } else { "image/jpeg" }, w, h))
}

// ---- suffixes and folders -----------------------------------------------------------

static SUFFIXES: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut s: Vec<String> = qobject::readable_suffixes().to_string().split(',').map(str::to_owned).filter(|s| !s.is_empty()).collect();
    // (Not pictures to look at: a PDF's first page, Qt's own formats.)
    s.retain(|x| !matches!(x.as_str(), "pdf" | "pbm" | "pgm" | "ppm" | "xbm" | "xpm" | "svgz" | "cur" | "icns" | "wbmp"));
    for always in ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico"] {
        if !s.iter().any(|x| x == always) {
            s.push(always.to_owned());
        }
    }
    if s.iter().any(|x| x == "jpg") && !s.iter().any(|x| x == "jfif") {
        s.push("jfif".to_owned());
    }
    s
});

fn is_image_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => {
            let ext = ext.to_ascii_lowercase();
            SUFFIXES.contains(&ext)
        }
        _ => false,
    }
}

/// "img2" before "img10": runs of digits compare as numbers, the rest
/// without case.
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n = String::new();
                while let Some(c) = x.peek().copied().filter(char::is_ascii_digit) {
                    n.push(c);
                    x.next();
                }
                let mut m = String::new();
                while let Some(d) = y.peek().copied().filter(char::is_ascii_digit) {
                    m.push(d);
                    y.next();
                }
                let (nt, mt) = (n.trim_start_matches('0'), m.trim_start_matches('0'));
                let o = nt.len().cmp(&mt.len()).then_with(|| nt.cmp(mt));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(c), Some(d)) => {
                let o = c.to_lowercase().cmp(d.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                x.next();
                y.next();
            }
        }
    }
}

fn list_images(folder: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(folder)
        .map(|rd| {
            rd.filter_map(|e| {
                let e = e.ok()?;
                let name = e.file_name().into_string().ok()?;
                if name.starts_with('.') || !is_image_path(&name) || !e.file_type().ok()?.is_file() && !e.path().is_file() {
                    return None;
                }
                Some(name)
            })
            .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| natural_cmp(a, b));
    out.into_iter().map(|n| folder.join(n).to_string_lossy().into_owned()).collect()
}

// ---- Images ---------------------------------------------------------------------------

#[derive(Default)]
pub struct ImagesRust;

#[expect(clippy::unused_self, reason = "QML calls these as methods of their object, so they take self")]
impl qobject::Images {
    pub fn list(&self, folder: &QString) -> QString {
        let folder = folder.to_string();
        if folder.is_empty() || folder.starts_with("ssh://") {
            return qs("[]");
        }
        qs(&serde_json::to_string(&list_images(Path::new(&folder))).unwrap_or_else(|_| "[]".into()))
    }

    pub fn is_image(&self, path: &QString) -> bool {
        is_image_path(&path.to_string())
    }

    pub fn common_shape(&self, paths: &QString) -> f64 {
        let paths: Vec<String> = serde_json::from_str(&paths.to_string()).unwrap_or_default();
        let step = paths.len().div_ceil(48).max(1);
        let shapes: Vec<f64> = paths
            .iter()
            .step_by(step)
            .filter_map(|p| {
                let (mut w, mut h) = (0, 0);
                qobject::image_size(&qs(p), &mut w, &mut h);
                (w > 0 && h > 0).then(|| f64::from(h) / f64::from(w))
            })
            .collect();
        median_shape(shapes)
    }

    pub fn stats(&self) -> QString {
        let (full, full_bytes) = {
            let f = lock(&FULL);
            (f.entries.len(), f.entries.iter().map(|e| e.pixels.bytes()).sum::<usize>())
        };
        let thumbs = lock(&THUMBS).map.len();
        let disk = std::fs::read_dir(cache_dir()).map_or(0, std::iter::Iterator::count);
        qs(&serde_json::json!({
            "decoded": DECODED.load(Ordering::Relaxed),
            "thumbsMade": THUMBS_MADE.load(Ordering::Relaxed),
            "thumbsFromDisk": THUMBS_FROM_DISK.load(Ordering::Relaxed),
            "thumbsFromMemory": THUMBS_FROM_MEMORY.load(Ordering::Relaxed),
            "full": full,
            "fullMB": full_bytes >> 20,
            "thumbs": thumbs,
            "diskThumbs": disk,
        })
        .to_string())
    }

    pub fn forget_memory(&self) {
        lock(&FULL).entries.clear();
        let mut t = lock(&THUMBS);
        t.map.clear();
        t.bytes = 0;
    }
}

// ---- ImageView ----------------------------------------------------------------------

/// A smooth zoom under way: log zoom from → to about an item point.
struct ZoomAnim {
    from: f64,
    to: f64,
    start: Instant,
    anchor: (f64, f64),
}

const ZOOM_MS: f64 = 140.0;
const MAX_ZOOM: f64 = 64.0;
pub struct ImageViewRust {
    path: QString,
    status: QString,
    error: QString,
    image_width: i32,
    image_height: i32,
    zoom: f64,
    fit: bool,
    turn: i32,
    frame_count: i32,
    frame: i32,
    playing: bool,
    pixelated: bool,
    load_ms: f64,
    first_ms: f64,
    whole: bool,
    dragging: bool,

    /// Requests made; results for an earlier one are dropped.
    generation: u64,
    asked: Instant,
    /// Image pixels to a shown (texture) pixel: above 1 for a preview.
    shown_k: f64,
    shown_full: bool,
    /// The picture's pixels to a pixel of the copy it's kept as (1: kept whole).
    copy_k: f64,
    /// Its own pixels, decoded for a zoom past what the copy shows.
    own: Option<Pixels>,
    /// Asked for (its count, so an answer to an earlier ask is dropped).
    own_asked: bool,
    own_ask: u64,
    /// Where the image's centre is (item pixels).
    cx: f64,
    cy: f64,
    anim: Option<ZoomAnim>,
    anim_timer: i32,
    decoded: Option<Arc<Decoded>>,
    /// An animation decoded as it plays: its frames as they come, and how
    /// long the one shown stays.
    player: Option<Player>,
    /// A frame stepped to that its player hasn't sent yet: shown when it
    /// comes, the frames before it passed over.
    want: Option<i32>,
    frame_ms: i32,
    frame_timer: i32,
    drag_from: Option<(f64, f64)>,
    /// Keep the zoom and position through the next image (a reload).
    keep_view: bool,
    light: u32,
    dark: u32,
}

impl Default for ImageViewRust {
    fn default() -> Self {
        Self {
            path: QString::default(),
            status: QString::default(),
            error: QString::default(),
            image_width: -1,
            image_height: -1,
            zoom: 1.0,
            fit: true,
            turn: 0,
            frame_count: 0,
            frame: 0,
            playing: false,
            pixelated: false,
            load_ms: 0.0,
            first_ms: 0.0,
            whole: false,
            dragging: false,
            generation: 0,
            asked: Instant::now(),
            shown_k: 1.0,
            shown_full: false,
            copy_k: 1.0,
            own: None,
            own_asked: false,
            own_ask: 0,
            cx: 0.0,
            cy: 0.0,
            anim: None,
            anim_timer: 0,
            decoded: None,
            player: None,
            want: None,
            frame_ms: 100,
            frame_timer: 0,
            drag_from: None,
            keep_view: false,
            light: 0x3a3a3a,
            dark: 0x2e2e2e,
        }
    }
}

impl Drop for ImageViewRust {
    fn drop(&mut self) {
        if !self.path.is_empty() {
            self.decoded = None;
            view_emptied();
        }
    }
}

impl cxx_qt::Initialize for qobject::ImageView {
    fn initialize(self: Pin<&mut Self>) {
        let (light, dark) = (self.light, self.dark);
        self.set_checker(true, light, dark);
    }
}

fn parse_rgb(s: &str) -> Option<u32> {
    let h = s.strip_prefix('#')?;
    let h = if h.len() == 8 { &h[2..] } else { h };
    (h.len() == 6).then(|| u32::from_str_radix(h, 16).ok()).flatten()
}

#[expect(clippy::unused_self, reason = "QML calls these as methods of their object, so they take self")]
impl qobject::ImageView {
    #[expect(clippy::needless_pass_by_value, reason = "a property's WRITE setter takes its value, as cxx-qt calls it")]
    pub fn set_path(mut self: Pin<&mut Self>, path: QString) {
        if path == self.path {
            return;
        }
        let (was, now) = (!self.path.is_empty(), !path.is_empty());
        self.as_mut().rust_mut().path = path.clone();
        self.as_mut().path_changed();
        self.as_mut().rust_mut().keep_view = false;
        if now && !was {
            VIEWS.fetch_add(1, Ordering::AcqRel);
        }
        self.as_mut().load();
        // (After load: it no longer holds its picture either.)
        if was && !now {
            view_emptied();
        }
    }

    pub fn reload(mut self: Pin<&mut Self>) {
        let path = self.path.to_string();
        if path.is_empty() {
            return;
        }
        forget_full(&path);
        self.as_mut().rust_mut().keep_view = true;
        self.load();
    }

    fn set_status_text(mut self: Pin<&mut Self>, status: &str) {
        self.as_mut().set_status(qs(status));
    }

    #[expect(clippy::cast_possible_truncation, reason = "whole pixels, sizes and times from floats: `as` rounds toward zero and saturates, as wanted here")]
    fn load(mut self: Pin<&mut Self>) {
        let path = self.path.to_string();
        let keep = self.keep_view;
        self.as_mut().stop_frames();
        {
            let mut r = self.as_mut().rust_mut();
            r.generation += 1;
            r.asked = Instant::now();
            r.decoded = None;
            r.player = None;
            r.want = None;
            r.shown_full = false;
            r.copy_k = 1.0;
            r.own = None;
            r.own_asked = false;
        };
        // (The biggest screen a viewer is on: pictures are kept at its size.)
        let side = self.screen_side().max((self.width().max(self.height()) * self.pixel_ratio()).ceil() as i32);
        COPY_SIDE.fetch_max(side, Ordering::Relaxed);
        self.as_mut().set_error(QString::default());
        self.as_mut().set_playing(false);
        self.as_mut().set_frame_count(0);
        self.as_mut().set_frame(0);
        self.as_mut().set_load_ms(0.0);
        self.as_mut().set_first_ms(0.0);
        if path.is_empty() {
            self.as_mut().clear_image();
            self.as_mut().set_image_width(-1);
            self.as_mut().set_image_height(-1);
            self.set_status_text("");
            return;
        }
        if !keep {
            self.as_mut().rust_mut().anim = None;
            self.as_mut().set_turn(0);
            self.as_mut().set_fit(true);
        }
        // Cached: on screen now.
        if let Some(d) = cached_full(&path) {
            let g = self.generation;
            self.take_full(g, Ok(d));
            return;
        }
        // Its size from the file's header, so the view is laid out now.
        let (mut w, mut h) = (0, 0);
        qobject::image_size(&qs(&path), &mut w, &mut h);
        let size_changed = w != self.image_width || h != self.image_height;
        self.as_mut().set_image_width(w);
        self.as_mut().set_image_height(h);
        self.as_mut().set_status_text("loading");
        if !keep || size_changed {
            self.as_mut().set_fit(true);
            self.as_mut().relayout();
        }
        // Something to show meanwhile: a thumbnail already made.
        if let Some(t) = cached_thumb(&path, 0, true, stamp(&path)).filter(|_| w > 0) {
            THUMBS_FROM_MEMORY.fetch_add(1, Ordering::Relaxed);
            self.as_mut().show_pixels(&t, false, false);
        } else if !keep {
            self.as_mut().clear_image();
        }
        let thread = self.qt_thread();
        let g = self.generation;
        fetch_full(
            &path,
            Lane::Now,
            Some(Box::new(move |result| {
                let _ = thread.queue(move |this| this.take_full(g, result));
            })),
        );
    }

    /// The whole image arrived (or failed).
    fn take_full(mut self: Pin<&mut Self>, generation: u64, result: anyhow::Result<Arc<Decoded>>) {
        if generation != self.generation {
            return;
        }
        match result {
            Ok(d) => {
                let first = d.first().clone();
                let (w, h) = d.size();
                let size_changed = w != self.image_width || h != self.image_height;
                self.as_mut().set_image_width(w);
                self.as_mut().set_image_height(h);
                if size_changed && !self.keep_view || self.fit {
                    self.as_mut().set_fit(true);
                }
                let frames = match &*d {
                    Decoded::Frames(f) => i32::try_from(f.len()).unwrap_or(i32::MAX),
                    Decoded::Stream { count, .. } => *count,
                    _ => 1,
                };
                let copy = matches!(&*d, Decoded::Copy { .. });
                {
                    let path = self.path.to_string();
                    let mut r = self.as_mut().rust_mut();
                    if let Decoded::Stream { delay, .. } = &*d {
                        (r.player, r.frame_ms) = (Some(play_from(path, 1)), *delay);
                    }
                    r.decoded = Some(Arc::clone(&d));
                    r.copy_k = if copy { f64::from(w.max(1)) / f64::from(first.image().width().max(1)) } else { 1.0 }
                };
                self.as_mut().set_frame_count(frames);
                self.as_mut().set_frame(0);
                let ms = self.asked.elapsed().as_secs_f64() * 1000.0;
                self.as_mut().set_load_ms(ms);
                self.as_mut().show_pixels(&first, !copy, true);
                if frames > 1 {
                    self.as_mut().set_playing(true);
                } else {
                    self.as_mut().set_playing(false);
                }
            }
            Err(e) => {
                self.as_mut().set_error(qs(&format!("{e:#}")));
                self.as_mut().clear_image();
                self.set_status_text("error");
            }
        }
    }

    /// Its own pixels arrived, for a view zoomed in past its copy.
    fn take_own(mut self: Pin<&mut Self>, generation: u64, ask: u64, pixels: &Pixels) {
        if generation != self.generation || ask != self.own_ask || !self.own_asked {
            return;
        }
        self.as_mut().rust_mut().own_asked = false;
        if !pixels.ok() {
            self.as_mut().set_status_text("ready");
            return;
        }
        self.as_mut().rust_mut().own = Some(pixels.clone());
        self.show_pixels(pixels, true, true);
    }

    /// A picture kept as a copy: zoomed in past what the copy shows, its own
    /// pixels are decoded (shown when they come); back out of it, they go.
    fn own_pixels_for_zoom(mut self: Pin<&mut Self>) {
        let Some(Decoded::Copy { pixels, .. }) = self.decoded.as_deref() else { return };
        let texel = self.target_zoom().max(self.zoom) * self.copy_k;
        if texel > 1.0 + 1e-6 {
            if self.own.is_some() || self.own_asked {
                return;
            }
            let (g, path, thread) = (self.generation, self.path.to_string(), self.qt_thread());
            let ask = {
                let mut r = self.as_mut().rust_mut();
                r.own_asked = true;
                r.own_ask += 1;
                r.own_ask
            };
            self.as_mut().set_status_text("preview");
            submit(
                Lane::Now,
                Job {
                    key: format!("own:{path}"),
                    cancel: None,
                    work: Box::new(move || {
                        let pixels = Pixels::new(qobject::decode_image(&qs(&path), 0));
                        let _ = thread.queue(move |this| this.take_own(g, ask, &pixels));
                    }),
                },
            );
        } else if texel <= 0.8 && (self.own.is_some() || self.own_asked) {
            let copy = pixels.clone();
            {
                let mut r = self.as_mut().rust_mut();
                r.own = None;
                r.own_asked = false;
            };
            self.show_pixels(&copy, false, true);
        }
    }

    /// Puts pixels on screen: the whole image (`full`) or a smaller copy,
    /// `enough` for the picture as it's zoomed ("ready") or not yet ("preview").
    fn show_pixels(mut self: Pin<&mut Self>, p: &Pixels, full: bool, enough: bool) {
        let w = f64::from(self.image_width.max(1));
        let k = if full || self.image_width <= 0 { 1.0 } else { w / f64::from(p.image().width().max(1)) };
        {
            let mut r = self.as_mut().rust_mut();
            r.shown_k = k;
            r.shown_full = full;
        };
        if self.first_ms == 0.0 {
            let ms = self.asked.elapsed().as_secs_f64() * 1000.0;
            self.as_mut().set_first_ms(ms.max(0.001));
        }
        if self.image_width <= 0 {
            self.as_mut().set_image_width(p.image().width());
            self.as_mut().set_image_height(p.image().height());
        }
        self.as_mut().set_whole(full);
        self.as_mut().set_status_text(if enough { "ready" } else { "preview" });
        self.as_mut().show_image(p.image());
        self.relayout();
    }

    // ---- geometry ----

    fn dpr(&self) -> f64 {
        self.pixel_ratio().max(0.5)
    }

    /// The image's size as turned (image pixels).
    fn turned(&self) -> (f64, f64) {
        let (w, h) = (f64::from(self.image_width.max(1)), f64::from(self.image_height.max(1)));
        if self.turn % 180 == 90 { (h, w) } else { (w, h) }
    }

    /// The zoom that fits the view (at most actual size).
    fn fit_zoom(&self) -> f64 {
        let (tw, th) = self.turned();
        let dpr = self.dpr();
        let (vw, vh) = (self.width() * dpr, self.height() * dpr);
        if vw <= 0.0 || vh <= 0.0 {
            return 1.0;
        }
        (vw / tw).min(vh / th).min(1.0)
    }

    fn min_zoom(&self) -> f64 {
        (self.fit_zoom().min(1.0) / 4.0).max(1e-4)
    }

    /// Fits again (when fitting), keeps the image in view, and shows it.
    fn relayout(mut self: Pin<&mut Self>) {
        if self.fit && self.anim.is_none() {
            let z = self.fit_zoom();
            self.as_mut().set_zoom(z);
        }
        self.as_mut().clamp();
        self.as_mut().push_view();
        self.own_pixels_for_zoom();
    }

    /// Centres an axis the image fits in; keeps the view covered in the others.
    fn clamp(mut self: Pin<&mut Self>) {
        let (tw, th) = self.turned();
        let s = self.zoom / self.dpr();
        let (ew, eh) = (tw * s, th * s);
        let (vw, vh) = (self.width(), self.height());
        let animating = self.anim.is_some();
        let dpr = self.dpr();
        let z = self.zoom;
        let mut r = self.as_mut().rust_mut();
        r.cx = if ew <= vw { vw / 2.0 } else { r.cx.clamp(vw - ew / 2.0, ew / 2.0) };
        r.cy = if eh <= vh { vh / 2.0 } else { r.cy.clamp(vh - eh / 2.0, eh / 2.0) };
        // At a whole zoom, pixels on whole screen pixels (sharp).
        if !animating && (z - z.round()).abs() < 1e-6 {
            let snap = |c: f64, e: f64| ((c - e / 2.0) * dpr).round() / dpr + e / 2.0;
            r.cx = snap(r.cx, ew);
            r.cy = snap(r.cy, eh);
        }
    }

    /// The matrix from image pixels to item pixels: (a, b, c, d) rotation
    /// and scale, then the shift.
    fn matrix(&self) -> (f64, f64, f64, f64, f64, f64) {
        let s = self.zoom / self.dpr();
        let (w, h) = (f64::from(self.image_width.max(1)), f64::from(self.image_height.max(1)));
        // x' = a·x + b·y, y' = c·x + d·y (clockwise on screen).
        let (a, b, c, d) = match self.turn {
            90 => (0.0, -1.0, 1.0, 0.0),
            180 => (-1.0, 0.0, 0.0, -1.0),
            270 => (0.0, 1.0, -1.0, 0.0),
            _ => (1.0, 0.0, 0.0, 1.0),
        };
        let dx = self.cx - s * (a * w / 2.0 + b * h / 2.0);
        let dy = self.cy - s * (c * w / 2.0 + d * h / 2.0);
        (a * s, b * s, c * s, d * s, dx, dy)
    }

    fn push_view(mut self: Pin<&mut Self>) {
        let (a, b, c, d, dx, dy) = self.matrix();
        let k = self.shown_k;
        // QTransform: x' = m11·x + m21·y + dx, y' = m12·x + m22·y + dy.
        self.as_mut().set_view(a * k, c * k, b * k, d * k, dx, dy);
        let texel = self.zoom * k;
        let nearest = texel >= 2.0 - 1e-9;
        self.as_mut().set_sampling(nearest);
        self.as_mut().set_pixelated(nearest);
    }

    pub fn image_point_at(&self, x: f64, y: f64) -> QString {
        let (a, b, c, d, dx, dy) = self.matrix();
        let det = a * d - b * c;
        if det.abs() < 1e-12 {
            return qs("[0,0]");
        }
        let (px, py) = (x - dx, y - dy);
        let ix = (d * px - b * py) / det;
        let iy = (-c * px + a * py) / det;
        qs(&format!("[{ix:.3},{iy:.3}]"))
    }

    /// Sets the zoom keeping item point (ax, ay) over the same image point.
    fn zoom_about(mut self: Pin<&mut Self>, zoom: f64, ax: f64, ay: f64) {
        let zoom = zoom.clamp(self.min_zoom(), MAX_ZOOM);
        let ratio = zoom / self.zoom;
        {
            let mut r = self.as_mut().rust_mut();
            r.cx = ax - (ax - r.cx) * ratio;
            r.cy = ay - (ay - r.cy) * ratio;
        };
        self.as_mut().set_zoom(zoom);
        self.as_mut().clamp();
        self.as_mut().push_view();
        self.own_pixels_for_zoom();
    }

    fn animate_to(mut self: Pin<&mut Self>, zoom: f64, ax: f64, ay: f64) {
        let zoom = zoom.clamp(self.min_zoom(), MAX_ZOOM);
        // (A zoom already under way continues from where it's got to, to
        // the new target times what was still to come.)
        let from = self.zoom.ln();
        self.as_mut().rust_mut().anim = Some(ZoomAnim { from, to: zoom.ln(), start: Instant::now(), anchor: (ax, ay) });
        // (Its own pixels asked for now, if it's heading past its copy.)
        self.as_mut().own_pixels_for_zoom();
        if self.anim_timer == 0 {
            let id = self.as_mut().start_ticks(8);
            self.as_mut().rust_mut().anim_timer = id;
        }
    }

    fn tick_anim(mut self: Pin<&mut Self>) {
        let Some(a) = self.anim.as_ref() else {
            let id = self.anim_timer;
            self.as_mut().stop_ticks(id);
            self.as_mut().rust_mut().anim_timer = 0;
            return;
        };
        let t = (a.start.elapsed().as_secs_f64() * 1000.0 / ZOOM_MS).min(1.0);
        let ease = 1.0 - (1.0 - t).powi(3);
        let z = (a.from + (a.to - a.from) * ease).exp();
        let (ax, ay) = a.anchor;
        if t >= 1.0 {
            self.as_mut().rust_mut().anim = None;
            let id = self.anim_timer;
            self.as_mut().stop_ticks(id);
            self.as_mut().rust_mut().anim_timer = 0;
        }
        self.zoom_about(z, ax, ay);
    }

    /// Where a zoom is heading (for zooming further while one is under way).
    fn target_zoom(&self) -> f64 {
        self.anim.as_ref().map_or(self.zoom, |a| a.to.exp())
    }

    pub fn zoom_by(mut self: Pin<&mut Self>, factor: f64, x: f64, y: f64, now: bool) {
        if self.image_width <= 0 {
            return;
        }
        let (x, y) = if x < 0.0 { (self.width() / 2.0, self.height() / 2.0) } else { (x, y) };
        self.as_mut().set_fit(false);
        let target = self.target_zoom() * factor;
        if now {
            self.as_mut().rust_mut().anim = None;
            self.zoom_about(target, x, y);
        } else {
            self.animate_to(target, x, y);
        }
    }

    /// The next zoom step up or down, on a ladder of round zooms (×1.25
    /// past its ends).
    fn step_zoom(self: Pin<&mut Self>, up: bool) {
        const LADDER: [f64; 19] = [1.0 / 16.0, 1.0 / 12.0, 0.125, 1.0 / 6.0, 0.25, 1.0 / 3.0, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0, 32.0, 64.0];
        let z = self.target_zoom();
        let next = if up {
            LADDER.into_iter().find(|&s| s > z * 1.001).unwrap_or(z * 1.25)
        } else {
            LADDER.into_iter().rev().find(|&s| s < z / 1.001).unwrap_or(z / 1.25)
        };
        self.zoom_by(next / z, -1.0, -1.0, false);
    }

    pub fn zoom_in(self: Pin<&mut Self>) {
        self.step_zoom(true);
    }

    pub fn zoom_out(self: Pin<&mut Self>) {
        self.step_zoom(false);
    }

    pub fn actual_size(self: Pin<&mut Self>) {
        let z = self.target_zoom();
        self.zoom_by(1.0 / z, -1.0, -1.0, false);
    }

    pub fn fit_view(mut self: Pin<&mut Self>) {
        if self.image_width <= 0 {
            self.set_fit(true);
            return;
        }
        let target = self.fit_zoom();
        let (x, y) = (self.width() / 2.0, self.height() / 2.0);
        // (Centred on the way: the image's centre moves to the view's.)
        {
            let mut r = self.as_mut().rust_mut();
            r.cx = x;
            r.cy = y;
        };
        self.as_mut().animate_to(target, x, y);
        self.as_mut().set_fit(true);
    }

    pub fn rotate(mut self: Pin<&mut Self>, clockwise: bool) {
        let r = (self.turn + if clockwise { 90 } else { 270 }) % 360;
        self.as_mut().set_turn(r);
        if self.fit {
            self.as_mut().rust_mut().anim = None;
        }
        self.relayout();
    }

    pub fn pan(mut self: Pin<&mut Self>, dx: f64, dy: f64) {
        {
            let mut r = self.as_mut().rust_mut();
            r.cx += dx;
            r.cy += dy;
        };
        self.as_mut().clamp();
        self.push_view();
    }

    pub fn prefetch(self: Pin<&mut Self>, paths: &QString) {
        let paths: Vec<String> = serde_json::from_str(&paths.to_string()).unwrap_or_default();
        for p in paths {
            if !p.is_empty() && cached_full(&p).is_none() {
                fetch_full(&p, Lane::Ahead, None);
            }
        }
    }

    pub fn set_backdrop(mut self: Pin<&mut Self>, light: &QString, dark: &QString) {
        let (Some(l), Some(d)) = (parse_rgb(&light.to_string()), parse_rgb(&dark.to_string())) else {
            return;
        };
        {
            let mut r = self.as_mut().rust_mut();
            r.light = l;
            r.dark = d;
        };
        self.set_checker(true, l, d);
    }

    // ---- animation frames ----

    pub fn set_playing(mut self: Pin<&mut Self>, on: bool) {
        let on = on && self.frame_count > 1;
        if on {
            // (Playing again: a frame stepped to isn't waited for.)
            self.as_mut().rust_mut().want = None;
        }
        if on == self.playing {
            return;
        }
        self.as_mut().rust_mut().playing = on;
        self.as_mut().playing_changed();
        if on { self.schedule_frame() } else { self.stop_frames() }
    }

    fn stop_frames(mut self: Pin<&mut Self>) {
        let id = self.frame_timer;
        if id > 0 {
            self.as_mut().stop_ticks(id);
            self.as_mut().rust_mut().frame_timer = 0;
        }
    }

    fn schedule_frame(mut self: Pin<&mut Self>) {
        self.as_mut().stop_frames();
        let delay = match self.decoded.as_deref() {
            Some(Decoded::Frames(f)) => f.get(usize::try_from(self.frame).unwrap_or(0)).map_or(100, |x| x.1),
            Some(Decoded::Stream { .. }) => self.frame_ms,
            _ => return,
        };
        let id = self.as_mut().start_ticks(delay.max(10));
        self.as_mut().rust_mut().frame_timer = id;
    }

    fn show_frame(mut self: Pin<&mut Self>, i: i32) {
        let Some(Decoded::Frames(f)) = self.decoded.as_deref() else { return };
        let n = i32::try_from(f.len()).unwrap_or(i32::MAX);
        let i = i.rem_euclid(n.max(1));
        let p = f[usize::try_from(i).unwrap_or(0)].0.clone();
        self.as_mut().set_frame(i);
        self.as_mut().show_image(p.image());
    }

    /// The next frame of one decoded as it plays, when it's ready.
    fn show_played(mut self: Pin<&mut Self>, (i, p, ms): Played) {
        self.as_mut().rust_mut().frame_ms = ms;
        self.as_mut().set_frame(i);
        self.as_mut().show_image(p.image());
    }

    /// A frame its player sent, taken (its next one counted on).
    fn take_played(mut self: Pin<&mut Self>) -> Result<Played, std::sync::mpsc::TryRecvError> {
        let count = match self.decoded.as_deref() {
            Some(Decoded::Stream { count, .. }) => (*count).max(1),
            _ => 1,
        };
        let mut r = self.as_mut().rust_mut();
        let player = r.player.as_mut().ok_or(std::sync::mpsc::TryRecvError::Disconnected)?;
        let played = player.rx.try_recv()?;
        player.next = (played.0 + 1).rem_euclid(count);
        Ok(played)
    }

    /// Looks again for the frame stepped to in a moment.
    fn look_again(mut self: Pin<&mut Self>) {
        self.as_mut().stop_frames();
        let id = self.as_mut().start_ticks(10);
        self.as_mut().rust_mut().frame_timer = id;
    }

    /// The frame stepped to, once its player sends it (those before it
    /// passed over); never waited for here.
    fn seek_on(mut self: Pin<&mut Self>) {
        use std::sync::mpsc::TryRecvError;
        let Some(want) = self.want else { return };
        loop {
            match self.as_mut().take_played() {
                Ok(played) if played.0 == want => {
                    self.as_mut().stop_frames();
                    self.as_mut().rust_mut().want = None;
                    self.show_played(played);
                    return;
                }
                Ok(_) => {}
                Err(TryRecvError::Empty) => return self.look_again(),
                // (Its file can't be read any more.)
                Err(TryRecvError::Disconnected) => {
                    self.as_mut().stop_frames();
                    self.as_mut().rust_mut().want = None;
                    return;
                }
            }
        }
    }

    /// Plays on: the next frame, or (one decoded as it plays, not ready
    /// yet) a look again shortly.
    fn next_frame(mut self: Pin<&mut Self>) {
        if !matches!(self.decoded.as_deref(), Some(Decoded::Stream { .. })) {
            let next = self.frame + 1;
            self.as_mut().show_frame(next);
            self.schedule_frame();
            return;
        }
        if self.want.is_some() {
            return self.seek_on();
        }
        use std::sync::mpsc::TryRecvError;
        match self.as_mut().take_played() {
            Ok(played) => {
                self.as_mut().show_played(played);
                self.schedule_frame();
            }
            Err(TryRecvError::Empty) => self.look_again(),
            // (Its file can't be read any more.)
            Err(TryRecvError::Disconnected) => self.set_playing(false),
        }
    }

    pub fn step_frame(mut self: Pin<&mut Self>, forward: bool) {
        self.as_mut().set_playing(false);
        let Some(Decoded::Stream { count, .. }) = self.decoded.as_deref() else {
            let i = self.frame + if forward { 1 } else { -1 };
            self.show_frame(i);
            return;
        };
        // One decoded as it plays: the frame comes from its player when
        // it's ready, never waited for here. Steps count on from one still
        // coming; its player goes on if the frame is a little ahead of it,
        // or one is started there (one sent before it is passed over, and
        // a player still on its way to an earlier frame stops).
        let count = (*count).max(1);
        let i = (self.want.unwrap_or(self.frame) + if forward { 1 } else { -1 }).rem_euclid(count);
        let near = self.player.as_ref().is_some_and(|p| (i - p.next).rem_euclid(count) < STEP_AHEAD);
        if !near {
            let path = self.path.to_string();
            self.as_mut().rust_mut().player = Some(play_from(path, i));
        }
        self.as_mut().rust_mut().want = Some(i);
        self.seek_on();
    }

    // ---- input ----

    pub fn pointer_hook(mut self: Pin<&mut Self>, kind: i32, x: f64, y: f64, button: i32, _modifiers: i32) {
        const LEFT: i32 = 1;
        const MIDDLE: i32 = 4;
        match kind {
            0 if button == LEFT || button == MIDDLE => {
                self.as_mut().rust_mut().drag_from = Some((x, y));
            }
            1 => {
                if let Some((fx, fy)) = self.drag_from {
                    if !self.dragging && (x - fx).hypot(y - fy) < 3.0 {
                        return;
                    }
                    if !self.dragging {
                        self.as_mut().set_dragging(true);
                        self.as_mut().set_grabbing(true);
                    }
                    self.as_mut().rust_mut().drag_from = Some((x, y));
                    self.as_mut().set_fit(false);
                    self.pan(x - fx, y - fy);
                }
            }
            2 => {
                self.as_mut().rust_mut().drag_from = None;
                if self.dragging {
                    self.as_mut().set_dragging(false);
                    self.as_mut().set_grabbing(false);
                }
            }
            // Double click: actual size there, or fit again.
            3 if button == LEFT => {
                if self.fit && (self.fit_zoom() - 1.0).abs() > 1e-6 {
                    let z = self.target_zoom();
                    self.zoom_by(1.0 / z, x, y, false);
                } else {
                    self.fit_view();
                }
            }
            _ => {}
        }
    }

    pub fn wheel_hook(mut self: Pin<&mut Self>, dx: f64, dy: f64, x: f64, y: f64, modifiers: i32, pixels: bool) -> bool {
        const CTRL: i32 = 0x0400_0000;
        if self.image_width <= 0 {
            return false;
        }
        if pixels && modifiers & CTRL == 0 {
            // A touchpad: two fingers pan.
            self.as_mut().set_fit(false);
            self.pan(dx, dy);
        } else if pixels {
            self.zoom_by((dy * 0.01).exp(), x, y, true);
        } else if dy != 0.0 {
            // A wheel: zooms at the pointer.
            self.zoom_by(1.25f64.powf(dy), x, y, false);
        } else if dx != 0.0 {
            self.as_mut().set_fit(false);
            self.pan(dx * 60.0, 0.0);
        }
        true
    }

    pub fn pinch_hook(self: Pin<&mut Self>, factor: f64, x: f64, y: f64) {
        if factor > 0.0 {
            self.zoom_by(factor, x, y, true);
        }
    }

    pub fn size_hook(self: Pin<&mut Self>) {
        self.relayout();
    }

    pub fn timer_hook(self: Pin<&mut Self>, id: i32) {
        if id == self.anim_timer {
            self.tick_anim();
        } else if id == self.frame_timer {
            self.next_frame();
        }
    }
}

/// The middle of the shapes, kept between wide and tall bounds (1: none).
fn median_shape(mut shapes: Vec<f64>) -> f64 {
    if shapes.is_empty() {
        return 1.0;
    }
    shapes.sort_by(f64::total_cmp);
    shapes[shapes.len() / 2].clamp(0.5, 1.4)
}

// ---- ImageThumb ---------------------------------------------------------------------

pub struct ImageThumbRust {
    path: QString,
    status: QString,
    bottom_aligned: bool,
    /// The thumbnail's size in pixels.
    size: (f64, f64),
    /// The thumbnail size asked for (0: none yet).
    side: i32,
    cancel: Arc<AtomicBool>,
    generation: u64,
}

impl Default for ImageThumbRust {
    fn default() -> Self {
        Self { path: QString::default(), status: QString::default(), bottom_aligned: false, size: (0.0, 0.0), side: 0, cancel: Arc::new(AtomicBool::new(false)), generation: 0 }
    }
}

impl Drop for ImageThumbRust {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

impl cxx_qt::Initialize for qobject::ImageThumb {
    fn initialize(self: Pin<&mut Self>) {
        // (Its cell takes the clicks.)
        self.take_no_input();
    }
}

impl qobject::ImageThumb {
    pub fn set_path(mut self: Pin<&mut Self>, path: QString) {
        if path == self.path {
            return;
        }
        self.as_mut().rust_mut().path = path;
        self.as_mut().path_changed();
        self.request();
    }

    pub fn set_bottom_aligned(mut self: Pin<&mut Self>, on: bool) {
        if on == self.bottom_aligned {
            return;
        }
        self.as_mut().rust_mut().bottom_aligned = on;
        self.as_mut().bottom_aligned_changed();
        self.layout();
    }

    fn request(mut self: Pin<&mut Self>) {
        // The last request (if it hasn't started) isn't wanted now.
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut r = self.as_mut().rust_mut();
            r.cancel.store(true, Ordering::Release);
            r.cancel = Arc::clone(&cancel);
            r.generation += 1;
            r.size = (0.0, 0.0);
            r.side = 0;
        };
        self.as_mut().clear_image();
        let path = self.path.to_string();
        if path.is_empty() || self.width() <= 0.0 {
            self.as_mut().set_status(QString::default());
            return;
        }
        let side = thumb_side(self.width().max(self.height()) * self.pixel_ratio());
        self.as_mut().rust_mut().side = side;
        if let Some(p) = cached_thumb(&path, side, false, stamp(&path)) {
            THUMBS_FROM_MEMORY.fetch_add(1, Ordering::Relaxed);
            self.take(&p);
            return;
        }
        self.as_mut().set_status(qs("loading"));
        let thread = self.qt_thread();
        let g = self.generation;
        submit(
            Lane::Thumb,
            Job {
                key: path.clone(),
                cancel: Some(cancel),
                work: Box::new(move || {
                    let p = make_thumb(&path, side);
                    let _ = thread.queue(move |mut this| {
                        if this.generation != g {
                            return;
                        }
                        match p {
                            Some(p) => this.take(&p),
                            None => this.as_mut().set_status(qs("error")),
                        }
                    });
                }),
            },
        );
    }

    fn take(mut self: Pin<&mut Self>, p: &Pixels) {
        self.as_mut().rust_mut().size = (f64::from(p.image().width()), f64::from(p.image().height()));
        self.as_mut().show_image(p.image());
        self.as_mut().set_status(qs("ready"));
        self.layout();
    }

    /// Fitted in the item and centred (or at its bottom), never above a
    /// screen pixel a pixel.
    fn layout(self: Pin<&mut Self>) {
        let (pw, ph) = self.size;
        if pw <= 0.0 || ph <= 0.0 {
            return;
        }
        let (w, h) = (self.width(), self.height());
        let s = (w / pw).min(h / ph).min(1.0 / self.pixel_ratio().max(0.5));
        let dpr = self.pixel_ratio().max(0.5);
        let x = (((w - pw * s) / 2.0) * dpr).round() / dpr;
        let y = (((h - ph * s) / if self.bottom_aligned { 1.0 } else { 2.0 }) * dpr).round() / dpr;
        self.set_view(s, 0.0, 0.0, s, x, y);
    }

    pub fn size_hook(mut self: Pin<&mut Self>) {
        if self.path.is_empty() {
            return;
        }
        // (A bigger cell wants a bigger thumbnail.)
        let wanted = thumb_side(self.width().max(self.height()) * self.pixel_ratio());
        if self.width() > 0.0 && (self.side == 0 || wanted > self.side) {
            self.as_mut().request();
        } else {
            self.layout();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grids_shape_is_its_pictures_middle_one() {
        assert_eq!(median_shape(vec![]), 1.0);
        assert_eq!(median_shape(vec![0.667, 0.667, 1.0, 0.5625, 0.667]), 0.667);
        assert_eq!(median_shape(vec![0.2, 0.25]), 0.5);
        assert_eq!(median_shape(vec![3.0]), 1.4);
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["img10.png", "img2.png", "Img1.png", "img02b.png", "a.png"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["a.png", "Img1.png", "img2.png", "img02b.png", "img10.png"]);
    }

    #[test]
    fn thumb_sides() {
        assert_eq!(thumb_side(100.0), 128);
        assert_eq!(thumb_side(129.0), 256);
        assert_eq!(thumb_side(600.0), 512);
    }
}
