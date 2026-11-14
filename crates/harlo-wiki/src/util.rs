//! What the writer shares: the time, new ids, atomic writes and JSON
//! written as OpenWiki writes it.
//!
//! For the writing bench, WIKI_TEST_NOW fixes the time and WIKI_TEST_IDS
//! (a file holding a count) makes a run's, its page jobs' and its Claims'
//! ids counted (00000000-0000-4000-8000-<n>), as the bench's reference run
//! has them, so the two can be compared byte for byte.

use serde_json::Value;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Now as JavaScript's `toISOString` ("2026-01-02T03:04:05.678Z").
pub fn now_iso() -> String {
    if let Ok(t) = std::env::var("WIKI_TEST_NOW") {
        if !t.is_empty() {
            return t;
        }
    }
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    iso_from_millis(ms)
}

pub fn iso_from_millis(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let rem = ms.rem_euclid(86_400_000);
    // (Days to a civil date: Howard Hinnant's algorithm.)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z", rem / 3_600_000, rem / 60_000 % 60, rem / 1000 % 60, rem % 1000)
}

fn random_bytes() -> [u8; 16] {
    let mut b = [0u8; 16];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| std::io::Read::read_exact(&mut f, &mut b)).is_err() {
        static N: AtomicU64 = AtomicU64::new(0);
        let t = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0) as u64;
        let x = t ^ (std::process::id() as u64) << 32 ^ N.fetch_add(1, Ordering::SeqCst).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        b[..8].copy_from_slice(&x.to_le_bytes());
        b[8..].copy_from_slice(&x.rotate_left(29).wrapping_mul(0xbf58_476d_1ce4_e5b9).to_le_bytes());
    }
    b
}

/// A random version-4 UUID.
pub fn random_uuid() -> String {
    let mut b = random_bytes();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

/// A new id for a run, a page job or a Claim (counted under WIKI_TEST_IDS).
pub fn new_id() -> String {
    match std::env::var_os("WIKI_TEST_IDS").filter(|p| !p.is_empty()) {
        None => random_uuid(),
        Some(file) => {
            let n: u64 = std::fs::read_to_string(&file).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0) + 1;
            let _ = std::fs::write(&file, n.to_string());
            format!("00000000-0000-4000-8000-{n:012}")
        }
    }
}

/// `JSON.stringify(value, null, 2)`.
pub fn json_pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

/// Writes `content` to `path` through a temporary file beside it, created
/// with an existing file's permissions, else `default_mode` (less the
/// umask, as Node does).
pub fn write_text_atomic(path: &Path, content: &str, default_mode: u32) -> std::io::Result<()> {
    let mode = match std::fs::symlink_metadata(path) {
        #[cfg(unix)]
        Ok(m) => std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o777,
        #[cfg(not(unix))]
        Ok(_) => default_mode,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => default_mode,
        Err(e) => return Err(e),
    };
    write_new_then_rename(path, content, mode)
}

/// Writes `content` to `path` through a new temporary file beside it,
/// created with `mode` (less the umask).
pub fn write_new_then_rename(path: &Path, content: &str, mode: u32) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", random_uuid()));
    let result = (|| {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut opts, mode);
        #[cfg(not(unix))]
        let _ = mode;
        let mut f = opts.open(&tmp)?;
        f.write_all(content.as_bytes())?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_times() {
        assert_eq!(iso_from_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_from_millis(1_767_323_045_678), "2026-01-02T03:04:05.678Z");
        assert_eq!(iso_from_millis(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }

    #[test]
    fn uuids() {
        let u = random_uuid();
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
        assert!(matches!(&u[19..20], "8" | "9" | "a" | "b"));
    }
}

/// An I/O error as Node words it ("ENOENT: no such file or directory,
/// open '/x'").
pub fn node_io_error(e: &std::io::Error, syscall: &str, path: &Path) -> String {
    let (code, text) = match e.raw_os_error() {
        Some(2) => ("ENOENT", "no such file or directory"),
        Some(13) => ("EACCES", "permission denied"),
        Some(20) => ("ENOTDIR", "not a directory"),
        Some(21) => ("EISDIR", "illegal operation on a directory"),
        Some(40) => ("ELOOP", "too many symbolic links encountered"),
        Some(1) => ("EPERM", "operation not permitted"),
        Some(28) => ("ENOSPC", "no space left on device"),
        Some(30) => ("EROFS", "read-only file system"),
        _ => return format!("{e}, {syscall} '{}'", path.display()),
    };
    format!("{code}: {text}, {syscall} '{}'", path.display())
}

/// JavaScript's `<` on strings: by UTF-16 code units.
pub fn cmp16(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// `[...value].slice(0, limit)` and "…" when it was longer (code points).
pub fn truncate_points(value: &str, limit: usize) -> String {
    match value.char_indices().nth(limit) {
        None => value.to_owned(),
        Some((i, _)) => format!("{}…", &value[..i]),
    }
}
