//! Installing and updating the engines from their GitHub releases, into
//! `<data>/harlo/engines/<engine>/<version>`, and finding what's installed.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

use futures_util::{Stream, StreamExt};
use serde_json::Value;

use super::{Build, Engine};
use crate::download::{Download, Fetch};

/// The file naming the installed version, inside an engine's folder.
const CURRENT: &str = "current";

/// GitHub's API, unless a test points elsewhere.
const GITHUB: &str = "https://api.github.com";

/// The engines Harlo installed, and where their releases come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Engines {
    /// Where engines are installed.
    folder: PathBuf,
    /// GitHub's API.
    api: String,
    /// Whether an engine on `PATH` counts as installed.
    system: bool,
}

/// An installed engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// Its version, such as `0.40.17` or `b12648`.
    pub version: String,
    /// The program that serves.
    pub program: PathBuf,
    /// Whether Harlo installed it; otherwise it's the system's, on `PATH`.
    pub own: bool,
}

/// A release of an engine: its version and the files this computer needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// Its version.
    pub version: String,
    /// The archives to unpack, together.
    pub assets: Vec<Asset>,
}

/// A file of a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    /// Its name.
    pub name: String,
    /// Where it downloads from.
    pub url: String,
    /// Its size in bytes.
    pub size: u64,
    /// Its SHA-256, as GitHub publishes it.
    pub sha256: Option<String>,
}

/// A step of installing an engine.
#[derive(Debug, Clone, PartialEq)]
pub enum Install {
    /// This much has downloaded, from 0 to 1.
    Downloading(f64),
    /// The archives are being unpacked.
    Unpacking,
    /// It ended: the engine is installed, or why not.
    Done(Result<Installed, String>),
}

impl Engines {
    /// The user's engines, in `$XDG_DATA_HOME/harlo/engines`. `None` when
    /// the system has no data folder.
    #[must_use]
    pub fn user() -> Option<Self> {
        let data = dirs::data_dir()?;
        Some(Self {
            system: true,
            ..Self::new(data.join("harlo").join("engines"), GITHUB)
        })
    }

    /// Engines in `folder`, with releases from the GitHub API at `api`;
    /// only these count, not ones on `PATH`.
    #[must_use]
    pub fn new(folder: impl Into<PathBuf>, api: &str) -> Self {
        Self {
            folder: folder.into(),
            api: api.trim_end_matches('/').to_owned(),
            system: false,
        }
    }

    /// The folder.
    #[must_use]
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// `engine`'s folder.
    fn home(&self, engine: Engine) -> PathBuf {
        self.folder.join(engine.id())
    }

    /// The engine Harlo installed, or else the system's on `PATH` (for the
    /// user's engines).
    #[must_use]
    pub fn installed(&self, engine: Engine) -> Option<Installed> {
        self.own(engine).or_else(|| {
            if !self.system {
                return None;
            }
            let program = which::which(engine.program()).ok()?;
            Some(Installed {
                version: String::new(),
                program,
                own: false,
            })
        })
    }

    /// The engine Harlo installed.
    #[must_use]
    pub fn own(&self, engine: Engine) -> Option<Installed> {
        let home = self.home(engine);
        let version = std::fs::read_to_string(home.join(CURRENT)).ok()?;
        let version = version.trim().to_owned();
        let program = find_program(&home.join(&version), engine.program(), 4)?;
        Some(Installed {
            version,
            program,
            own: true,
        })
    }

