//! Reading a long session must take about what's kept of it, not many times
//! the file: a session of hundreds of megabytes once took gigabytes to open.
//! These count what's allocated, so they fail the moment a reader goes back to
//! holding a whole file or every line of one.

// Rust guideline compliant 2026-10-09

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use harlo_core::transcript::Transcript;
use harlo_harness::{branches, store};

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: passes everything to the system allocator and only counts.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
        PEAK.fetch_max(live, Ordering::Relaxed);
        // SAFETY: the caller's contract for `alloc` (a non-zero-size layout)
        // is passed on unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: `ptr` came from `alloc`/`realloc` above with `layout`, so it
        // was the system allocator's, as `dealloc` requires.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if new_size > layout.size() {
            let live = LIVE.fetch_add(new_size - layout.size(), Ordering::Relaxed) + new_size - layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        } else {
            LIVE.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
        }
        // SAFETY: `ptr` and `layout` are the system allocator's (see above),
        // and the caller's contract for `new_size` is passed on unchanged.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// One at a time: the counts are the whole process's.
static SERIAL: Mutex<()> = Mutex::new(());

const MB: usize = 1024 * 1024;
const FILE: usize = 64 * MB;
/// Everything a test may hold above what it started with.
const ALLOWED: usize = 8 * MB;

/// How much more than at the start `f` had allocated at its highest.
fn peak_of<T>(f: impl FnOnce() -> T) -> (T, usize) {
    let start = LIVE.load(Ordering::Relaxed);
    PEAK.store(start, Ordering::Relaxed);
    let out = f();
    (out, PEAK.load(Ordering::Relaxed).saturating_sub(start))
}

