//! The image viewer: `ImageView` (one image, zoomed, panned and turned on
//! the GPU), `ImageThumb` (a folder grid's thumbnail) and `Images` (a
//! folder's images, the caches). Pixels come from worker threads through
//! Qt's image readers (cpp/harlo_image.h) and go up to the GPU once; after
//! that, zooming and panning only move them.
//!
//! - Full images: decoded at full size (an animation's frames all kept),
//!   about 768 MB of them cached, the next and previous ones decoded ahead.
//!   A big JPEG shows first at the screen's size, then whole.
//! - Thumbnails: kept in memory (about 192 MB) and on disk under
//!   `$XDG_CACHE_HOME/harlo/thumbnails`, made on every core, the newest
//!   asked for (the ones coming into view) first.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qimage.h");
        type QImage = cxx_qt_lib::QImage;
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
        fn readable_suffixes() -> QString;

        type FrameReader;
        fn open_frames(path: &QString) -> UniquePtr<FrameReader>;
        fn animated(self: &FrameReader) -> bool;
        fn next_frame(self: Pin<&mut FrameReader>, delay_ms: &mut i32) -> QImage;
    }

    extern "RustQt" {
        #[qobject]
        #[base = ImageItem]
        #[qml_element]
        /// The file shown (a local path).
        #[qproperty(QString, path, READ, WRITE = set_path, NOTIFY)]
        /// "" (none), "loading", "preview" (a smaller copy, the whole on its
        /// way), "ready" or "error".
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
        /// Being dragged.
        #[qproperty(bool, dragging)]
        type ImageView = super::ImageViewRust;

        #[qobject]
        #[base = ImageItem]
        #[qml_element]
        #[qproperty(QString, path, READ, WRITE = set_path, NOTIFY)]
        /// "", "loading", "ready" or "error".
        #[qproperty(QString, status)]
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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard};
use std::time::{Instant, UNIX_EPOCH};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QImage, QString};

// ---- pixels that cross threads -----------------------------------------------

/// A decoded image. (QImage shares its pixels with an atomic count, and none
/// is changed after decoding, so it can move between threads.)
#[derive(Clone)]
struct Pixels(QImage);
unsafe impl Send for Pixels {}
unsafe impl Sync for Pixels {}

impl Pixels {
    fn bytes(&self) -> usize {
        self.0.width().max(0) as usize * self.0.height().max(0) as usize * 4
    }
    fn ok(&self) -> bool {
        self.0.width() > 0 && self.0.height() > 0
    }
}

/// A whole image: a still, or an animation's frames and how long each shows.
enum Decoded {
    Still(Pixels),
    Frames(Vec<(Pixels, i32)>),
}

impl Decoded {
    fn bytes(&self) -> usize {
        match self {
            Decoded::Still(p) => p.bytes(),
            Decoded::Frames(f) => f.iter().map(|(p, _)| p.bytes()).sum(),
        }
    }
    fn first(&self) -> &Pixels {
        match self {
            Decoded::Still(p) => p,
            Decoded::Frames(f) => &f[0].0,
        }
    }
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
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn submit(lane: Lane, job: Job) {
    let mut q = lock(&POOL.queues);
    if !q.started {
        q.started = true;
        let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).saturating_sub(1).clamp(2, 8);
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
                q = POOL.ready.wait(q).unwrap_or_else(|e| e.into_inner());
            }
        };
        if job.cancel.as_ref().is_some_and(|c| c.load(Ordering::Acquire)) {
            continue;
        }
        (job.work)();
    }
}

// ---- full images ----------------------------------------------------------------

const FULL_BUDGET: usize = 768 << 20;
/// Not decoded ahead past this (a decode that big is only worth doing when asked).
const AHEAD_LIMIT: usize = 256 << 20;
/// An animation's frames kept at most.
const FRAMES_BUDGET: usize = 1 << 30;

struct FullEntry {
    path: String,
    stamp: Option<(u128, u64)>,
    pixels: Arc<Decoded>,
    used: u64,
}

type Waiter = Box<dyn FnOnce(Result<Arc<Decoded>, String>) + Send>;

#[derive(Default)]
struct Full {
    entries: Vec<FullEntry>,
    /// Being decoded, and who's waiting for each.
    decoding: HashMap<String, Vec<Waiter>>,
    clock: u64,
}

static FULL: LazyLock<Mutex<Full>> = LazyLock::new(Mutex::default);

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
        full.decoding.insert(path.to_owned(), deliver.into_iter().collect());
    }
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
                    if let Ok(pixels) = &result {
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
                    w(result.as_ref().map(Arc::clone).map_err(Clone::clone));
                }
            }),
        },
    );
}

