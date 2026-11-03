//! Copies of session files on other machines, kept here so they're read as
//! local ones are (a line at a time, from the open cache) and still open
//! with the host offline. A session file only grows: bringing its copy up to
//! date fetches only what was added, once the copy's last bytes are checked
//! against the host's (a file that changed otherwise comes again, whole).
//! What comes over SSH goes straight to the copy, never all into memory.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The copy's last bytes, checked against the host's.
const CHECK: u64 = 64;
/// Read this recently: as good as now (one open reads a session more than
/// once).
const FRESH: Duration = Duration::from_secs(2);
/// Nothing came for this long: the connection is taken as dropped (and the
/// copy stays as it is).
const STALL: Duration = Duration::from_secs(20);
/// A copy not used for this long goes.
const KEPT: Duration = Duration::from_secs(30 * 86400);

/// Where the copies are kept.
pub fn dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .map(|d| d.join("harlo/remote"))
}

/// The folder for `remote`'s copy on `host` (FNV-1a of both: a stable name).
fn slot(dir: &Path, host: &str, remote: &str) -> PathBuf {
    let hash = host.bytes().chain([0]).chain(remote.bytes()).fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3));
    dir.join(format!("{hash:016x}"))
}

/// Where `remote`'s copy on `host` is (it may not exist yet): under its own
/// name, in a folder of its own.
pub fn copy_path(host: &str, remote: &str) -> Option<PathBuf> {
    let name = Path::new(remote).file_name()?;
    Some(slot(&dir()?, host, remote).join(name))
}

/// What fetching found.
enum Fetched {
    /// The host's file is gone.
    Gone,
    /// The host couldn't be reached (or the command failed).
    Unreachable,
    /// The copy isn't the start of the host's file any more.
    Changed,
    /// Brought up to date (`bytes` added).
    Done(u64),
}

/// Brings the copy of `remote` on `host` up to date and returns where it is.
/// `command` makes the ssh command that runs a script on the host. With the
/// host unreachable, the copy as it was, if there is one.
pub fn refresh(host: &str, remote: &str, command: impl Fn(&str) -> Command) -> std::io::Result<PathBuf> {
    let dir = dir().ok_or_else(|| std::io::Error::other("no cache folder"))?;
    refresh_in(&dir, host, remote, command)
}

fn refresh_in(dir: &Path, host: &str, remote: &str, command: impl Fn(&str) -> Command) -> std::io::Result<PathBuf> {
    let name = Path::new(remote).file_name().ok_or_else(|| std::io::Error::other("not a file"))?;
    let slot = slot(dir, host, remote);
    let copy = slot.join(name);
    // One refresh of a copy at a time; one just done stands.
    let lock = {
        static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<Option<Instant>>>>>> = OnceLock::new();
        LOCKS.get_or_init(Default::default).lock().unwrap().entry(copy.clone()).or_default().clone()
    };
    let mut last = lock.lock().unwrap();
    if last.is_some_and(|t| t.elapsed() < FRESH) && copy.is_file() {
        return Ok(copy);
    }
    prune_once(dir);
    std::fs::create_dir_all(&slot)?;
    // (What a session says is yours alone, on a shared PC too.)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let mut size = None;
    let mut fetched = fetch(&command, remote, &copy, false, STALL, &mut size);
    if matches!(fetched, Fetched::Changed) {
        fetched = fetch(&command, remote, &copy, true, STALL, &mut size);
    }
    // How big the host's file was (what the copy is short of, if it didn't
    // all come); as before when the host didn't say.
    let source = slot.join("source.json");
    let size = size.or_else(|| read_source(&source)["size"].as_u64()).unwrap_or(0);
    let note = |used: u64| {
        let _ = std::fs::write(&source, serde_json::json!({ "host": host, "path": remote, "used": used, "size": size }).to_string());
    };
    match fetched {
        Fetched::Gone => {
            let _ = std::fs::remove_dir_all(&slot);
            *last = None;
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("{remote} is gone from {host}")))
        }
        Fetched::Unreachable | Fetched::Changed if copy.is_file() => {
            tracing::debug!("{host}: {remote} as last copied");
            note(read_source(&source)["used"].as_u64().unwrap_or(0));
            Ok(copy)
        }
        Fetched::Unreachable | Fetched::Changed => Err(std::io::Error::other(format!("couldn't read {remote} on {host}"))),
        Fetched::Done(added) => {
            tracing::debug!("{host}: {remote} +{added} bytes");
            note(SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()));
            // (Cut short: the rest is asked for again next time, not in a moment.)
            if short_by(&copy, size) == 0 {
                *last = Some(Instant::now());
            } else {
                tracing::warn!("{host}: {remote} came over only in part ({} of {size} bytes)", size - short_by(&copy, size));
            }
            Ok(copy)
        }
    }
}

