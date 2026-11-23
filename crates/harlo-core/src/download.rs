//! Downloading a file to this computer, a piece at a time, with how far it
//! got: speech models, engines and language models all come this way.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

use futures_util::Stream;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

/// A step of a download.
#[derive(Debug, Clone, PartialEq)]
pub enum Download {
    /// This much of it has arrived, from 0 to 1.
    Progress(f64),
    /// It ended: the file is in place, or why not.
    Done(Result<(), String>),
}

/// What to download, and where to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetch {
    /// The file's address.
    pub url: String,
    /// Where it goes once whole.
    pub path: PathBuf,
    /// Its size, for progress when the server doesn't say.
    pub size: u64,
    /// Its SHA-256, in hex, when known; a file that doesn't match is
    /// thrown away.
    pub sha256: Option<String>,
}

impl Fetch {
    /// Downloads the file, telling how far it got; dropping the stream
    /// stops it, and the next download starts over. Only a whole file, with
    /// the right checksum, takes the final name.
    ///
    /// The stream gives [`Download::Progress`] steps and ends with one
    /// [`Download::Done`].
    pub fn download(self) -> impl Stream<Item = Download> + Send + 'static {
        futures_util::stream::unfold(State::Start(self), State::step)
    }
}

/// Where a download stands.
enum State {
    /// Not asked for yet.
    Start(Fetch),
    /// Arriving.
    Going(Box<Going>),
    /// Over.
    Finished,
}

/// A download under way.
struct Going {
    /// The answer, read a piece at a time.
    response: reqwest::Response,
    /// The partial file.
    file: tokio::fs::File,
    /// Its name while partial.
    part: PathBuf,
    /// What's being fetched.
    fetch: Fetch,
    /// The checksum so far.
    hash: Sha256,
    /// How many bytes have arrived.
    got: u64,
    /// How many are coming.
    total: u64,
    /// The fraction last reported.
    reported: f64,
}

impl State {
    /// Takes the download one step on.
    async fn step(self) -> Option<(Download, Self)> {
        match self {
            Self::Start(fetch) => Some(match Going::open(fetch).await {
                Ok(going) => (Download::Progress(0.0), Self::Going(Box::new(going))),
                Err(why) => (Download::Done(Err(why)), Self::Finished),
            }),
            Self::Going(mut going) => Some(match going.next().await {
                Ok(Some(fraction)) => (Download::Progress(fraction), Self::Going(going)),
                Ok(None) => (Download::Done(going.finish().await), Self::Finished),
                Err(why) => {
                    let _ = tokio::fs::remove_file(&going.part).await;
                    (Download::Done(Err(why)), Self::Finished)
                }
            }),
            Self::Finished => None,
        }
    }
}

impl Going {
    /// Asks for the file.
    async fn open(fetch: Fetch) -> Result<Self, String> {
        let folder = fetch.path.parent().unwrap_or_else(|| Path::new("."));
        tokio::fs::create_dir_all(folder)
            .await
            .map_err(|e| format!("Couldn't make {}: {e}", folder.display()))?;
        let response = reqwest::get(&fetch.url)
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| format!("Couldn't download {}: {e}", name(&fetch.path)))?;
        let mut part = fetch.path.clone().into_os_string();
        part.push(".part");
        let part = PathBuf::from(part);
        let file = tokio::fs::File::create(&part)
            .await
            .map_err(|e| format!("Couldn't save {}: {e}", name(&fetch.path)))?;
        Ok(Self {
            total: response.content_length().unwrap_or(fetch.size).max(1),
            response,
            file,
            part,
            fetch,
            hash: Sha256::new(),
            got: 0,
            reported: 0.0,
        })
    }

    /// Reads on until another hundredth has arrived; `None` once all has.
    async fn next(&mut self) -> Result<Option<f64>, String> {
        while let Some(piece) = self
            .response
            .chunk()
            .await
            .map_err(|e| format!("The download broke off: {e}"))?
        {
            self.file
                .write_all(&piece)
                .await
                .map_err(|e| format!("Couldn't save {}: {e}", name(&self.fetch.path)))?;
            self.hash.update(&piece);
            self.got += piece.len() as u64;
            #[expect(
                clippy::cast_precision_loss,
                reason = "a fraction for a progress bar; file sizes are far below 2^52"
            )]
            let fraction = (self.got as f64 / self.total as f64).min(1.0);
            if fraction - self.reported >= 0.01 {
                self.reported = fraction;
                return Ok(Some(fraction));
            }
        }
        Ok(None)
    }

    /// Checks the whole file and gives it its name.
    async fn finish(mut self) -> Result<(), String> {
        let saved = async {
            self.file.flush().await?;
            self.file.sync_all().await?;
            Ok::<_, std::io::Error>(())
        }
        .await;
        let sum = hex::encode(self.hash.finalize());
        let result = if let Err(e) = saved {
            Err(format!("Couldn't save {}: {e}", name(&self.fetch.path)))
        } else if self.got < self.total {
            Err(format!(
                "The download stopped early, at {} of {} bytes.",
                self.got, self.total
            ))
        } else if let Some(want) = self.fetch.sha256.as_deref()
            && !want.eq_ignore_ascii_case(&sum)
        {
            Err(format!(
                "{} arrived damaged: its checksum doesn't match.",
                name(&self.fetch.path)
            ))
        } else {
            tokio::fs::rename(&self.part, &self.fetch.path)
                .await
                .map_err(|e| format!("Couldn't save {}: {e}", name(&self.fetch.path)))
        };
        if result.is_err() {
            let _ = tokio::fs::remove_file(&self.part).await;
        }
        result
    }
}

/// The file name of `path`, for messages.
fn name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}