fn decode_full(path: &str, ahead: bool) -> Result<Arc<Decoded>, String> {
    let q = qs(path);
    if ahead {
        let (mut w, mut h) = (0, 0);
        qobject::image_size(&q, &mut w, &mut h);
        if w as usize * h.max(0) as usize * 4 > AHEAD_LIMIT {
            return Err("too big to decode ahead".into());
        }
    }
    DECODED.fetch_add(1, Ordering::Relaxed);
    let mut reader = qobject::open_frames(&q);
    if let Some(mut r) = reader.as_mut().filter(|r| r.animated()) {
        let mut frames = Vec::new();
        let mut bytes = 0;
        loop {
            let mut delay = 0;
            let img = Pixels(r.as_mut().next_frame(&mut delay));
            if !img.ok() {
                break;
            }
            bytes += img.bytes();
            // (Browsers show frames asking for 10 ms or less for 100 ms.)
            frames.push((img, if delay <= 10 { 100 } else { delay }));
            if bytes > FRAMES_BUDGET {
                break;
            }
        }
        match frames.len() {
            0 => {}
            1 => return Ok(Arc::new(Decoded::Still(frames.pop().unwrap().0))),
            _ => return Ok(Arc::new(Decoded::Frames(frames))),
        }
    }
    drop(reader);
    let img = Pixels(qobject::decode_image(&q, 0));
    if img.ok() { Ok(Arc::new(Decoded::Still(img))) } else { Err("Could not read this image.".into()) }
}

// ---- thumbnails -------------------------------------------------------------------

/// Thumbnail sizes (the longer side, in pixels): the smallest that's big
/// enough is made.
const THUMB_SIDES: [i32; 3] = [128, 256, 512];
const THUMB_BUDGET: usize = 192 << 20;

#[derive(Default)]
struct Thumbs {
    map: HashMap<(String, i32), (Pixels, Option<(u128, u64)>, u64)>,
    bytes: usize,
    clock: u64,
}

static THUMBS: LazyLock<Mutex<Thumbs>> = LazyLock::new(Mutex::default);

fn thumb_side(device_px: f64) -> i32 {
    THUMB_SIDES.into_iter().find(|&s| s as f64 >= device_px).unwrap_or(512)
}