fn read_source(source: &Path) -> serde_json::Value {
    std::fs::read(source).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn short_by(copy: &Path, size: u64) -> u64 {
    size.saturating_sub(std::fs::metadata(copy).map_or(0, |m| m.len()))
}

/// How many bytes of the host's file its copy lacks, as far as harlo knows
/// (the size the host last said, less what came over): 0 when it all came.
/// A copy is the start of the host's file, so what's missing is its end.
pub fn missing(copy: &Path) -> u64 {
    copy.parent().map_or(0, |slot| short_by(copy, read_source(&slot.join("source.json"))["size"].as_u64().unwrap_or(0)))
}

/// The host's file from where the copy ends (`whole`: from its start, into
/// a new copy), checking the copy's last bytes first. Given up when nothing
/// comes for `stall`. `size`: the host's file's size, once it says.
fn fetch(command: &impl Fn(&str) -> Command, remote: &str, copy: &Path, whole: bool, stall: Duration, size_out: &mut Option<u64>) -> Fetched {
    let have = if whole { 0 } else { std::fs::metadata(copy).map_or(0, |m| m.len()) };
    let check = have.min(CHECK);
    let mut tail = vec![0; check as usize];
    if check > 0 {
        let read = std::fs::File::open(copy).and_then(|mut f| {
            f.seek(SeekFrom::Start(have - check))?;
            f.read_exact(&mut tail)
        });
        if read.is_err() {
            return Fetched::Changed;
        }
    }
    // Its size, then the bytes from the copy's last `check` on (no more
    // than that size: a line being written comes next time).
    let q = crate::process::shell_quote(remote);
    let script = format!(
        r#"f={q}; [ -f "$f" ] || {{ echo gone; exit 0; }}; n=$(wc -c < "$f" | tr -d ' '); echo "$n"; [ "$n" -ge {have} ] && tail -c +{start} -- "$f" | head -c $((n - {have} + {check})); exit 0"#,
        start = have - check + 1
    );
    let mut cmd = command(&script);
    let Ok(mut child) = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else { return Fetched::Unreachable };
    let stdout = child.stdout.take().expect("piped");
    let child = Arc::new(Mutex::new(child));
    // Ended when nothing has come for `stall` (the read then ends too).
    let (start, heard) = (Instant::now(), Arc::new(AtomicU64::new(0)));
    let (_running, ended) = mpsc::channel::<()>();
    let stalled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let (child, heard, stalled) = (Arc::clone(&child), Arc::clone(&heard), Arc::clone(&stalled));
        std::thread::spawn(move || {
            while let Err(mpsc::RecvTimeoutError::Timeout) = ended.recv_timeout(Duration::from_millis(250)) {
                if start.elapsed().saturating_sub(Duration::from_millis(heard.load(Ordering::Relaxed))) > stall {
                    stalled.store(true, Ordering::Relaxed);
                    let _ = child.lock().unwrap().kill();
                    break;
                }
            }
        });
    }
    let mut out = BufReader::new(Heard { inner: stdout, start, heard });
    let result = (|| {
        let mut header = String::new();
        out.read_line(&mut header).ok()?;
        let header = header.trim();
        if header == "gone" {
            return Some(Fetched::Gone);
        }
        let size: u64 = header.parse().ok()?;
        *size_out = Some(size);
        if size < have {
            return Some(Fetched::Changed);
        }
        let mut seen = vec![0; check as usize];
        if out.read_exact(&mut seen).is_err() || seen != tail {
            return Some(Fetched::Changed);
        }
        if whole {
            let tmp = copy.with_extension("part");
            let added = std::fs::File::create(&tmp).and_then(|mut f| std::io::copy(&mut out, &mut f));
            return match added.and_then(|n| std::fs::rename(&tmp, copy).map(|()| n)) {
                Ok(n) => Some(Fetched::Done(n)),
                Err(_) => {
                    let _ = std::fs::remove_file(&tmp);
                    None
                }
            };
        }
        // What's added goes on the end (cut short, it's still the start of
        // the host's file: the rest comes next time).
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(copy).ok()?;
        let n = std::io::copy(&mut out, &mut f).ok()?;
        f.flush().ok()?;
        Some(Fetched::Done(n))
    })();
    let mut child = child.lock().unwrap();
    let _ = child.kill();
    let stalled = stalled.load(Ordering::Relaxed);
    let ok = child.wait().is_ok_and(|s| s.success()) && !stalled;
    match result {
        // (What came stays: the start of the host's file. The rest, next time.)
        _ if stalled => Fetched::Unreachable,
        Some(r @ (Fetched::Gone | Fetched::Changed)) => r,
        Some(done @ Fetched::Done(_)) => done,
        // (A whole copy cut short by a dropped connection isn't kept.)
        _ if ok => Fetched::Changed,
        _ => Fetched::Unreachable,
    }
}