    /// The newest release of `engine`, with the files a computer needing
    /// `build` uses.
    ///
    /// # Errors
    ///
    /// When GitHub can't be reached or has no build for this computer.
    pub async fn latest(&self, engine: Engine, build: Build) -> Result<Release, String> {
        let client = reqwest::Client::builder()
            .user_agent("harlo")
            .build()
            .map_err(|e| e.to_string())?;
        let url = match engine {
            Engine::Ollama => format!("{}/repos/{}/releases/latest", self.api, engine.repo()),
            // Its builds are prereleases, newest first.
            Engine::LlamaCpp => format!("{}/repos/{}/releases?per_page=5", self.api, engine.repo()),
        };
        let answer: Value = client
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| format!("Couldn't ask GitHub for {}'s releases: {e}", engine.name()))?
            .json()
            .await
            .map_err(|e| format!("GitHub's answer didn't read: {e}"))?;
        let release = match engine {
            Engine::Ollama => &answer,
            Engine::LlamaCpp => answer
                .as_array()
                .and_then(|list| {
                    list.iter()
                        .find(|r| r["tag_name"].as_str().is_some_and(|t| t.starts_with('b')))
                })
                .ok_or("GitHub listed no llama.cpp builds.")?,
        };
        choose(engine, release, build, cfg!(target_arch = "aarch64"))
    }

    /// Installs `release` of `engine`: downloads, checks and unpacks it,
    /// then makes it the version used and removes older ones. Dropping the
    /// stream stops it.
    pub fn install(
        &self,
        engine: Engine,
        release: Release,
    ) -> impl Stream<Item = Install> + Send + 'static {
        let start = Run::Start(self.home(engine), engine, release);
        futures_util::stream::unfold(Some(start), |run| async move {
            let mut receiver = match run? {
                // The work runs on its own task from the first poll, and
                // stops at its next step once nobody listens.
                Run::Start(home, engine, release) => {
                    let (sender, receiver) = tokio::sync::mpsc::channel(16);
                    tokio::spawn(async move {
                        let done = install(&home, engine, release, &sender).await;
                        let _ = sender.send(Install::Done(done)).await;
                    });
                    receiver
                }
                Run::Going(receiver) => receiver,
            };
            let step = receiver.recv().await?;
            Some((step, Some(Run::Going(receiver))))
        })
    }
}

/// An install's stream: not started, or under way.
enum Run {
    /// Not started: where, what and which release.
    Start(PathBuf, Engine, Release),
    /// Under way, telling its steps.
    Going(tokio::sync::mpsc::Receiver<Install>),
}

/// Picks the files `build` needs from GitHub's `release` of `engine`.
fn choose(engine: Engine, release: &Value, build: Build, arm: bool) -> Result<Release, String> {
    let tag = release["tag_name"]
        .as_str()
        .ok_or("A release has no tag.")?;
    let assets: Vec<Asset> = release["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| {
            Some(Asset {
                name: a["name"].as_str()?.to_owned(),
                url: a["browser_download_url"].as_str()?.to_owned(),
                size: a["size"].as_u64().unwrap_or(0),
                sha256: a["digest"]
                    .as_str()
                    .and_then(|d| d.strip_prefix("sha256:"))
                    .map(str::to_owned),
            })
        })
        .collect();
    let named = |test: &dyn Fn(&str) -> bool| assets.iter().find(|a| test(&a.name)).cloned();
    let cpu = if arm { "arm64" } else { "x64" };
    let wanted: Vec<Option<Asset>> = match engine {
        Engine::Ollama => {
            let base = if arm {
                "ollama-linux-arm64"
            } else {
                "ollama-linux-amd64"
            };
            let mut list = vec![named(&|n| n == format!("{base}.tar.zst"))];
            if build == Build::Rocm && !arm {
                list.push(named(&|n| n == format!("{base}-rocm.tar.zst")));
            }
            list
        }
        Engine::LlamaCpp => {
            let start = format!("llama-{tag}-bin-ubuntu-");
            let end = format!("-{cpu}.tar.gz");
            let kind = |n: &str, what: &str| {
                n.strip_prefix(&start)
                    .and_then(|rest| rest.strip_suffix(&end))
                    .is_some_and(|middle| middle.starts_with(what))
            };
            match build {
                Build::Cuda { cuda13 } => {
                    let cuda = if cuda13 { "cuda-13" } else { "cuda-12" };
                    let runtime = format!("cudart-llama-{tag}-bin-ubuntu-{cuda}");
                    vec![
                        named(&|n| kind(n, cuda)),
                        named(&|n| n.starts_with(&runtime) && n.ends_with(&end)),
                    ]
                }
                Build::Rocm => vec![named(&|n| kind(n, "rocm"))],
                Build::Vulkan => vec![named(&|n| kind(n, "vulkan"))],
                Build::Cpu => vec![named(&|n| {
                    n == format!("llama-{tag}-bin-ubuntu-{cpu}.tar.gz")
                })],
            }
        }
    };
    let assets: Option<Vec<Asset>> = wanted.into_iter().collect();
    let assets =
        assets.ok_or_else(|| format!("{} {tag} has no build for this computer.", engine.name()))?;
    Ok(Release {
        version: tag.trim_start_matches('v').to_owned(),
        assets,
    })
}

