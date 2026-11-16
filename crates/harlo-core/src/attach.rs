//! Files attached to a prompt, as blocks ahead of the message text.
//!
//! Each file becomes a `<file name="/abs/path">…</file>` block. Text files are
//! inlined, images also go along as image content, and other files are named so
//! the agent can open them itself.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Text files larger than this are referenced, not inlined.
pub const INLINE_LIMIT: u64 = 512 * 1024;

/// What a prompt carries for its attachments.
#[derive(Debug, Default)]
pub struct Prepared {
    /// `<file>` blocks to put before the message text.
    pub prefix: String,
    pub images: Vec<Value>,
}

fn is_image(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp")
    )
}

/// The file as text, if it is small UTF-8 without NUL bytes.
fn read_inline(path: &Path, size: u64) -> Option<String> {
    if size > INLINE_LIMIT {
        return None;
    }
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(usize::MAX));
    std::fs::File::open(path).ok()?.take(INLINE_LIMIT + 1).read_to_end(&mut bytes).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    Some(text.strip_prefix('\u{feff}').map(str::to_owned).unwrap_or(text))
}

pub(crate) fn human_size(n: u64) -> String {
    match n {
        n if n >= 1 << 20 => format!("{:.1} MB", n as f64 / f64::from(1 << 20)),
        n if n >= 1 << 10 => format!("{:.0} KB", n as f64 / f64::from(1 << 10)),
        n => format!("{n} bytes"),
    }
}

/// Builds the `<file>` blocks and image contents for `paths`. Files that
/// can't be read are noted in their block rather than dropped silently.
pub fn prepare(paths: &[PathBuf]) -> Prepared {
    prepare_for(paths, None)
}

/// A file too big to copy to another machine with a message.
const COPY_LIMIT: u64 = 200 * 1024 * 1024;

/// [`prepare`] for a session on `host` (None: this machine): a file that
/// isn't inlined is copied there first (blocking), so its agent can read it.
pub fn prepare_for(paths: &[PathBuf], host: Option<&str>) -> Prepared {
    let mut out = Prepared::default();
    for path in paths {
        let name = path.display();
        let size = match std::fs::metadata(path) {
            Ok(m) if m.is_file() => m.len(),
            Ok(_) => {
                let _ = writeln!(out.prefix, "<file name=\"{name}\">(not a regular file)</file>");
                continue;
            }
            Err(e) => {
                let _ = writeln!(out.prefix, "<file name=\"{name}\">(could not read: {e})</file>");
                continue;
            }
        };
        if is_image(path) {
            match harlo_harness::image_from_file(path) {
                Ok(img) => {
                    out.images.push(img);
                    let _ = writeln!(out.prefix, "<file name=\"{name}\"></file>");
                }
                Err(e) => _ = writeln!(out.prefix, "<file name=\"{name}\">(could not read: {e:#})</file>"),
            }
            continue;
        }
        if size == 0 {
            let _ = writeln!(out.prefix, "<file name=\"{name}\">(empty file)</file>");
        } else if let Some(text) = read_inline(path, size) {
            let _ = write!(out.prefix, "<file name=\"{name}\">\n{text}\n</file>\n");
        } else {
            let why = if size > INLINE_LIMIT { "too large to include" } else { "binary" };
            match host {
                None => _ = writeln!(out.prefix, 
                    "<file name=\"{name}\">({why}, {}; not included here; read it from disk if needed)</file>",
                    human_size(size)
                ),
                // (The agent there can't see this machine's files.)
                Some(h) => match copy_to(h, path, size) {
                    Ok(there) => _ = writeln!(out.prefix, 
                        "<file name=\"{there}\">({why}, {}; not included here; read it from disk if needed)</file>",
                        human_size(size)
                    ),
                    Err(e) => _ = writeln!(out.prefix, "<file name=\"{name}\">({why}, {}; couldn't copy it to {h}: {e})</file>", human_size(size)),
                },
            }
        }
    }
    out
}

/// Copies a file to another machine's `~/.cache/harlo/attachments` (blocking):
/// its path there.
fn copy_to(host: &str, path: &Path, size: u64) -> anyhow::Result<String> {
    if size > COPY_LIMIT {
        return Err(anyhow::anyhow!("it's over {}", human_size(COPY_LIMIT)));
    }
    let h = crate::remote::host(host).ok_or_else(|| anyhow::anyhow!("there's no machine called {host}"))?;
    let bytes = std::fs::read(path)?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let name = format!("{stamp}-{}", path.file_name().map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned()));
    let script = format!(
        "d=\"$HOME/.cache/harlo/attachments\"; mkdir -p \"$d\" && cat > \"$d\"/{q} && printf '%s/%s' \"$d\" {q}",
        q = crate::remote::quote(&name)
    );
    let out = crate::remote::run_with_input(&h, &script, Some(&bytes), std::time::Duration::from_secs(600)).map_err(|e| anyhow::anyhow!("{e:#}"))?;
    let there = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if !out.status.success() || !there.starts_with('/') {
        return Err(anyhow::Error::msg(crate::remote::ssh_error(host, &out)));
    }
    Ok(there)
}

pub use harlo_harness::file_blocks::split_blocks;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepares_and_splits() {
        let dir = std::env::temp_dir().join(format!("harlo-attach-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let txt = dir.join("notes.md");
        std::fs::write(&txt, "\u{feff}# Title\nbody</file> inside\n").unwrap();
        let bin = dir.join("blob.bin");
        std::fs::write(&bin, [1u8, 0, 2, 3]).unwrap();
        let empty = dir.join("empty.txt");
        std::fs::write(&empty, "").unwrap();
        let png = dir.join("dot.png");
        std::fs::write(&png, [0x89, b'P', b'N', b'G']).unwrap();

        let p = prepare(&[txt.clone(), bin.clone(), empty.clone(), png.clone(), dir.join("missing")]);
        assert_eq!(p.images.len(), 1);
        assert!(p.prefix.contains(&format!("<file name=\"{}\">\n# Title\nbody</file> inside\n\n</file>\n", txt.display())), "{}", p.prefix);
        assert!(p.prefix.contains("(binary, 4 bytes;"));
        assert!(p.prefix.contains("(empty file)"));
        assert!(p.prefix.contains("could not read"));

        let msg = format!("{}Please review these.", p.prefix);
        let (names, rest) = split_blocks(&msg);
        assert_eq!(names.len(), 5);
        assert_eq!(names[0], txt.display().to_string());
        assert_eq!(names[3], png.display().to_string());
        assert_eq!(rest, "Please review these.");

        assert_eq!(split_blocks("no files here"), (vec![], "no files here".into()));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