/// A reader that notes when it last got something.
struct Heard<R> {
    inner: R,
    start: Instant,
    heard: Arc<AtomicU64>,
}

impl<R: Read> Read for Heard<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.heard.store(self.start.elapsed().as_millis() as u64, Ordering::Relaxed);
        Ok(n)
    }
}

/// A deleted session's copy goes with it.
pub fn forget(host: &str, remote: &str) {
    if let Some(dir) = dir() {
        let slot = slot(&dir, host, remote);
        if let Some(copy) = Path::new(remote).file_name().map(|n| slot.join(n)) {
            crate::claude::forget_cached(&copy);
        }
        let _ = std::fs::remove_dir_all(slot);
    }
}

/// Copies not used for [`KEPT`] go (once a run).
fn prune_once(dir: &Path) {
    static PRUNED: OnceLock<()> = OnceLock::new();
    if PRUNED.set(()).is_err() {
        return;
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    for slot in std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()) {
        let used = std::fs::read_to_string(slot.join("source.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|v| v["used"].as_u64());
        if used.is_none_or(|u| now.saturating_sub(u) > KEPT.as_secs()) {
            if let Some(copy) = std::fs::read_dir(&slot).into_iter().flatten().flatten().map(|e| e.path()).find(|p| p.file_name().is_some_and(|n| n != "source.json")) {
                crate::claude::forget_cached(&copy);
            }
            let _ = std::fs::remove_dir_all(&slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in host: the script runs here, in `sh`.
    fn here(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.arg("-c").arg(script);
        c
    }

    #[test]
    fn a_copy_takes_only_what_was_added() {
        let root = std::env::temp_dir().join(format!("harlo-mirror-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (host_dir, cache) = (root.join("host"), root.join("cache"));
        std::fs::create_dir_all(&host_dir).unwrap();
        let remote = host_dir.join("s.jsonl");
        let remote_s = remote.to_string_lossy().into_owned();
        let line = |i: usize| format!("{{\"n\":{i},\"text\":\"{}\"}}\n", "x".repeat(i % 90));
        std::fs::write(&remote, (0..100).map(line).collect::<String>()).unwrap();

        let copy = refresh_in(&cache, "nas", &remote_s, here).unwrap();
        assert_eq!(std::fs::read(&copy).unwrap(), std::fs::read(&remote).unwrap(), "first: the whole file");

        // Grown: only what's new comes (a copy just made stands for a moment).
        let mut f = std::fs::OpenOptions::new().append(true).open(&remote).unwrap();
        f.write_all((100..150).map(line).collect::<String>().as_bytes()).unwrap();
        drop(f);
        let size = std::fs::metadata(&copy).unwrap().len();
        assert_eq!(std::fs::metadata(refresh_in(&cache, "nas", &remote_s, here).unwrap()).unwrap().len(), size, "just read: as it was");
        std::thread::sleep(FRESH);
        let fresh = |what: &str| {
            std::thread::sleep(FRESH);
            let c = refresh_in(&cache, "nas", &remote_s, here).unwrap();
            assert_eq!(std::fs::read(&c).unwrap(), std::fs::read(&remote).unwrap(), "{what}");
        };
        fresh("grown");

        // Rewritten (the same size, other bytes at the end), cut shorter,
        // then grown again: whole each time it isn't the start.
        let mut text = std::fs::read_to_string(&remote).unwrap();
        text.replace_range(text.len() - 10.., "ABCDEFGHI\n");
        std::fs::write(&remote, &text).unwrap();
        fresh("rewritten");
        std::fs::write(&remote, &text[..text.len() / 2]).unwrap();
        fresh("cut shorter");
        std::fs::write(&remote, (0..300).map(line).collect::<String>()).unwrap();
        fresh("grown again from another start");

        // Unreachable: the copy as it was.
        let before = std::fs::read(&copy).unwrap();
        std::thread::sleep(FRESH);
        let down = |_: &str| Command::new("false");
        assert_eq!(std::fs::read(refresh_in(&cache, "nas", &remote_s, down).unwrap()).unwrap(), before);
        assert!(refresh_in(&cache, "nas", "/elsewhere/never.jsonl", down).is_err(), "nothing to fall back on");

        // Gone from the host: so is the copy.
        std::fs::remove_file(&remote).unwrap();
        std::thread::sleep(FRESH);
        assert_eq!(refresh_in(&cache, "nas", &remote_s, here).unwrap_err().kind(), std::io::ErrorKind::NotFound);
        assert!(!copy.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_copy_cut_short_says_what_it_lacks() {
        let root = std::env::temp_dir().join(format!("harlo-mirror-short-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (host_dir, cache) = (root.join("host"), root.join("cache"));
        std::fs::create_dir_all(&host_dir).unwrap();
        let remote = host_dir.join("s.jsonl");
        let remote_s = remote.to_string_lossy().into_owned();
        std::fs::write(&remote, (0..200).map(|i| format!("{{\"n\":{i}}}\n")).collect::<String>()).unwrap();
        // The connection drops after 300 bytes (ssh's exit then).
        let dropped = |script: &str| {
            let mut c = Command::new("sh");
            c.arg("-c").arg(format!("sh -c {} | head -c 300; exit 255", crate::process::shell_quote(script)));
            c
        };
        let copy = refresh_in(&cache, "nas", &remote_s, dropped).unwrap();
        let whole = std::fs::metadata(&remote).unwrap().len();
        let got = std::fs::metadata(&copy).unwrap().len();
        assert!(got < whole);
        assert_eq!(missing(&copy), whole - got);
        // Offline: still short by as much.
        assert_eq!(missing(&refresh_in(&cache, "nas", &remote_s, |_: &str| Command::new("false")).unwrap()), whole - got);
        // Back: the rest comes at once (a short copy isn't kept as fresh).
        let copy = refresh_in(&cache, "nas", &remote_s, here).unwrap();
        assert_eq!(std::fs::read(&copy).unwrap(), std::fs::read(&remote).unwrap());
        assert_eq!(missing(&copy), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_stalled_connection_is_given_up() {
        let root = std::env::temp_dir().join(format!("harlo-mirror-stall-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let copy = root.join("s.jsonl");
        std::fs::write(&copy, "{\"n\":1}\n").unwrap();
        // Its size, then nothing.
        let hung = |_: &str| {
            let mut c = Command::new("sh");
            c.arg("-c").arg("echo 100; exec sleep 30");
            c
        };
        let t = Instant::now();
        assert!(matches!(fetch(&hung, "/x/s.jsonl", &copy, false, Duration::from_secs(1), &mut None), Fetched::Unreachable), "not taken as changed (and fetched whole)");
        assert!(t.elapsed() < Duration::from_secs(5), "took {:?}", t.elapsed());
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), "{\"n\":1}\n", "the copy as it was");
        let _ = std::fs::remove_dir_all(&root);
    }
}