struct Temp(PathBuf);
impl Temp {
    fn new(name: &str) -> Temp {
        let dir = std::env::temp_dir().join(format!("harlo-memory-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Temp(dir)
    }
    fn file(&self) -> PathBuf {
        self.0.join("s.jsonl")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_lines(path: &Path, mut next: impl FnMut(usize) -> String) {
    let mut out = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let (mut written, mut i) = (0, 0);
    while written < FILE {
        let line = next(i);
        out.write_all(line.as_bytes()).unwrap();
        out.write_all(b"\n").unwrap();
        written += line.len() + 1;
        i += 1;
    }
    out.flush().unwrap();
}

fn pi_entry(i: usize, parent: &str, role: &str, text: &str) -> String {
    format!(r#"{{"type":"message","id":"m{i}","parentId":{parent},"timestamp":"2026-11-01T08:00:00Z","message":{{"role":"{role}","content":[{{"type":"text","text":"{text}"}}]}}}}"#)
}

/// A pi session with two big branches off its first message and a short one
/// (the current branch).
fn pi_session(path: &Path) -> usize {
    let padding = "x".repeat(16 * 1024);
    let mut count = 0;
    write_lines(path, |i| {
        count = i + 1;
        match i {
            0 => r#"{"type":"session","version":3,"id":"s","timestamp":"2026-11-01T08:00:00Z","cwd":"/tmp"}"#.to_owned(),
            1 => pi_entry(1, "null", "user", "first"),
            2 => pi_entry(2, r#""m1""#, "assistant", &padding),
            _ => pi_entry(i, &format!(r#""m{}""#, i - 1), "assistant", &padding),
        }
    });
    let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    writeln!(file, "{}", pi_entry(count, r#""m1""#, "assistant", "the current answer")).unwrap();
    writeln!(file, "{}", pi_entry(count + 1, &format!(r#""m{count}""#), "user", "go on")).unwrap();
    count + 2
}

#[test]
fn listing_branches_holds_no_more_than_their_outline() {
    let _serial = SERIAL.lock().unwrap();
    let temp = Temp::new("branches");
    let path = temp.file();
    pi_session(&path);
    let (list, peak) = peak_of(|| branches::branches(&path).unwrap());
    assert_eq!(list.len(), 2, "the long branch and the current one");
    assert!(list[0].current);
    assert!(peak < ALLOWED, "held {} MB", peak / MB);
    let (entries, peak) = peak_of(|| branches::current_user_entries(&path).unwrap());
    assert_eq!(entries.len(), 2);
    assert!(peak < ALLOWED, "held {} MB", peak / MB);
}

#[test]
fn opening_a_session_reads_only_its_current_branch() {
    let _serial = SERIAL.lock().unwrap();
    let temp = Temp::new("saved");
    let path = temp.file();
    pi_session(&path);
    let (saved, peak) = peak_of(|| store::read_saved(&path).unwrap());
    assert_eq!(saved.messages.len(), 3, "first, the current answer, go on");
    assert!(peak < ALLOWED, "held {} MB", peak / MB);
}

/// Opened a message at a time, a session as long as the file holds only the
/// message being passed on.
#[test]
fn a_long_branch_streams_a_message_at_a_time() {
    let _serial = SERIAL.lock().unwrap();
    let temp = Temp::new("streamed");
    let path = temp.file();
    let body = "x".repeat(32 * 1024);
    write_lines(&path, |i| match i {
        0 => r#"{"type":"session","version":3,"id":"s","cwd":"/w"}"#.to_owned(),
        _ => {
            let parent = if i == 1 { "null".to_owned() } else { format!("\"m{}\"", i - 1) };
            format!(r#"{{"type":"message","id":"m{i}","parentId":{parent},"message":{{"role":"user","content":"{body}"}}}}"#)
        }
    });
    let ((n, saved), peak) = peak_of(|| {
        let mut n = 0;
        let saved = store::stream_saved_for(&path, &mut |_| n += 1).unwrap();
        (n, saved)
    });
    assert!(n > 1000 && saved.messages.is_empty(), "{n}");
    assert!(peak < ALLOWED, "held {} MB", peak / MB);
}

#[test]
fn a_grown_session_is_read_from_where_it_was() {
    let _serial = SERIAL.lock().unwrap();
    let temp = Temp::new("grown");
    let path = temp.file();
    let padding = "x".repeat(16 * 1024);
    let mut last = 0;
    write_lines(&path, |i| {
        last = i;
        match i {
            0 => r#"{"type":"session","version":3,"id":"s","timestamp":"2026-11-01T08:00:00Z","cwd":"/tmp"}"#.to_owned(),
            1 => pi_entry(1, "null", "user", "first"),
            _ => pi_entry(i, &format!(r#""m{}""#, i - 1), if i % 2 == 0 { "assistant" } else { "user" }, &padding),
        }
    });
    let first = store::read_part(&path, None).unwrap();
    assert!(first.messages.len() > 100);
    let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    writeln!(file, "{}", pi_entry(last + 1, &format!(r#""m{last}""#), "assistant", "more")).unwrap();
    let (next, peak) = peak_of(|| store::read_part(&path, Some(&first.resume)).unwrap());
    assert!(next.first_index > 0);
    assert_eq!(next.messages.len(), 1, "only what was added");
    assert!(peak < ALLOWED, "held {} MB", peak / MB);
}

/// Tool calls in runs: each row's output is held once, and showing the list
/// again (every moment while a reply streams) copies nothing.
#[test]
fn rows_in_runs_are_held_once() {
    let _serial = SERIAL.lock().unwrap();
    const CALLS: usize = 2000;
    let output = "x".repeat(4096);
    let mut messages = vec![serde_json::json!({ "role": "user", "content": "go" })];
    for i in 0..CALLS {
        messages.push(serde_json::json!({ "role": "assistant", "content": [{ "type": "toolCall", "id": format!("c{i}"), "name": "bash", "arguments": {} }] }));
        messages.push(serde_json::json!({ "role": "toolResult", "toolCallId": format!("c{i}"), "content": [{ "type": "text", "text": output }] }));
    }
    let mut transcript = Transcript::new();
    transcript.load_messages(&messages);

    let start = LIVE.load(Ordering::Relaxed);
    let rows = transcript.rows();
    let held = LIVE.load(Ordering::Relaxed) - start;
    assert_eq!(rows.iter().filter(|r| !r.group.is_empty()).count(), CALLS);
    assert!(held < CALLS * output.len() * 3 / 2, "held {} MB for {} MB of output", held / MB, CALLS * output.len() / MB);

    let (again, peak) = peak_of(|| transcript.rows());
    assert!(rows.iter().zip(&again).all(|(a, b)| std::sync::Arc::ptr_eq(a, b)));
    assert!(peak < MB, "showing them again took {} KB", peak / 1024);
}