/// A thumbnail kept in memory, of `side` (or, `any_side`, whichever is biggest).
fn cached_thumb(path: &str, side: i32, any_side: bool, stamp_now: Option<(u128, u64)>) -> Option<Pixels> {
    let mut t = lock(&THUMBS);
    t.clock += 1;
    let clock = t.clock;
    let sides: Vec<i32> = if any_side { THUMB_SIDES.into_iter().rev().collect() } else { vec![side] };
    for s in sides {
        if let Some(e) = t.map.get_mut(&(path.to_owned(), s)) {
            if e.1 == stamp_now {
                e.2 = clock;
                return Some(e.0.clone());
            }
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
            let p = Pixels(qobject::load_cached(&qs(&file.to_string_lossy())));
            if p.ok() {
                THUMBS_FROM_DISK.fetch_add(1, Ordering::Relaxed);
                keep_thumb(path, side, &p, Some(stamp));
                return Some(p);
            }
        }
    }
    let p = Pixels(qobject::decode_thumbnail(&qs(path), side));
    if !p.ok() {
        return None;
    }
    THUMBS_MADE.fetch_add(1, Ordering::Relaxed);
    keep_thumb(path, side, &p, Some(stamp));
    let _ = std::fs::create_dir_all(cache_dir());
    qobject::save_thumbnail(&p.0, &qs(&base.to_string_lossy()));
    Some(p)
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
            SUFFIXES.iter().any(|s| *s == ext)
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

    pub fn stats(&self) -> QString {
        let (full, full_bytes) = {
            let f = lock(&FULL);
            (f.entries.len(), f.entries.iter().map(|e| e.pixels.bytes()).sum::<usize>())
        };
        let thumbs = lock(&THUMBS).map.len();
        let disk = std::fs::read_dir(cache_dir()).map(|rd| rd.count()).unwrap_or(0);
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
/// JPEGs bigger than this show a screen-sized copy first. (A JPEG decodes
/// straight at a smaller size, quickly; other formats decode whole anyway.)
const PREVIEW_PIXELS: i64 = 12_000_000;

fn decodes_small(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.ends_with(".jpg") || p.ends_with(".jpeg") || p.ends_with(".jfif")
}

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
    dragging: bool,

    /// Requests made; results for an earlier one are dropped.
    generation: u64,
    asked: Instant,
    /// Image pixels to a shown (texture) pixel: above 1 for a preview.
    shown_k: f64,
    shown_full: bool,
    /// Where the image's centre is (item pixels).
    cx: f64,
    cy: f64,
    anim: Option<ZoomAnim>,
    anim_timer: i32,
    decoded: Option<Arc<Decoded>>,
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
            dragging: false,
            generation: 0,
            asked: Instant::now(),
            shown_k: 1.0,
            shown_full: false,
            cx: 0.0,
            cy: 0.0,
            anim: None,
            anim_timer: 0,
            decoded: None,
            frame_timer: 0,
            drag_from: None,
            keep_view: false,
            light: 0x3a3a3a,
            dark: 0x2e2e2e,
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

impl qobject::ImageView {
    pub fn set_path(mut self: Pin<&mut Self>, path: QString) {
        if path == self.path {
            return;
        }
        self.as_mut().rust_mut().path = path.clone();
        self.as_mut().path_changed();
        self.as_mut().rust_mut().keep_view = false;
        self.load();
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

    fn load(mut self: Pin<&mut Self>) {
        let path = self.path.to_string();
        let keep = self.keep_view;
        self.as_mut().stop_frames();
        {
            let mut r = self.as_mut().rust_mut();
            r.generation += 1;
            r.asked = Instant::now();
            r.decoded = None;
            r.shown_full = false;
        }
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
        // Something to show meanwhile: a thumbnail already made, else (a
        // big one) a copy at the screen's size.
        let shown = if let Some(t) = cached_thumb(&path, 0, true, stamp(&path)).filter(|_| w > 0) {
            THUMBS_FROM_MEMORY.fetch_add(1, Ordering::Relaxed);
            self.as_mut().show_pixels(&t, false);
            true
        } else {
            if !keep {
                self.as_mut().clear_image();
            }
            false
        };
        let thread = self.qt_thread();
        let g = self.generation;
        if !shown && w > 0 && (w as i64) * (h as i64) > PREVIEW_PIXELS && decodes_small(&path) {
            let side = (self.width().max(self.height()) * self.pixel_ratio()).ceil().max(512.0) as i32;
            let thread = thread.clone();
            let p = path.clone();
            submit(
                Lane::Now,
                Job {
                    key: format!("preview:{path}"),
                    cancel: None,
                    work: Box::new(move || {
                        let img = Pixels(qobject::decode_image(&qs(&p), side));
                        let _ = thread.queue(move |mut this| {
                            if this.generation == g && !this.shown_full && img.ok() {
                                this.as_mut().show_pixels(&img, false);
                            }
                        });
                    }),
                },
            );
        }
        fetch_full(
            &path,
            Lane::Now,
            Some(Box::new(move |result| {
                let _ = thread.queue(move |this| this.take_full(g, result));
            })),
        );
    }

    /// The whole image arrived (or failed).
    fn take_full(mut self: Pin<&mut Self>, generation: u64, result: Result<Arc<Decoded>, String>) {
        if generation != self.generation {
            return;
        }
        match result {
            Ok(d) => {
                let first = d.first().clone();
                let (w, h) = (first.0.width(), first.0.height());
                let size_changed = w != self.image_width || h != self.image_height;
                self.as_mut().set_image_width(w);
                self.as_mut().set_image_height(h);
                if size_changed && !(self.keep_view && !self.fit) {
                    self.as_mut().set_fit(true);
                }
                let frames = match &*d {
                    Decoded::Frames(f) => f.len() as i32,
                    Decoded::Still(_) => 1,
                };
                self.as_mut().rust_mut().decoded = Some(Arc::clone(&d));
                self.as_mut().set_frame_count(frames);
                self.as_mut().set_frame(0);
                self.as_mut().show_pixels(&first, true);
                let ms = self.asked.elapsed().as_secs_f64() * 1000.0;
                self.as_mut().set_load_ms(ms);
                self.as_mut().set_status_text("ready");
                if frames > 1 {
                    self.as_mut().set_playing(true);
                } else {
                    self.as_mut().set_playing(false);
                }
            }
            Err(e) => {
                self.as_mut().set_error(qs(&e));
                self.as_mut().clear_image();
                self.set_status_text("error");
            }
        }
    }

    /// Puts pixels on screen: the whole image (`full`) or a smaller copy.
    fn show_pixels(mut self: Pin<&mut Self>, p: &Pixels, full: bool) {
        let w = self.image_width.max(1) as f64;
        let k = if full || self.image_width <= 0 { 1.0 } else { w / p.0.width().max(1) as f64 };
        {
            let mut r = self.as_mut().rust_mut();
            r.shown_k = k;
            r.shown_full = full;
        }
        if self.first_ms == 0.0 {
            let ms = self.asked.elapsed().as_secs_f64() * 1000.0;
            self.as_mut().set_first_ms(ms.max(0.001));
        }
        if self.image_width <= 0 {
            self.as_mut().set_image_width(p.0.width());
            self.as_mut().set_image_height(p.0.height());
        }
        if !full {
            self.as_mut().set_status_text("preview");
        }
        self.as_mut().show_image(&p.0);
        self.relayout();
    }

    // ---- geometry ----

    fn dpr(&self) -> f64 {
        self.pixel_ratio().max(0.5)
    }

    /// The image's size as turned (image pixels).
    fn turned(&self) -> (f64, f64) {
        let (w, h) = (self.image_width.max(1) as f64, self.image_height.max(1) as f64);
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
        self.push_view();
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
        let (w, h) = (self.image_width.max(1) as f64, self.image_height.max(1) as f64);
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
        }
        self.as_mut().set_zoom(zoom);
        self.as_mut().clamp();
        self.push_view();
    }

    fn animate_to(mut self: Pin<&mut Self>, zoom: f64, ax: f64, ay: f64) {
        let zoom = zoom.clamp(self.min_zoom(), MAX_ZOOM);
        // (A zoom already under way continues from where it's got to, to
        // the new target times what was still to come.)
        let from = self.zoom.ln();
        self.as_mut().rust_mut().anim = Some(ZoomAnim { from, to: zoom.ln(), start: Instant::now(), anchor: (ax, ay) });
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
        }
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
        }
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
        }
        self.set_checker(true, l, d);
    }

    // ---- animation frames ----

    pub fn set_playing(mut self: Pin<&mut Self>, on: bool) {
        let on = on && self.frame_count > 1;
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
            Some(Decoded::Frames(f)) => f.get(self.frame as usize).map_or(100, |x| x.1),
            _ => return,
        };
        let id = self.as_mut().start_ticks(delay.max(10));
        self.as_mut().rust_mut().frame_timer = id;
    }

    fn show_frame(mut self: Pin<&mut Self>, i: i32) {
        let Some(Decoded::Frames(f)) = self.decoded.as_deref() else { return };
        let n = f.len() as i32;
        let i = i.rem_euclid(n.max(1));
        let p = f[i as usize].0.clone();
        self.as_mut().set_frame(i);
        self.as_mut().show_image(&p.0);
    }

    pub fn step_frame(mut self: Pin<&mut Self>, forward: bool) {
        self.as_mut().set_playing(false);
        let i = self.frame + if forward { 1 } else { -1 };
        self.show_frame(i);
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

    pub fn timer_hook(mut self: Pin<&mut Self>, id: i32) {
        if id == self.anim_timer {
            self.tick_anim();
        } else if id == self.frame_timer {
            let next = self.frame + 1;
            self.as_mut().show_frame(next);
            self.schedule_frame();
        }
    }
}

