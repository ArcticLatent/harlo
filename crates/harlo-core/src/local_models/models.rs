//! The models that suit agents and fit this computer, and getting them:
//! Ollama pulls a GGUF from Hugging Face by name, and llama.cpp's files are
//! downloaded into `<data>/harlo/models`.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

use futures_util::{Stream, StreamExt};
use llmfit_core::fit::{FitLevel, ModelFit};
use llmfit_core::hardware::SystemSpecs;
use llmfit_core::models::ModelDatabase;
use serde_json::{Value, json};

use crate::download::{Download, Fetch};

/// Hugging Face, unless a test points elsewhere.
const HUGGING_FACE: &str = "https://huggingface.co";

/// Models known to work well with agents' tools, the most capable first,
/// by their name in llmfit's database.
const SUITED: [&str; 11] = [
    "Qwen/Qwen3.6-35B-A3B",
    "Qwen/Qwen3.6-27B",
    "Qwen/Qwen3-Coder-30B-A3B-Instruct",
    "zai-org/GLM-4.7-Flash",
    "openai/gpt-oss-20b",
    "mistralai/Devstral-Small-2-24B-Instruct-2512",
    "google/gemma-4-26B-A4B-it",
    "Qwen/Qwen3.5-9B",
    "google/gemma-4-12B-it",
    "Qwen/Qwen3.5-4B",
    "google/gemma-4-E4B-it",
];

/// How well a model fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Fit {
    /// In the graphics card's memory with room to spare.
    Well,
    /// It fits.
    Fits,
    /// Only just; little room for its context.
    Tight,
}

impl Fit {
    /// How Settings says it.
    #[must_use]
    pub fn words(self) -> &'static str {
        match self {
            Self::Well => "fits well",
            Self::Fits => "fits",
            Self::Tight => "tight",
        }
    }
}

/// A model that suits agents and fits this computer.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    /// Its short name, such as `Qwen3-Coder-30B-A3B-Instruct`.
    pub title: String,
    /// The Hugging Face repository of its GGUF files.
    pub repo: String,
    /// The quantization that fits best, such as `Q6_K`.
    pub quant: String,
    /// The memory it needs, in GB.
    pub memory_gb: f64,
    /// The context that fits beside it, in tokens.
    pub context: u32,
    /// How well it fits.
    pub fit: Fit,
}

impl Suggestion {
    /// The name Ollama pulls it by.
    #[must_use]
    pub fn ollama_tag(&self) -> String {
        format!("hf.co/{}:{}", self.repo, self.quant)
    }

    /// The model's ID in OpenCode, lower case.
    #[must_use]
    pub fn id(&self) -> String {
        self.title.to_lowercase()
    }
}

/// The suited models that fit `specs`, the most capable first.
#[must_use]
pub fn suggest(specs: &SystemSpecs) -> Vec<Suggestion> {
    let database = ModelDatabase::embedded();
    SUITED
        .iter()
        .filter_map(|name| {
            let model = database.get_all_models().iter().find(|m| m.name == *name)?;
            let source = model.gguf_sources.first()?;
            let fitted = ModelFit::analyze(model, specs);
            let fit = match fitted.fit_level {
                FitLevel::Perfect => Fit::Well,
                FitLevel::Good => Fit::Fits,
                FitLevel::Marginal => Fit::Tight,
                FitLevel::TooTight => return None,
            };
            Some(Suggestion {
                title: name.rsplit('/').next().unwrap_or(name).to_owned(),
                repo: source.repo.clone(),
                quant: fitted.best_quant.clone(),
                memory_gb: fitted.memory_required_gb,
                context: fitted.usable_context,
                fit,
            })
        })
        .collect()
}

/// The models the Ollama server at `base` has.
///
/// # Errors
///
/// When the server can't be reached.
pub async fn ollama_models(base: &str) -> Result<Vec<String>, String> {
    let answer: Value = reqwest::get(format!("{base}/api/tags"))
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("Ollama isn't answering: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Ollama's answer didn't read: {e}"))?;
    Ok(answer["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["name"].as_str().map(str::to_owned))
        .collect())
}

/// Has the Ollama server at `base` pull `model`, telling how far it got.
/// Dropping the stream stops it.
pub fn ollama_pull(base: &str, model: &str) -> impl Stream<Item = Download> + Send + 'static {
    let request = reqwest::Client::new()
        .post(format!("{base}/api/pull"))
        .json(&json!({ "model": model, "stream": true }));
    let start = futures_util::stream::once(async move {
        request
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| format!("Ollama couldn't pull it: {e}"))
    });
    start.flat_map(|sent| match sent {
        Err(why) => futures_util::stream::iter(vec![Download::Done(Err(why))]).boxed(),
        Ok(response) => pull_lines(response).boxed(),
    })
}