/// Downloads, checks and unpacks `release` into `home`, telling `sender`
/// how far it got.
async fn install(
    home: &Path,
    engine: Engine,
    release: Release,
    sender: &tokio::sync::mpsc::Sender<Install>,
) -> Result<Installed, String> {
    let downloads = home.join("downloads");
    let total: u64 = release.assets.iter().map(|a| a.size).sum::<u64>().max(1);
    let mut before = 0_u64;
    let mut archives = Vec::new();
    for asset in &release.assets {
        let path = downloads.join(&asset.name);
        let mut steps = Box::pin(
            Fetch {
                url: asset.url.clone(),
                path: path.clone(),
                size: asset.size,
                sha256: asset.sha256.clone(),
            }
            .download(),
        );
        while let Some(step) = steps.next().await {
            match step {
                Download::Progress(fraction) => {
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "a fraction for a progress bar; sizes are far below 2^52"
                    )]
                    let overall = (before as f64 + fraction * asset.size as f64) / total as f64;
                    if sender.send(Install::Downloading(overall)).await.is_err() {
                        // Nobody's listening: stopped.
                        return Err("Stopped.".to_owned());
                    }
                }
                Download::Done(result) => result?,
            }
        }
        before += asset.size;
        archives.push(path);
    }
    let _ = sender.send(Install::Unpacking).await;
    let version = release.version.clone();
    let home = home.to_owned();
    tokio::task::spawn_blocking(move || put_in_place(&home, engine, &version, &archives))
        .await
        .map_err(|e| format!("Unpacking stopped: {e}"))?
}

/// Unpacks `archives` as `version` of `engine` in `home`, makes it current
/// and removes the archives and older versions.
fn put_in_place(
    home: &Path,
    engine: Engine,
    version: &str,
    archives: &[PathBuf],
) -> Result<Installed, String> {
    let fresh = home.join(format!("{version}.unpacking"));
    let _ = std::fs::remove_dir_all(&fresh);
    std::fs::create_dir_all(&fresh).map_err(|e| format!("Couldn't unpack: {e}"))?;
    for archive in archives {
        unpack(archive, &fresh)
            .map_err(|e| format!("Couldn't unpack {}: {e}", archive.display()))?;
    }
    let program = find_program(&fresh, engine.program(), 4)
        .ok_or_else(|| format!("The download has no {} program in it.", engine.program()))?;
    let target = home.join(version);
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(&fresh, &target).map_err(|e| format!("Couldn't install: {e}"))?;
    std::fs::write(home.join(CURRENT), version).map_err(|e| format!("Couldn't install: {e}"))?;
    let _ = std::fs::remove_dir_all(home.join("downloads"));
    // Older versions go; the new one is in use from now on.
    if let Ok(entries) = std::fs::read_dir(home) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let path = entry.path();
            if path.is_dir() && name != *version {
                let _ = std::fs::remove_dir_all(path);
            }
        }
    }
    let relative = program.strip_prefix(&fresh).unwrap_or(&program).to_owned();
    Ok(Installed {
        version: version.to_owned(),
        program: target.join(relative),
        own: true,
    })
}

/// Unpacks a `.tar.gz` or `.tar.zst` `archive` into `folder`.
fn unpack(archive: &Path, folder: &Path) -> std::io::Result<()> {
    let file = std::io::BufReader::new(std::fs::File::open(archive)?);
    let name = archive.to_string_lossy();
    let reader: Box<dyn std::io::Read> = if name.ends_with(".tar.zst") {
        Box::new(
            ruzstd::decoding::StreamingDecoder::new(file)
                .map_err(|e| std::io::Error::other(e.to_string()))?,
        )
    } else {
        Box::new(flate2::read::GzDecoder::new(file))
    };
    let mut tar = tar::Archive::new(reader);
    tar.set_preserve_permissions(true);
    // `unpack` keeps every entry inside `folder`.
    tar.unpack(folder)
}

/// The file called `program` in `folder` or below, `depth` folders deep at
/// most.
fn find_program(folder: &Path, program: &str, depth: usize) -> Option<PathBuf> {
    let candidate = folder.join(program);
    if candidate.is_file() {
        return Some(candidate);
    }
    if depth == 0 {
        return None;
    }
    let mut folders: Vec<PathBuf> = std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    folders.sort();
    folders
        .iter()
        .find_map(|sub| find_program(sub, program, depth - 1))
}

/// Whether version `latest` is newer than `installed`: their numbers
/// compared in turn, so `b12648` > `b12600` and `0.40.17` > `0.40.9`.
#[must_use]
pub fn newer(latest: &str, installed: &str) -> bool {
    let numbers = |v: &str| -> Vec<u64> {
        v.split(|c: char| !c.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .filter_map(|part| part.parse().ok())
            .collect()
    };
    numbers(latest) > numbers(installed)
}
