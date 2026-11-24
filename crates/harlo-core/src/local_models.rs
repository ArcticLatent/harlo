//! Models on this computer: the hardware, the engines that serve models
//! (Ollama and llama.cpp), the models that fit, and their downloads.
//!
//! Harlo installs the engines from their GitHub releases into its own data
//! folder, checked against the published SHA-256, and runs their servers
//! while it's open. The server finds Ollama's models on its own; a llama.cpp
//! server is added to the server's config as a provider when asked.

// Rust guideline compliant 2026-10-09

mod engines;
mod models;
mod server;

pub use engines::{Asset, Engines, Install, Installed, Release, newer};
pub use models::{Fit, ModelFiles, Suggestion, ollama_models, ollama_pull, suggest};
pub use server::{Running, Serve, answers, base_url};

use llmfit_core::hardware::{GpuBackend, SystemSpecs};

/// A program that serves models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Engine {
    /// Ollama: pulls models by name and serves all of them.
    Ollama,
    /// llama.cpp's `llama-server`: serves one GGUF file.
    LlamaCpp,
}

impl Engine {
    /// Both, in the order Settings shows them.
    pub const ALL: [Self; 2] = [Self::Ollama, Self::LlamaCpp];

    /// Its name for people.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Ollama => "Ollama",
            Self::LlamaCpp => "llama.cpp",
        }
    }

    /// Its name in folders and config.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::LlamaCpp => "llamacpp",
        }
    }

    /// The port its server listens on, the one the server looks at for
    /// Ollama.
    #[must_use]
    pub fn port(self) -> u16 {
        match self {
            Self::Ollama => 11_434,
            Self::LlamaCpp => 8_080,
        }
    }

    /// The program that serves.
    #[must_use]
    pub fn program(self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::LlamaCpp => "llama-server",
        }
    }

    /// Its GitHub repository.
    #[must_use]
    pub fn repo(self) -> &'static str {
        match self {
            Self::Ollama => "ollama/ollama",
            Self::LlamaCpp => "ggml-org/llama.cpp",
        }
    }
}

/// What runs models on this computer.
#[derive(Debug, Clone)]
pub struct Computer {
    /// The graphics card models run on, if any.
    pub gpu: Option<String>,
    /// Its memory, in GB.
    pub vram_gb: Option<f64>,
    /// The computer's memory, in GB.
    pub ram_gb: f64,
    /// Which builds of the engines suit it.
    pub build: Build,
    /// What llmfit measured, for fitting models.
    pub specs: SystemSpecs,
}

/// The kind of engine build a computer needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Build {
    /// NVIDIA's CUDA; `13` when the driver runs CUDA 13 (580 or newer).
    Cuda {
        /// Whether CUDA 13 builds run.
        cuda13: bool,
    },
    /// AMD's `ROCm`.
    Rocm,
    /// Vulkan, for other graphics cards.
    Vulkan,
    /// The processor alone.
    Cpu,
}

impl Computer {
    /// Looks at this computer's graphics card and memory; takes about a
    /// second, so call it off the UI's thread.
    #[must_use]
    pub fn detect() -> Self {
        Self::from_specs(SystemSpecs::detect(), nvidia_driver_major())
    }

    /// The computer llmfit measured as `specs`, with NVIDIA's driver at
    /// version `driver` (its major number).
    #[must_use]
    pub fn from_specs(specs: SystemSpecs, driver: Option<u32>) -> Self {
        let build = match specs.backend {
            GpuBackend::Cuda => Build::Cuda {
                cuda13: driver.is_some_and(|major| major >= 580),
            },
            GpuBackend::Rocm => Build::Rocm,
            GpuBackend::Vulkan | GpuBackend::Sycl => Build::Vulkan,
            GpuBackend::Metal | GpuBackend::CpuArm | GpuBackend::CpuX86 | GpuBackend::Ascend => {
                Build::Cpu
            }
        };
        Self {
            gpu: specs.gpu_name.clone().filter(|_| specs.has_gpu),
            vram_gb: specs.total_gpu_vram_gb.or(specs.gpu_vram_gb),
            ram_gb: specs.total_ram_gb,
            build,
            specs,
        }
    }

    /// The graphics card and its memory, as Settings shows it.
    #[must_use]
    pub fn gpu_words(&self) -> String {
        match (&self.gpu, self.vram_gb) {
            (Some(gpu), Some(vram)) => format!("{gpu} · {vram:.0} GB"),
            (Some(gpu), None) => gpu.clone(),
            (None, _) => "None for models: they run on the processor".to_owned(),
        }
    }
}

/// The major version of NVIDIA's driver, if it's loaded.
fn nvidia_driver_major() -> Option<u32> {
    let text = std::fs::read_to_string("/proc/driver/nvidia/version").ok()?;
    driver_major(&text)
}

/// The driver's major version in `/proc/driver/nvidia/version`'s `text`.
#[must_use]
pub fn driver_major(text: &str) -> Option<u32> {
    let line = text.lines().find(|l| l.contains("Kernel Module"))?;
    let after = line.split("Kernel Module").nth(1)?;
    let version = after.split_whitespace().find(|w| w.contains('.'))?;
    version.split('.').next()?.parse().ok()
}