/// Ollama's pull progress, a JSON object a line, as download steps.
fn pull_lines(response: reqwest::Response) -> impl Stream<Item = Download> + Send {
    let state = Some((response, Vec::<u8>::new()));
    futures_util::stream::unfold(state, |state| async move {
        let (mut response, mut pending) = state?;
        loop {
            if let Some(end) = pending.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = pending.drain(..=end).collect();
                let Ok(step) = serde_json::from_slice::<Value>(&line) else {
                    continue;
                };
                if let Some(error) = step["error"].as_str() {
                    return Some((Download::Done(Err(error.to_owned())), None));
                }
                if step["status"] == "success" {
                    return Some((Download::Done(Ok(())), None));
                }
                if let (Some(done), Some(total)) =
                    (step["completed"].as_u64(), step["total"].as_u64())
                {
                    #[expect(clippy::cast_precision_loss, reason = "a fraction for a progress bar")]
                    let fraction = done as f64 / total.max(1) as f64;
                    return Some((Download::Progress(fraction), Some((response, pending))));
                }
                continue;
            }
            match response.chunk().await {
                Ok(Some(piece)) => pending.extend_from_slice(&piece),
                Ok(None) => {
                    return Some((
                        Download::Done(
                            Err("Ollama stopped before the model was whole.".to_owned()),
                        ),
                        None,
                    ));
                }
                Err(e) => {
                    return Some((
                        Download::Done(Err(format!("The pull broke off: {e}"))),
                        None,
                    ));
                }
            }
        }
    })
}

/// llama.cpp's model files, kept in a folder and downloaded from Hugging
/// Face.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFiles {
    /// Where they're kept.
    folder: PathBuf,
    /// Hugging Face's address.
    hub: String,
}

impl ModelFiles {
    /// The user's, in `$XDG_DATA_HOME/harlo/models`. `None` when the system
    /// has no data folder.
    #[must_use]
    pub fn user() -> Option<Self> {
        let data = dirs::data_dir()?;
        Some(Self::new(data.join("harlo").join("models"), HUGGING_FACE))
    }

    /// Files in `folder`, from the Hugging Face at `hub`.
    #[must_use]
    pub fn new(folder: impl Into<PathBuf>, hub: &str) -> Self {
        Self {
            folder: folder.into(),
            hub: hub.trim_end_matches('/').to_owned(),
        }
    }

    /// The folder.
    #[must_use]
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Where `suggestion`'s file is kept once downloaded; `None` until it
    /// is.
    #[must_use]
    pub fn downloaded(&self, suggestion: &Suggestion) -> Option<PathBuf> {
        let folder = self.folder.join(suggestion.repo.replace('/', "--"));
        std::fs::read_dir(folder)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| at_quant(&p.to_string_lossy(), &suggestion.quant))
    }

    /// Finds `suggestion`'s file at its quantization in its repository.
    ///
    /// # Errors
    ///
    /// When Hugging Face can't be reached or the repository has no single
    /// file at that quantization.
    pub async fn find(&self, suggestion: &Suggestion) -> Result<Fetch, String> {
        let url = format!("{}/api/models/{}/tree/main", self.hub, suggestion.repo);
        let files: Value = reqwest::get(&url)
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| format!("Couldn't list {}'s files: {e}", suggestion.title))?
            .json()
            .await
            .map_err(|e| format!("Hugging Face's answer didn't read: {e}"))?;
        let file = files
            .as_array()
            .into_iter()
            .flatten()
            .filter(|f| f["type"] == "file")
            .find(|f| {
                let path = f["path"].as_str().unwrap_or_default();
                // One file: a model split in parts isn't offered.
                at_quant(path, &suggestion.quant)
            })
            .ok_or_else(|| {
                format!(
                    "{} has no single {} file.",
                    suggestion.repo, suggestion.quant
                )
            })?;
        let path = file["path"].as_str().unwrap_or_default();
        let name = path.rsplit('/').next().unwrap_or(path);
        Ok(Fetch {
            url: format!("{}/{}/resolve/main/{path}", self.hub, suggestion.repo),
            path: self
                .folder
                .join(suggestion.repo.replace('/', "--"))
                .join(name),
            size: file["size"].as_u64().unwrap_or(0),
            sha256: file["lfs"]["oid"].as_str().map(str::to_owned),
        })
    }
}

/// Whether `path` is a GGUF file at `quant`: its name ends with it, as
/// `Model-Q6_K.gguf` does (and `Model-UD-Q6_K_XL.gguf` doesn't).
fn at_quant(path: &str, quant: &str) -> bool {
    let path = Path::new(path);
    let gguf = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("gguf"));
    let stem = path.file_stem().map(|s| s.to_string_lossy());
    gguf && stem.is_some_and(|stem| {
        stem.rsplit(['-', '.'])
            .next()
            .is_some_and(|last| last.eq_ignore_ascii_case(quant))
    })
}