// ---- ImageThumb ---------------------------------------------------------------------

pub struct ImageThumbRust {
    path: QString,
    status: QString,
    /// The thumbnail's size in pixels.
    size: (f64, f64),
    /// The thumbnail size asked for (0: none yet).
    side: i32,
    cancel: Arc<AtomicBool>,
    generation: u64,
}

impl Default for ImageThumbRust {
    fn default() -> Self {
        Self { path: QString::default(), status: QString::default(), size: (0.0, 0.0), side: 0, cancel: Arc::new(AtomicBool::new(false)), generation: 0 }
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
        }
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
        self.as_mut().rust_mut().size = (p.0.width() as f64, p.0.height() as f64);
        self.as_mut().show_image(&p.0);
        self.as_mut().set_status(qs("ready"));
        self.layout();
    }

    /// Fitted in the item and centred, never above a screen pixel a pixel.
    fn layout(self: Pin<&mut Self>) {
        let (pw, ph) = self.size;
        if pw <= 0.0 || ph <= 0.0 {
            return;
        }
        let (w, h) = (self.width(), self.height());
        let s = (w / pw).min(h / ph).min(1.0 / self.pixel_ratio().max(0.5));
        let dpr = self.pixel_ratio().max(0.5);
        let x = (((w - pw * s) / 2.0) * dpr).round() / dpr;
        let y = (((h - ph * s) / 2.0) * dpr).round() / dpr;
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
