//! Where a long session's old text waits: the replies far above what's
//! shown keep their rows' short parts in memory (what a list, a run's line
//! or a search of your messages needs) and their long text, and the blocks
//! it was made from, in a file of this process's in the cache, read back
//! when a row is shown or anything comes for its reply.

use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::PathBuf;
use std::sync::Arc;

/// The file; gone when nothing points into it any more (a row the window
/// still has keeps it, after its transcript was loaded again).
pub struct PageFile {
    path: PathBuf,
    file: File,
}

impl Drop for PageFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// One record in a page file.
#[derive(Clone)]
pub struct Paged {
    page: Arc<PageFile>,
    at: u64,
    len: u32,
}

impl PartialEq for Paged {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.page, &other.page) && self.at == other.at && self.len == other.len
    }
}
impl Eq for Paged {}

impl std::fmt::Debug for Paged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Paged({}+{})", self.at, self.len)
    }
}

impl Paged {
    pub fn read(&self) -> std::io::Result<Vec<u8>> {
        let mut buf = vec![0; self.len as usize];
        self.page.file.read_exact_at(&mut buf, self.at)?;
        Ok(buf)
    }
}

/// A transcript's page file, made when the first record is written.
#[derive(Default)]
pub struct Pages {
    page: Option<Arc<PageFile>>,
    end: u64,
    /// Couldn't be made or written: what would go there stays in memory.
    failed: bool,
    /// Where it's made (tests); the cache's `pages` folder otherwise.
    pub dir: Option<PathBuf>,
}

impl Pages {
    /// Pages made in `dir` (None: the cache's).
    pub fn in_dir(dir: Option<PathBuf>) -> Self {
        Pages { dir, ..Default::default() }
    }

    /// Writes a record; None: it stays in memory (no file to be had).
    pub fn write(&mut self, bytes: &[u8]) -> Option<Paged> {
        if self.failed || bytes.len() > u32::MAX as usize {
            return None;
        }
        if self.page.is_none() {
            let dir = self.dir.clone().or_else(|| harlo_harness::claude::cache_dir().map(|d| d.join("pages")));
            match dir.ok_or_else(|| std::io::Error::other("no cache folder")).and_then(|d| harlo_harness::store::own_file(&d, "pages")) {
                Ok((path, file)) => self.page = Some(Arc::new(PageFile { path, file })),
                Err(e) => {
                    tracing::warn!("long sessions stay whole in memory: no page file ({e})");
                    self.failed = true;
                    return None;
                }
            }
        }
        let page = self.page.clone()?;
        if let Err(e) = page.file.write_all_at(bytes, self.end) {
            tracing::warn!("long sessions stay whole in memory: the page file can't be written ({e})");
            self.failed = true;
            return None;
        }
        let at = self.end;
        self.end += bytes.len() as u64;
        Some(Paged { page, at, len: bytes.len() as u32 })
    }

    /// It couldn't be made or written: nothing more goes to it.
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// The file, while there is one (tests).
    pub fn path(&self) -> Option<PathBuf> {
        self.page.as_ref().map(|p| p.path.clone())
    }
}
