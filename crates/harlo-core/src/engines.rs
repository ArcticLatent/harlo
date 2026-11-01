//! Setting up a model server (Settings → Model servers → Set up a model
//! server): this computer's hardware, the inference engines that suit it,
//! the ones already installed, and installing one with its first model.
//!
//! | engine | installed from | serves with |
//! |---|---|---|
//! | Ollama | its GitHub release (`.tar.zst`, checked against `sha256sum.txt`) | `ollama serve`, models by `ollama pull` |
//! | llama.cpp | its GitHub build release for the GPU (CUDA 12.8 / 13.4, ROCm, Vulkan, CPU; GitHub's SHA-256) | `llama-server -m <gguf>` |
//! | Strata | its own installer from a pinned release (it checks its downloads) | `strata launch … --no-tui` / `--stop` |
//!
//! harlo's own installs go under `<data>/harlo/engines/<engine>/<version>`;
//! Strata's installer keeps to its usual `~/.local/share/strata` and
//! `~/.local/bin/strata`. Nothing runs as root.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::{Value, json};

// ---- this computer ----

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Gpu {
    /// "nvidia", "amd", "intel".
    pub vendor: String,
    pub name: String,
    pub vram_mb: u64,
    /// NVIDIA's compute capability ("12.0"), AMD's gfx target ("gfx1100").
    pub arch: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Hardware {
    pub gpus: Vec<Gpu>,
    pub ram_mb: u64,
    /// Free space where models go (the home folder).
    pub disk_free_mb: u64,
    pub avx2: bool,
    /// NVIDIA's driver version (its major number decides CUDA 12 or 13).
    pub nvidia_driver: String,
}

impl Hardware {
    pub fn vram_mb(&self) -> u64 {
        self.gpus.iter().map(|g| g.vram_mb).max().unwrap_or(0)
    }

    fn vendor(&self) -> &str {
        self.gpus.iter().max_by_key(|g| g.vram_mb).map_or("", |g| g.vendor.as_str())
    }

    /// CUDA 13 builds need driver 580 or newer.
    fn cuda13(&self) -> bool {
        self.nvidia_driver.split('.').next().and_then(|m| m.parse::<u32>().ok()).is_some_and(|m| m >= 580)
    }

    pub fn summary(&self) -> String {
        let gpu = match self.gpus.iter().max_by_key(|g| g.vram_mb) {
            Some(g) => format!("{} · {} GB VRAM", g.name, (g.vram_mb + 512) / 1024),
            None => "No graphics card for models (CPU only)".into(),
        };
        format!("{gpu} · {} GB RAM · {} GB free", (self.ram_mb + 512) / 1024, self.disk_free_mb / 1024)
    }

    pub fn json(&self) -> Value {
        json!({
            "summary": self.summary(),
            "gpus": self.gpus.iter().map(|g| json!({ "vendor": g.vendor, "name": g.name, "vramMb": g.vram_mb, "arch": g.arch })).collect::<Vec<_>>(),
            "ramMb": self.ram_mb, "diskFreeMb": self.disk_free_mb, "avx2": self.avx2,
        })
    }
}

fn run(prog: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(prog).args(args).stdin(std::process::Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `nvidia-smi --query-gpu=name,memory.total,compute_cap,driver_version`.
fn nvidia_gpus(csv: &str) -> (Vec<Gpu>, String) {
    let mut driver = String::new();
    let gpus = csv
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').map(str::trim).collect();
            if f.len() < 4 {
                return None;
            }
            driver = f[3].to_owned();
            Some(Gpu { vendor: "nvidia".into(), name: f[0].to_owned(), vram_mb: f[1].parse().unwrap_or(0), arch: f[2].to_owned() })
        })
        .collect();
    (gpus, driver)
}

/// AMD and Intel cards from sysfs (`/sys/class/drm/card*/device`), with
/// AMD's gfx targets from `rocminfo` when it's there.
fn other_gpus() -> Vec<Gpu> {
    let gfx: Vec<String> = run("rocminfo", &[]).map(|t| t.split_whitespace().filter(|w| w.starts_with("gfx") && w.len() > 4).map(str::to_owned).collect()).unwrap_or_default();
    let mut out = Vec::new();
    let Ok(cards) = std::fs::read_dir("/sys/class/drm") else { return out };
    for card in cards.flatten() {
        let name = card.file_name().to_string_lossy().into_owned();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let dev = card.path().join("device");
        let vendor = std::fs::read_to_string(dev.join("vendor")).unwrap_or_default();
        let vram = std::fs::read_to_string(dev.join("mem_info_vram_total")).ok().and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(0) / (1024 * 1024);
        let product = std::fs::read_to_string(dev.join("product_name")).unwrap_or_default().trim().to_owned();
        match vendor.trim() {
            "0x1002" if vram >= 2048 => out.push(Gpu { vendor: "amd".into(), name: if product.is_empty() { "AMD Radeon".into() } else { product }, vram_mb: vram, arch: gfx.first().cloned().unwrap_or_default() }),
            "0x8086" if vram >= 2048 => out.push(Gpu { vendor: "intel".into(), name: if product.is_empty() { "Intel Arc".into() } else { product }, vram_mb: vram, arch: String::new() }),
            _ => {}
        }
    }
    out
}

/// This computer, as model servers care (blocking: runs `nvidia-smi`).
pub fn detect() -> Hardware {
    let (mut gpus, nvidia_driver) = run("nvidia-smi", &["--query-gpu=name,memory.total,compute_cap,driver_version", "--format=csv,noheader,nounits"]).map(|t| nvidia_gpus(&t)).unwrap_or_default();
    gpus.extend(other_gpus());
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let ram_mb = meminfo.lines().find_map(|l| l.strip_prefix("MemTotal:")).and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok()).unwrap_or(0) / 1024;
    let avx2 = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default().split_whitespace().any(|w| w == "avx2");
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    let disk_free_mb = run("df", &["-Pk", &home]).and_then(|t| t.lines().nth(1)?.split_whitespace().nth(3)?.parse::<u64>().ok()).unwrap_or(0) / 1024;
    Hardware { gpus, ram_mb, disk_free_mb, avx2, nvidia_driver }
}

// ---- the engines ----

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Engine {
    Ollama,
    LlamaCpp,
    Strata,
    Vllm,
    Basalt,
    Ninfer,
}

pub const ENGINES: [Engine; 6] = [Engine::Ollama, Engine::LlamaCpp, Engine::Vllm, Engine::Strata, Engine::Basalt, Engine::Ninfer];

impl Engine {
    pub fn id(self) -> &'static str {
        match self {
            Engine::Ollama => "ollama",
            Engine::LlamaCpp => "llamacpp",
            Engine::Strata => "strata",
            Engine::Vllm => "vllm",
            Engine::Basalt => "basalt",
            Engine::Ninfer => "ninfer",
        }
    }

    pub fn parse(id: &str) -> Option<Engine> {
        ENGINES.into_iter().find(|e| e.id() == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            Engine::Ollama => "Ollama",
            Engine::LlamaCpp => "llama.cpp",
            Engine::Strata => "Strata",
            Engine::Vllm => "vLLM",
            Engine::Basalt => "Basalt",
            Engine::Ninfer => "NInfer",
        }
    }

    pub fn tagline(self) -> &'static str {
        match self {
            Engine::Ollama => "The easiest: pick a model by name and it does the rest.",
            Engine::LlamaCpp => "Any GGUF model from Hugging Face, fast on one computer, every setting yours.",
            Engine::Strata => "Qwen3.8-Flash-Next and DeepSeek-V4.1-Flash on consumer hardware, tuned for your card.",
            Engine::Vllm => "The fastest when several sessions run at once, on an NVIDIA card: Hugging Face models in their own format, full or quantized.",
            Engine::Basalt => "Qwen3.8-Flash-Next on an NVIDIA Blackwell card, its experts in RAM where the card is full. Built on this computer.",
            Engine::Ninfer => "Built for the RTX 5090: Qwen3.6 and Qwen3.8, 27B to Flash-Next, with fast drafts. Built on this computer.",
        }
    }

    pub fn repo(self) -> &'static str {
        match self {
            Engine::Ollama => "ollama/ollama",
            Engine::LlamaCpp => "ggml-org/llama.cpp",
            Engine::Strata => "Niko1221/Strata",
            // (Installed with uv, Python's fast installer: its release.)
            Engine::Vllm => "astral-sh/uv",
            Engine::Basalt => "jesdga95/basalt",
            Engine::Ninfer => "Neroued/ninfer",
        }
    }

    /// Whether it suits this computer: "ok", "warn" or "no", and why.
    pub fn fit(self, hw: &Hardware) -> (&'static str, String) {
        let total_gb = (hw.vram_mb() + hw.ram_mb) / 1024;
        match self {
            Engine::Ollama | Engine::LlamaCpp => match hw.vendor() {
                "" => ("warn", "Runs on the CPU here: fine for small models, slow for big ones.".into()),
                "intel" if self == Engine::Ollama => ("warn", "Ollama doesn't use Intel graphics: it runs on the CPU.".into()),
                _ => ("ok", String::new()),
            },
            Engine::Strata => match hw.vendor() {
                "" => ("no", "Needs an NVIDIA RTX, a supported AMD Radeon or an Intel Arc card.".into()),
                "nvidia" if hw.vram_mb() < 12 * 1024 => ("no", "Needs an NVIDIA card with 12 GB or more.".into()),
                _ if total_gb < 38 => ("warn", format!("Its smallest model needs about 38 GB of RAM and graphics memory together; this computer has {total_gb} GB.")),
                _ => ("ok", String::new()),
            },
            // NVIDIA (compute 7.5 and up), the whole model on the card.
            Engine::Vllm => match hw.vendor() {
                "nvidia" if hw.gpus.iter().filter(|g| g.vendor == "nvidia").all(|g| g.arch.parse::<f32>().is_ok_and(|c| c < 7.5)) => ("no", "Needs an NVIDIA card from the RTX 20 series (Turing) or newer.".into()),
                "nvidia" if hw.vram_mb() < 8 * 1024 => ("warn", "Small models only: vLLM keeps the whole model on the card, and this one has under 8 GB.".into()),
                "nvidia" => ("ok", String::new()),
                "amd" => ("no", "vLLM on AMD cards isn't set up in harlo yet; llama.cpp or Ollama run here.".into()),
                _ => ("no", "Needs an NVIDIA card.".into()),
            },
            // A Blackwell card (compute 12.x), the tools to build it, and room
            // for its smallest model (about 70 GB) in the card and RAM.
            Engine::Basalt => {
                if !hw.gpus.iter().any(|g| g.vendor == "nvidia" && g.arch.starts_with("12.")) {
                    return ("no", "Needs an NVIDIA Blackwell card (RTX 50 series).".into());
                }
                let missing = basalt_tools_missing();
                if !missing.is_empty() {
                    return ("no", format!("harlo builds it here, which needs {} installed first.", missing.join(", ")));
                }
                if total_gb < 78 {
                    return ("no", format!("Its smallest model needs about 70 GB of RAM and graphics memory together; this computer has {total_gb} GB."));
                }
                ("ok", String::new())
            }
            // An RTX 5090 (the one single card it's built and tuned for), and
            // the tools and libraries to build it.
            Engine::Ninfer => {
                if !hw.gpus.iter().any(|g| g.vendor == "nvidia" && g.name.contains("5090")) {
                    return ("no", "Needs an NVIDIA RTX 5090.".into());
                }
                let missing = ninfer_tools_missing();
                if !missing.is_empty() {
                    return ("no", format!("harlo builds it here, which needs {} installed first.", missing.join(", ")));
                }
                ("ok", String::new())
            }
        }
    }
}

/// An engine already on this computer: what runs it, and its version.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Installed {
    pub program: PathBuf,
    pub version: String,
    /// harlo installed it (in its engines folder).
    pub harlo: bool,
}

pub fn engines_dir() -> PathBuf {
    crate::db::default_path().parent().map(|d| d.join("engines")).unwrap_or_else(|| PathBuf::from("engines"))
}

fn on_path(prog: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).chain([PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/bin")]).map(|d| d.join(prog)).find(|p| p.is_file())
}

/// The newest harlo install of an engine (its program inside).
fn harlo_install(engine: Engine) -> Option<PathBuf> {
    let dir = engines_dir().join(engine.id());
    let mut versions: Vec<PathBuf> = std::fs::read_dir(&dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    versions.sort_by_key(|p| version_key(&p.file_name().unwrap_or_default().to_string_lossy()));
    let prog = match engine {
        Engine::Ollama => "ollama",
        Engine::LlamaCpp => "llama-server",
        Engine::Strata => return None,
        // (Its own Python environment: one, updated in place.)
        Engine::Vllm => return Some(vllm_env().join("bin/vllm")).filter(|p| p.is_file()),
        // (Built from its source: <release>/<source>/bin/basalt.)
        Engine::Basalt => {
            return versions.iter().rev().find_map(|v| std::fs::read_dir(v).ok()?.flatten().map(|e| e.path().join("bin/basalt")).find(|p| p.is_file()));
        }
        // (Built from its source: <commit>/<source>/build/…/apps/ninfer-serve.)
        Engine::Ninfer => {
            return versions.iter().rev().find_map(|v| std::fs::read_dir(v).ok()?.flatten().map(|e| e.path().join("build/rtx5090-release/apps/ninfer-serve")).find(|p| p.is_file()));
        }
    };
    versions.iter().rev().find_map(|v| find_file(v, prog, 4))
}

/// A version's numbers, to compare ("v0.2.10" after "v0.2.9", "b12024").
fn version_key(version: &str) -> Vec<u64> {
    version.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse().ok()).collect()
}

/// Whether `latest` is newer than what's installed. (NInfer's versions are
/// commits, `<date>-<sha>`: any other commit is its newest.)
pub fn update_available(engine: Engine, installed: &str, latest: &str) -> bool {
    if engine == Engine::Ninfer {
        return installed.trim() != latest.trim();
    }
    let (now, new) = (version_key(installed), version_key(latest));
    !now.is_empty() && new > now
}

/// An engine's latest version, as `plan` would install it (uv only delivers
/// vLLM: its version is PyPI's).
pub fn latest_version(engine: Engine) -> Result<String, String> {
    match engine {
        Engine::Ninfer => Ok(ninfer_latest()?.0),
        Engine::Vllm => get_json("https://pypi.org/pypi/vllm/json")?["info"]["version"].as_str().map(str::to_owned).ok_or_else(|| "PyPI doesn't say vLLM's version.".to_owned()),
        _ => release(engine)?["tag_name"].as_str().filter(|t| !t.is_empty()).map(str::to_owned).ok_or_else(|| format!("{} has no release.", engine.name())),
    }
}

/// NInfer's latest commit: its version (the date first, so the newest build
/// is last among harlo's) and its full id.
fn ninfer_latest() -> Result<(String, String), String> {
    let commits = get_json(&format!("https://api.github.com/repos/{}/commits?per_page=1", Engine::Ninfer.repo()))?;
    let latest = &commits[0];
    let sha = latest["sha"].as_str().filter(|s| s.len() >= 8).ok_or("NInfer's repository has no commits to build.")?;
    let date = latest["commit"]["committer"]["date"].as_str().unwrap_or_default().split('T').next().unwrap_or_default().replace('-', "");
    Ok((format!("{date}-{}", &sha[..8]), sha.to_owned()))
}

fn find_file(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            subdirs.push(p);
        } else if e.file_name() == name {
            return Some(p);
        }
    }
    if depth == 0 {
        return None;
    }
    subdirs.into_iter().find_map(|d| find_file(&d, name, depth - 1))
}

/// The version of the vLLM a `vllm` program runs: its environment's
/// `site-packages/vllm-<version>.dist-info`.
fn vllm_version(program: &Path) -> Option<String> {
    let lib = program.parent()?.parent()?.join("lib");
    std::fs::read_dir(lib).ok()?.flatten().find_map(|py| {
        std::fs::read_dir(py.path().join("site-packages")).ok()?.flatten().find_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_prefix("vllm-")?.strip_suffix(".dist-info").map(str::to_owned)
        })
    })
}

/// What building Basalt here needs and this computer lacks: the CUDA
/// toolkit (nvcc), CMake, a C++ compiler, Python.
pub fn basalt_tools_missing() -> Vec<&'static str> {
    let cuda = on_path("nvcc").is_some() || ["/opt/cuda/bin/nvcc", "/usr/local/cuda/bin/nvcc"].iter().any(|p| Path::new(p).is_file());
    let mut missing = Vec::new();
    if !cuda {
        missing.push("the CUDA 13 toolkit (nvcc)");
    }
    if on_path("cmake").is_none() {
        missing.push("CMake");
    }
    if on_path("g++").is_none() && on_path("clang++").is_none() {
        missing.push("a C++ compiler (g++ or clang++)");
    }
    if on_path("python3").is_none() {
        missing.push("Python 3");
    }
    missing
}

/// What building NInfer here needs and this computer lacks: the CUDA toolkit,
/// CMake, Ninja, a C++ compiler, and the libraries it links.
pub fn ninfer_tools_missing() -> Vec<String> {
    let mut missing: Vec<String> = basalt_tools_missing().into_iter().filter(|t| *t != "Python 3").map(str::to_owned).collect();
    if on_path("ninja").is_none() {
        missing.push("Ninja".to_owned());
    }
    if on_path("pkg-config").is_none() {
        missing.push("pkg-config".to_owned());
        return missing;
    }
    let libraries = [("libjpeg", "libjpeg"), ("libavformat", "FFmpeg"), ("hwloc", "hwloc"), ("liburing", "liburing"), ("libcurl", "curl")];
    for (library, package) in libraries {
        if !Command::new("pkg-config").args(["--exists", library]).status().is_ok_and(|s| s.success()) {
            missing.push(format!("{package} (with its development files)"));
        }
    }
    missing
}

/// vLLM's Python environment (harlo's): `bin/vllm` in it.
pub fn vllm_env() -> PathBuf {
    engines_dir().join("vllm").join("env")
}

/// Where an engine is installed on this computer, if it is.
pub fn installed(engine: Engine) -> Option<Installed> {
    let (harlo, program) = match harlo_install(engine) {
        Some(p) => (true, p),
        None => (false, on_path(match engine {
            Engine::Ollama => "ollama",
            Engine::LlamaCpp => "llama-server",
            Engine::Strata => "strata",
            Engine::Vllm => "vllm",
            Engine::Basalt => "basalt",
            Engine::Ninfer => "ninfer-serve",
        })?),
    };
    // (vLLM: its package's version, not `vllm --version`: that starts Python
    // with PyTorch, seconds every time the engines are listed.)
    if engine == Engine::Vllm {
        let version = vllm_version(&program).unwrap_or_default();
        return Some(Installed { program, version, harlo });
    }
    // (harlo's: the release it installed, its folder's name.)
    if harlo
        && let Some(tag) = program.strip_prefix(engines_dir().join(engine.id())).ok().and_then(|rest| rest.components().next()).map(|c| c.as_os_str().to_string_lossy().trim_start_matches('v').to_owned())
    {
        return Some(Installed { program, version: tag, harlo });
    }
    let out = Command::new(&program).arg("--version").stdin(std::process::Stdio::null()).output().ok();
    let text = out.map(|o| format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))).unwrap_or_default();
    let version = text.split_whitespace().find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit() || c == 'b' || c == 'v') && w.chars().any(|c| c.is_ascii_digit()) && w.len() < 24).unwrap_or("").trim_start_matches('v').to_owned();
    Some(Installed { program, version, harlo })
}

// ---- downloads ----

/// A release file to install: where, its SHA-256 (hex), and its size.
#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

fn http(timeout: Duration) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    ureq::Agent::config_builder().timeout_global(Some(timeout)).http_status_as_error(false).tls_config(tls).build().into()
}

fn get_json(url: &str) -> Result<Value, String> {
    get_json_as(url, "application/vnd.github+json")
}

/// (npm refuses GitHub's kind of `Accept`.)
pub(crate) fn get_json_as(url: &str, accept: &str) -> Result<Value, String> {
    let mut resp = http(Duration::from_secs(30)).get(url).header("User-Agent", "harlo").header("Accept", accept).call().map_err(|e| format!("{url}: {e}"))?;
    if resp.status().as_u16() != 200 {
        return Err(format!("{url}: HTTP {}", resp.status().as_u16()));
    }
    let text = resp.body_mut().with_config().limit(64 * 1024 * 1024).read_to_string().map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn get_text(url: &str) -> Result<String, String> {
    let mut resp = http(Duration::from_secs(30)).get(url).header("User-Agent", "harlo").call().map_err(|e| format!("{url}: {e}"))?;
    if resp.status().as_u16() != 200 {
        return Err(format!("{url}: HTTP {}", resp.status().as_u16()));
    }
    resp.body_mut().read_to_string().map_err(|e| e.to_string())
}

/// The release a version comes from: the newest (llama.cpp's builds are
/// releases of their own; its "latest" is a tag without files).
fn release(engine: Engine) -> Result<Value, String> {
    let repo = engine.repo();
    match engine {
        Engine::LlamaCpp => get_json(&format!("https://api.github.com/repos/{repo}/releases?per_page=5"))?
            .as_array()
            .and_then(|a| a.iter().find(|r| r["assets"].as_array().is_some_and(|x| x.len() > 5)).cloned())
            .ok_or_else(|| "llama.cpp has no build release right now.".into()),
        _ => get_json(&format!("https://api.github.com/repos/{repo}/releases/latest")),
    }
}

fn asset_named(release: &Value, name: &str, sums: Option<&str>) -> Option<Asset> {
    let a = release["assets"].as_array()?.iter().find(|a| a["name"] == name)?;
    // GitHub's digest, else the release's checksum file.
    let sha = a["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_owned).or_else(|| {
        sums?.lines().find_map(|l| {
            let mut f = l.split_whitespace();
            let (hash, file) = (f.next()?, f.next()?);
            (file.trim_start_matches('*').trim_start_matches("./") == name).then(|| hash.to_owned())
        })
    })?;
    Some(Asset { name: name.to_owned(), url: a["browser_download_url"].as_str()?.to_owned(), sha256: sha.to_lowercase(), size: a["size"].as_u64().unwrap_or(0) })
}

/// The llama.cpp build for this computer's card (and its CUDA runtime).
pub fn llamacpp_build(hw: &Hardware, tag: &str) -> (String, Option<String>) {
    match hw.vendor() {
        "nvidia" => {
            let cuda = if hw.cuda13() { "13.4" } else { "12.8" };
            (format!("llama-{tag}-bin-ubuntu-cuda-{cuda}-x64.tar.gz"), Some(format!("cudart-llama-{tag}-bin-ubuntu-cuda-{cuda}-x64.tar.gz")))
        }
        // Vulkan works with the usual Mesa drivers (ROCm builds want ROCm).
        "amd" | "intel" => (format!("llama-{tag}-bin-ubuntu-vulkan-x64.tar.gz"), None),
        _ => (format!("llama-{tag}-bin-ubuntu-x64.tar.gz"), None),
    }
}

/// What installing an engine downloads (its release's files for here), and
/// the version.
pub fn plan(engine: Engine, hw: &Hardware) -> Result<(String, Vec<Asset>), String> {
    // NInfer has no releases: its source at its latest commit, built here.
    if engine == Engine::Ninfer {
        let (tag, sha) = ninfer_latest()?;
        return Ok((tag, vec![Asset { name: format!("ninfer-{}.tar.gz", &sha[..8]), url: format!("https://codeload.github.com/{}/tar.gz/{sha}", engine.repo()), sha256: String::new(), size: 0 }]));
    }
    let rel = release(engine)?;
    let tag = rel["tag_name"].as_str().unwrap_or_default().to_owned();
    let assets = match engine {
        Engine::Ollama => {
            let sums = rel["assets"].as_array().into_iter().flatten().find(|a| a["name"] == "sha256sum.txt").and_then(|a| a["browser_download_url"].as_str()).map(get_text).transpose()?;
            let name = if hw.vendor() == "amd" { "ollama-linux-amd64-rocm.tar.zst" } else { "ollama-linux-amd64.tar.zst" };
            vec![asset_named(&rel, name, sums.as_deref()).ok_or_else(|| format!("Ollama {tag} has no {name} with a checksum."))?]
        }
        Engine::LlamaCpp => {
            let (main, runtime) = llamacpp_build(hw, &tag);
            let mut list = vec![asset_named(&rel, &main, None).ok_or_else(|| format!("llama.cpp {tag} has no {main} with a checksum."))?];
            if let Some(rt) = runtime {
                list.push(asset_named(&rel, &rt, None).ok_or_else(|| format!("llama.cpp {tag} has no {rt} with a checksum."))?);
            }
            list
        }
        Engine::Strata => {
            let sums = rel["assets"].as_array().into_iter().flatten().find(|a| a["name"] == "SHA256SUMS.txt").and_then(|a| a["browser_download_url"].as_str()).map(get_text).transpose()?;
            vec![asset_named(&rel, "install.sh", sums.as_deref()).ok_or_else(|| format!("Strata {tag} has no install.sh with a checksum."))?]
        }
        // Its source at the release (GitHub publishes no checksum for it: it
        // comes over HTTPS from GitHub), built here.
        Engine::Basalt => vec![Asset {
            name: format!("basalt-{tag}.tar.gz"),
            url: format!("https://codeload.github.com/{}/tar.gz/refs/tags/{tag}", engine.repo()),
            sha256: String::new(),
            size: 0,
        }],
        // (Above: no release.)
        Engine::Ninfer => Vec::new(),
        // uv, which then installs vLLM (and its Python) from PyPI.
        Engine::Vllm => {
            let name = "uv-x86_64-unknown-linux-gnu.tar.gz";
            vec![asset_named(&rel, name, None).ok_or_else(|| format!("uv {tag} has no {name} with a checksum."))?]
        }
    };
    Ok((tag, assets))
}

/// Downloads to `dest` (resuming a part already there), checking the
/// SHA-256 when given. `progress(done, total)`; `stop()` cancels.
/// Downloads `url` to `dest` (resuming a `.part` left before), checked
/// against `sha256` when given. `progress` gets what it's doing
/// ("download", then "check"), bytes done and the total.
pub fn download(url: &str, dest: &Path, sha256: Option<&str>, progress: &mut dyn FnMut(&str, u64, u64), stop: &dyn Fn() -> bool) -> Result<(), String> {
    let part = dest.with_extension("part");
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    let mut req = http(Duration::from_secs(24 * 3600)).get(url).header("User-Agent", "harlo");
    if have > 0 {
        req = req.header("Range", &format!("bytes={have}-"));
    }
    let mut resp = req.call().map_err(|e| format!("Couldn't download {url}: {e}"))?;
    let status = resp.status().as_u16();
    let resumed = status == 206;
    if status != 200 && !resumed {
        return Err(format!("Couldn't download {url}: HTTP {status}"));
    }
    let left: u64 = resp.headers().get("content-length").and_then(|v| v.to_str().ok()?.parse().ok()).unwrap_or(0);
    let start = if resumed { have } else { 0 };
    let total = start + left;
    let mut file = std::fs::OpenOptions::new().create(true).write(true).append(resumed).truncate(!resumed).open(&part).map_err(|e| e.to_string())?;
    let mut reader = resp.body_mut().with_config().limit(u64::MAX).reader();
    let mut buf = vec![0u8; 1 << 20];
    let mut done = start;
    loop {
        if stop() {
            return Err("Cancelled.".into());
        }
        let n = reader.read(&mut buf).map_err(|e| format!("The download broke off: {e} (it resumes when you try again)"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        progress("download", done, total);
    }
    drop(file);
    if let Some(want) = sha256 {
        check(&part, dest, want, progress, stop)?;
    }
    std::fs::rename(&part, dest).map_err(|e| e.to_string())
}

/// A downloaded `part` against its SHA-256 (gone if it doesn't match).
fn check(part: &Path, dest: &Path, want: &str, progress: &mut dyn FnMut(&str, u64, u64), stop: &dyn Fn() -> bool) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    let mut f = std::fs::File::open(part).map_err(|e| e.to_string())?;
    let size = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut buf = vec![0u8; 1 << 20];
    let mut read = 0;
    loop {
        if stop() {
            return Err("Cancelled.".into());
        }
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        read += n as u64;
        progress("check", read, size);
    }
    let got = format!("{:x}", h.finalize());
    if !got.eq_ignore_ascii_case(want) {
        let _ = std::fs::remove_file(part);
        return Err(format!("{} didn't match its published checksum, so it wasn't used.", dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
    }
    Ok(())
}

/// Where a file's bytes are (the address Hugging Face sends you to, or the
/// URL itself), its size and the SHA-256 it publishes (`X-Linked-Etag`),
/// without downloading it.
fn locate(url: &str) -> Result<(String, u64, Option<String>), String> {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).http_status_as_error(false).max_redirects(0).tls_config(tls).build().into();
    let resp = agent.head(url).header("User-Agent", "harlo").call().map_err(|e| format!("Couldn't reach {url}: {e}"))?;
    let header = |k: &str| resp.headers().get(k).and_then(|v| v.to_str().ok()).map(|v| v.trim().to_owned());
    let status = resp.status().as_u16();
    let at = match status {
        301 | 302 | 303 | 307 | 308 => header("location").ok_or("No address to download it from.")?,
        200 => url.to_owned(),
        _ => return Err(format!("Couldn't download {url}: HTTP {status}")),
    };
    let size = header("x-linked-size").or_else(|| header("content-length")).and_then(|s| s.parse().ok()).unwrap_or(0);
    let sha = header("x-linked-etag").map(|e| e.trim_matches('"').to_lowercase()).filter(|e| e.len() == 64 && e.chars().all(|c| c.is_ascii_hexdigit()));
    Ok((at, size, sha))
}

/// Downloads a large file in parts at once (`parts` connections, each to
/// its own place in the file), as fast as a single one isn't: a model from
/// Hugging Face. Each part resumes where it was; the whole is checked
/// against `sha256` (or the SHA-256 the server publishes). A server that
/// doesn't give parts: one connection ([`download`]).
pub fn download_parallel(url: &str, dest: &Path, sha256: Option<&str>, parts: usize, progress: &mut dyn FnMut(&str, u64, u64), stop: &dyn Fn() -> bool) -> Result<(), String> {
    use std::os::unix::fs::FileExt;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    let Ok((at, size, published)) = locate(url) else { return download(url, dest, sha256, progress, stop) };
    let want = sha256.map(str::to_owned).or(published);
    // (Parts only from a server that gives them: one byte, as a test.)
    let ranged = size > 0
        && http(Duration::from_secs(30)).get(&at).header("User-Agent", "harlo").header("Range", "bytes=0-0").call().is_ok_and(|r| r.status().as_u16() == 206);
    if !ranged {
        return download(url, dest, want.as_deref(), progress, stop);
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    // (Small files: fewer parts; none smaller than 32 MB.)
    let n = (size / (32 << 20)).clamp(1, parts.max(1) as u64) as usize;
    let span = size.div_ceil(n as u64);
    let part = dest.with_extension("part");
    let state = dest.with_extension("parts");
    // Where each part had got to (the same file and parts only).
    let saved: Option<Vec<u64>> = std::fs::read_to_string(&state)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(|v| v["size"] == size && v["done"].as_array().is_some_and(|d| d.len() == n))
        .filter(|_| std::fs::metadata(&part).is_ok_and(|m| m.len() == size))
        .map(|v| v["done"].as_array().unwrap().iter().map(|d| d.as_u64().unwrap_or(0)).collect());
    let file = std::fs::OpenOptions::new().create(true).write(true).truncate(saved.is_none()).open(&part).map_err(|e| e.to_string())?;
    file.set_len(size).map_err(|e| e.to_string())?;
    let done: Arc<Vec<AtomicU64>> = Arc::new((0..n).map(|i| AtomicU64::new(saved.as_ref().map_or(0, |d| d[i]))).collect());
    let save = |done: &[AtomicU64]| {
        let list: Vec<u64> = done.iter().map(|d| d.load(Ordering::Relaxed)).collect();
        let _ = std::fs::write(&state, json!({ "size": size, "done": list }).to_string());
    };
    let cancel = AtomicBool::new(false);
    let finished = AtomicUsize::new(0);
    let failed: Mutex<Option<String>> = Mutex::new(None);
    // (The address it sends you to expires: asked again if it does.)
    let address = Mutex::new(at);
    std::thread::scope(|scope| {
        for i in 0..n {
            let (done, file, cancel, finished, failed, address) = (&done, &file, &cancel, &finished, &failed, &address);
            scope.spawn(move || {
                let (start, end) = (i as u64 * span, ((i as u64 + 1) * span).min(size));
                let mut tries = 0;
                let mut buf = vec![0u8; 1 << 20];
                'part: loop {
                    let from = start + done[i].load(Ordering::Relaxed);
                    if from >= end || cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let at = address.lock().unwrap().clone();
                    let resp = http(Duration::from_secs(24 * 3600)).get(&at).header("User-Agent", "harlo").header("Range", &format!("bytes={from}-{}", end - 1)).call();
                    let mut resp = match resp {
                        Ok(r) if r.status().as_u16() == 206 => r,
                        other => {
                            tries += 1;
                            if other.as_ref().is_ok_and(|r| matches!(r.status().as_u16(), 403 | 410))
                                && let Ok((fresh, _, _)) = locate(url)
                            {
                                *address.lock().unwrap() = fresh;
                            }
                            if tries > 4 {
                                let why = match other {
                                    Ok(r) => format!("HTTP {}", r.status().as_u16()),
                                    Err(e) => e.to_string(),
                                };
                                *failed.lock().unwrap() = Some(format!("The download broke off: {why} (it resumes when you try again)"));
                                cancel.store(true, Ordering::Relaxed);
                                break;
                            }
                            std::thread::sleep(Duration::from_secs(tries));
                            continue;
                        }
                    };
                    let mut reader = resp.body_mut().with_config().limit(u64::MAX).reader();
                    let mut at_byte = from;
                    loop {
                        if cancel.load(Ordering::Relaxed) {
                            break 'part;
                        }
                        match reader.read(&mut buf) {
                            Ok(0) => break,
                            Ok(got) => {
                                let got = got.min((end - at_byte) as usize);
                                if file.write_all_at(&buf[..got], at_byte).is_err() {
                                    *failed.lock().unwrap() = Some("Couldn't write the download (is the disk full?)".into());
                                    cancel.store(true, Ordering::Relaxed);
                                    break 'part;
                                }
                                at_byte += got as u64;
                                done[i].fetch_add(got as u64, Ordering::Relaxed);
                                tries = 0;
                            }
                            // (Broke off: this part again from where it got.)
                            Err(_) => {
                                tries += 1;
                                break;
                            }
                        }
                    }
                }
                finished.fetch_add(1, Ordering::Relaxed);
            });
        }
        // How far it is, now and then; the parts saved as it goes.
        let mut last_save = std::time::Instant::now();
        while finished.load(Ordering::Relaxed) < n {
            if stop() {
                cancel.store(true, Ordering::Relaxed);
            }
            progress("download", done.iter().map(|d| d.load(Ordering::Relaxed)).sum(), size);
            if last_save.elapsed() > Duration::from_secs(2) {
                save(&done);
                last_save = std::time::Instant::now();
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });
    save(&done);
    drop(file);
    if let Some(e) = failed.into_inner().unwrap() {
        return Err(e);
    }
    if stop() {
        return Err("Cancelled.".into());
    }
    progress("download", size, size);
    if let Some(want) = &want {
        check(&part, dest, want, progress, stop).inspect_err(|_| {
            let _ = std::fs::remove_file(&state);
        })?;
    }
    std::fs::rename(&part, dest).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&state);
    Ok(())
}

/// Unpacks a `.tar.gz` or `.tar.zst` into `dir` (with the system's tar).
pub fn unpack(archive: &Path, dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let name = archive.to_string_lossy();
    let mut cmd = Command::new("tar");
    if name.ends_with(".zst") {
        if on_path("zstd").is_none() {
            return Err("Unpacking it needs zstd: install the zstd package and try again.".into());
        }
        cmd.args(["--use-compress-program=zstd -d"]);
        cmd.arg("-xf");
    } else {
        cmd.arg("-xzf");
    }
    let out = cmd.arg(archive).arg("-C").arg(dir).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(format!("Couldn't unpack it: {}", String::from_utf8_lossy(&out.stderr).trim())) }
}

// ---- models ----

/// Strata's Qwen3.8-Flash-Next sizes: `--model`, what it's like, and the
/// RAM + VRAM it needs (GB).
pub const STRATA_MODELS: [(&str, &str, u64); 5] = [
    ("q2_0", "Fastest, good quality", 38),
    ("iq2_xs", "Fast, better quality", 40),
    ("iq3_xxs", "Slower, great quality", 47),
    ("iq3_s", "Slowest, best quality", 55),
    ("coder", "Tuned for code", 30),
];

/// Strata's DeepSeek-V4.1-Flash sets: `--model`, the name to show, what it's
/// like, the RAM + VRAM it needs (GB: its own least RAM and a 12 GB card; the
/// rest is read from the SSD) and its download (GB, on disk).
pub const STRATA_DEEPSEEK: [(&str, &str, &str, u64, u64); 6] = [
    ("ist-das-1.75bpw", "1.75 BPW", "Smallest: fine for code, weaker on prose", 46, 239),
    ("ist-das-2.00bpw", "2.00 BPW", "Good for code and agent work", 52, 257),
    ("ist-das-2.25bpw", "2.25 BPW", "Closest to the original", 56, 274),
    ("ud-iq1_s", "UD-IQ1_S", "Unsloth's smallest: weaker on long work", 48, 241),
    ("ud-iq1_m", "UD-IQ1_M", "Unsloth's next: partly 1-bit", 52, 258),
    ("ud-q8_k_xl", "UD-Q8_K_XL", "For several cards and hundreds of GB of RAM", 318, 508),
];

/// Whether Strata's DeepSeek engine hit a graphics-card fault in its latest
/// run (its log, runs appended). Its process lives on and its server answers,
/// but every request fails until it's restarted.
pub fn strata_engine_faulted(log: &str) -> bool {
    let run = log.rsplit("engine started:").next().unwrap_or(log);
    run.lines().any(|l| l.starts_with("ds4: cuda") || (l.starts_with("ds4:") && l.contains("illegal memory access")))
}

/// The `--family` of one of Strata's models above.
pub fn strata_family(model: &str) -> &'static str {
    if STRATA_DEEPSEEK.iter().any(|m| m.0 == model) { "deepseek41" } else { "qwen" }
}

/// A model on Hugging Face with GGUF files (or, for vLLM, in its own
/// format: `bytes` its weights, `model_type` its family from config.json).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HfModel {
    pub repo: String,
    pub params: u64,
    pub context: u64,
    pub downloads: u64,
    pub updated: String,
    pub bytes: u64,
    pub model_type: String,
}

/// NInfer's official models for one card, most downloaded first, with what
/// they download (GLM-5.3-Flash needs two DGX Sparks).
pub fn hf_ninfer_models(search: &str) -> Result<Vec<HfModel>, String> {
    let list = get_json("https://huggingface.co/api/models?author=neroued&search=NInfer&sort=downloads&direction=-1&limit=30")?;
    let search = search.to_lowercase();
    let repos: Vec<(String, u64)> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| Some((m["id"].as_str()?.to_owned(), m["downloads"].as_u64().unwrap_or(0))))
        .filter(|(repo, _)| repo.ends_with("-NInfer") && !repo.contains("GLM") && repo.to_lowercase().contains(&search))
        .collect();
    Ok(std::thread::scope(|s| {
        let sizes: Vec<_> = repos.iter().map(|(repo, _)| s.spawn(move || hf_ninfer_files(repo).map(|f| f.iter().map(|(_, size)| size).sum::<u64>()).unwrap_or(0))).collect();
        repos.iter().zip(sizes).map(|((repo, downloads), size)| HfModel { repo: repo.clone(), downloads: *downloads, bytes: size.join().unwrap_or(0), ..Default::default() }).collect()
    }))
}

/// The files of a NInfer model: the entry file first, then its parts (kept
/// beside it; the engine is given only the entry).
pub fn hf_ninfer_files(repo: &str) -> Result<Vec<(String, u64)>, String> {
    let repo = repo.trim().trim_matches('/');
    let tree = get_json(&format!("https://huggingface.co/api/models/{repo}/tree/main"))?;
    let mut files: Vec<(String, u64)> = tree
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| {
            let path = f["path"].as_str()?;
            (path.ends_with(".ninfer") || path.contains(".ninfer.part-")).then(|| (path.to_owned(), f["lfs"]["size"].as_u64().or(f["size"].as_u64()).unwrap_or(0)))
        })
        .collect();
    files.sort_by_key(|(path, _)| !path.ends_with(".ninfer"));
    if files.first().is_none_or(|(path, _)| !path.ends_with(".ninfer")) {
        return Err(format!("{repo} has no .ninfer file."));
    }
    Ok(files)
}

/// What a NInfer model is called ("neroued/Qwen3.8-27B-nvfp4-NInfer" →
/// "Qwen3.8-27B-NVFP4").
pub fn ninfer_name(repo: &str) -> String {
    repo.rsplit('/').next().unwrap_or(repo).trim_end_matches("-NInfer").replace("nvfp4", "NVFP4")
}

/// The most downloaded GGUF chat models on Hugging Face (Unsloth's, a
/// reliable set of sizes), or those matching `search` (anyone's).
pub fn hf_models(search: &str) -> Result<Vec<HfModel>, String> {
    let q = search.trim();
    let mut url = "https://huggingface.co/api/models?filter=gguf&pipeline_tag=text-generation&sort=downloads&direction=-1&limit=40&expand[]=downloads&expand[]=gguf&expand[]=lastModified".to_owned();
    if q.is_empty() {
        url.push_str("&author=unsloth");
    } else {
        url.push_str(&format!("&search={}", q.split_whitespace().collect::<Vec<_>>().join("%20").replace('&', "%26").replace('#', "%23")));
    }
    let list = get_json(&url)?;
    Ok(list
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some(HfModel {
                repo: m["id"].as_str()?.to_owned(),
                params: m["gguf"]["total"].as_u64().unwrap_or(0),
                context: m["gguf"]["context_length"].as_u64().unwrap_or(0),
                downloads: m["downloads"].as_u64().unwrap_or(0),
                updated: m["lastModified"].as_str().unwrap_or_default().chars().take(10).collect(),
                ..Default::default()
            })
        })
        .collect())
}

/// Models for vLLM: repositories in Hugging Face's own format (safetensors),
/// full or quantized (AWQ, GPTQ, FP8), not gated (no account needed). The
/// most downloaded of the families vLLM serves well, or those matching
/// `search`. Their weights' size from their parameters by type.
pub fn hf_vllm_models(search: &str) -> Result<Vec<HfModel>, String> {
    let base = "https://huggingface.co/api/models?pipeline_tag=text-generation&library=safetensors&sort=downloads&direction=-1&expand[]=downloads&expand[]=safetensors&expand[]=gated&expand[]=config&expand[]=lastModified";
    let q = search.trim();
    let urls: Vec<String> = if q.is_empty() {
        ["Qwen", "openai", "mistralai", "zai-org", "ibm-granite"].iter().map(|a| format!("{base}&limit=15&author={a}")).collect()
    } else {
        vec![format!("{base}&limit=40&search={}", q.split_whitespace().collect::<Vec<_>>().join("%20").replace('&', "%26").replace('#', "%23"))]
    };
    let mut out: Vec<HfModel> = Vec::new();
    for url in urls {
        for m in get_json(&url)?.as_array().into_iter().flatten() {
            let gated = m["gated"].as_bool() != Some(false);
            let types = m["safetensors"]["parameters"].as_object();
            let model_type = m["config"]["model_type"].as_str().unwrap_or_default();
            let (Some(id), Some(types), false) = (m["id"].as_str(), types, gated || model_type.is_empty()) else { continue };
            // Each type's size: FP8 one byte, BF16 two, packed 4-bit in I32s…
            // (MXFP4, gpt-oss's: two of its "U8" parameters to a byte.)
            let mxfp4 = m["config"]["quantization_config"]["quant_method"] == "mxfp4";
            let bytes: u64 = types
                .iter()
                .map(|(t, n)| {
                    let n = n.as_u64().unwrap_or(0);
                    match t.as_str() {
                        "F64" | "I64" | "U64" => n * 8,
                        "F32" | "I32" | "U32" => n * 4,
                        "U8" if mxfp4 => n / 2,
                        t if t.starts_with("F8") || t == "I8" || t == "U8" || t == "BOOL" => n,
                        _ => n * 2,
                    }
                })
                .sum();
            out.push(HfModel {
                repo: id.to_owned(),
                params: m["safetensors"]["total"].as_u64().unwrap_or(0),
                downloads: m["downloads"].as_u64().unwrap_or(0),
                updated: m["lastModified"].as_str().unwrap_or_default().chars().take(10).collect(),
                bytes,
                model_type: model_type.to_owned(),
                ..Default::default()
            });
        }
    }
    out.sort_by_key(|m| std::cmp::Reverse(m.downloads));
    Ok(out)
}

/// The files vLLM needs from a repository: its weights (safetensors),
/// config and tokenizer, not other formats or extras. `(path, size)`.
pub fn hf_vllm_files(repo: &str) -> Result<Vec<(String, u64)>, String> {
    let repo = repo.trim().trim_matches('/');
    let tree = get_json(&format!("https://huggingface.co/api/models/{repo}/tree/main?recursive=true"))?;
    let wanted = |p: &str| {
        let name = p.rsplit('/').next().unwrap_or(p);
        let skip_dir = ["original/", "metal/", "onnx/", "gguf/", "openvino/"].iter().any(|d| p.starts_with(d));
        !skip_dir && (name.ends_with(".safetensors") || name.ends_with(".json") || name.ends_with(".jinja") || ["tokenizer.model", "merges.txt", "vocab.txt"].contains(&name) || name.ends_with(".tiktoken"))
    };
    let files: Vec<(String, u64)> = tree
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| f["type"] == "file")
        .filter_map(|f| {
            let path = f["path"].as_str()?;
            wanted(path).then(|| (path.to_owned(), f["lfs"]["size"].as_u64().or(f["size"].as_u64()).unwrap_or(0)))
        })
        .collect();
    if !files.iter().any(|(p, _)| p.ends_with(".safetensors")) {
        return Err(format!("{repo} has no safetensors weights for vLLM."));
    }
    Ok(files)
}

/// vLLM's parsers for a model family (config.json's `model_type`, and the
/// repository's name): how its tool calls are read (agents need them), and
/// its thinking (None: it doesn't think, or vLLM reads it on its own).
pub fn vllm_parsers(model_type: &str, repo: &str) -> (&'static str, Option<&'static str>) {
    let name = repo.to_lowercase();
    match model_type {
        "qwen3" | "qwen3_moe" | "qwen3_next" if name.contains("coder") => ("qwen3_coder", None),
        "qwen3" | "qwen3_moe" | "qwen3_next" => ("hermes", Some("qwen3")),
        "qwen2" | "qwen2_moe" => ("hermes", None),
        "gpt_oss" => ("openai", Some("openai_gptoss")),
        "llama" => ("llama3_json", None),
        "llama4" => ("llama4_pythonic", None),
        "mistral" | "mistral3" | "ministral" => ("mistral", None),
        "glm4_moe" if name.contains("4.7") => ("glm47", Some("glm47")),
        "glm4_moe" => ("glm45", Some("glm45")),
        "deepseek_v3" if name.contains("r1") => ("deepseek_v3", Some("deepseek_r1")),
        "deepseek_v3" => ("deepseek_v31", None),
        "granite" | "granitemoe" | "granitemoehybrid" => ("granite4", None),
        _ => ("hermes", None),
    }
}

/// A GGUF file's quantization ("Qwen3-4B-UD-Q4_K_XL.gguf" → "UD-Q4_K_XL"),
/// as Ollama names it in `hf.co/<repo>:<quant>`.
pub fn quant_of(file: &str) -> String {
    // (Basalt's packs: "Qwen3.8-FN-GSQ-RCO-IQ3_S.basalt" → "GSQ-RCO-IQ3_S".)
    if let Some(stem) = file.rsplit('/').next().unwrap_or(file).strip_suffix(".basalt") {
        return stem.split_once("-FN-").map_or(stem, |(_, q)| q).trim_end_matches("_v2").to_owned();
    }
    let name = file.rsplit('/').next().unwrap_or(file).trim_end_matches(".gguf");
    let name = name.split("-0000").next().unwrap_or(name);
    let parts: Vec<&str> = name.split('-').collect();
    let is_quant = |p: &str| {
        let u = p.to_uppercase();
        (u.starts_with('Q') || u.starts_with("IQ") || u.starts_with("TQ")) && u.chars().nth(if u.starts_with('Q') { 1 } else { 2 }).is_some_and(|c| c.is_ascii_digit())
            || ["F16", "BF16", "F32"].contains(&u.as_str())
            || u.starts_with("MXFP")
    };
    match parts.iter().position(|p| p.eq_ignore_ascii_case("UD") || is_quant(p)) {
        Some(i) => parts[i..].join("-"),
        None => name.to_owned(),
    }
}

/// Where a model of `bytes` runs here: "gpu" (in graphics memory, with
/// room for its context), "ram" (partly in RAM: slower) or "no".
pub fn fit(bytes: u64, vram_mb: u64, ram_mb: u64) -> &'static str {
    let mb = bytes / (1024 * 1024);
    if mb <= vram_mb * 9 / 10 {
        "gpu"
    } else if mb <= (vram_mb + ram_mb) * 7 / 10 {
        "ram"
    } else {
        "no"
    }
}

/// A model's size at Q4_K_M (about 4.85 bits a parameter).
pub fn q4_bytes(params: u64) -> u64 {
    params * 61 / 100
}

/// The file to start with: Q4 (close to the full model, and fast) where it
/// fits in graphics memory, else the largest that does; then the same
/// partly in RAM; else the smallest.
pub fn pick_file(files: &[(String, u64)], vram_mb: u64, ram_mb: u64) -> Option<String> {
    let best = |want: &str| {
        let fitting: Vec<&(String, u64)> = files.iter().filter(|(_, s)| fit(*s, vram_mb, ram_mb) == want).collect();
        ["UD-Q4_K_XL", "Q4_K_M"]
            .iter()
            .find_map(|q| fitting.iter().find(|(f, _)| quant_of(f).eq_ignore_ascii_case(q)))
            .or_else(|| fitting.iter().max_by_key(|(_, s)| *s))
            .map(|(f, _)| f.clone())
    };
    best("gpu").or_else(|| best("ram")).or_else(|| files.iter().min_by_key(|(_, s)| *s).map(|(f, _)| f.clone()))
}

/// A Hugging Face repository's GGUF files: `(file, size in bytes)`, a split
/// model's parts as one entry (its first part, their total size).
pub fn hf_ggufs(repo: &str) -> Result<Vec<(String, u64)>, String> {
    let repo = repo.trim().trim_start_matches("https://huggingface.co/").trim_matches('/');
    if repo.split('/').count() != 2 {
        return Err(format!("{repo} isn't a Hugging Face repository (owner/name)."));
    }
    let tree = get_json(&format!("https://huggingface.co/api/models/{repo}/tree/main?recursive=true"))?;
    let mut files: Vec<(String, u64)> = Vec::new();
    for f in tree.as_array().into_iter().flatten() {
        let path = f["path"].as_str().unwrap_or_default();
        if !(path.ends_with(".gguf") || path.ends_with(".basalt")) || path.contains("mmproj") {
            continue;
        }
        let size = f["lfs"]["size"].as_u64().or(f["size"].as_u64()).unwrap_or(0);
        // "x-00002-of-00003.gguf" joins "x-00001-of-00003.gguf".
        match split_first(path) {
            Some(first) => match files.iter_mut().find(|(p, _)| *p == first) {
                Some((_, s)) => *s += size,
                None => files.push((first, size)),
            },
            None => files.push((path.to_owned(), size)),
        }
    }
    // Basalt's packs: a build reads one format, the latest release the newest,
    // and a repository can hold both (the name doesn't say).
    if files.iter().any(|(f, _)| f.ends_with(".basalt")) {
        let formats: Vec<Option<u32>> = std::thread::scope(|s| {
            let reads: Vec<_> = files.iter().map(|(f, _)| s.spawn(|| basalt_format_online(repo, f))).collect();
            reads.into_iter().map(|r| r.join().ok().flatten()).collect()
        });
        let newest = formats.iter().flatten().max().copied();
        let mut format = formats.into_iter();
        files.retain(|_| format.next().flatten().is_none_or(|v| Some(v) == newest));
    }
    files.sort_by_key(|a| a.1);
    Ok(files)
}

/// The format of a `.basalt` pack, from its first bytes (`BASALTMF`, then the
/// version, little-endian).
fn basalt_format_in(head: &[u8]) -> Option<u32> {
    let version = head.strip_prefix(b"BASALTMF")?.first_chunk::<4>()?;
    Some(u32::from_le_bytes(*version))
}

/// The format of a pack on this computer.
pub fn basalt_format(path: &Path) -> Option<u32> {
    let mut head = [0u8; 12];
    std::fs::File::open(path).ok()?.read_exact(&mut head).ok()?;
    basalt_format_in(&head)
}

/// The format of a pack on Hugging Face, without downloading it.
pub fn basalt_format_online(repo: &str, file: &str) -> Option<u32> {
    let resp = http(Duration::from_secs(20)).get(&hf_url(repo, file)).header("User-Agent", "harlo").header("Range", "bytes=0-11").call().ok()?;
    let mut head = Vec::new();
    resp.into_body().into_reader().take(12).read_to_end(&mut head).ok()?;
    basalt_format_in(&head)
}

/// The pack format a Basalt build reads, from the reader in its source
/// (`<source>/bin/basalt`, `<source>/tools/model_file.py`).
pub fn basalt_build_format(program: &Path) -> Option<u32> {
    let reader = std::fs::read_to_string(program.parent()?.parent()?.join("tools/model_file.py")).ok()?;
    reader.lines().find_map(|l| l.strip_prefix("VERSION = ")?.trim().parse().ok())
}

/// A split GGUF's first part (`…-00001-of-0000N.gguf`).
fn split_first(path: &str) -> Option<String> {
    let stem = path.strip_suffix(".gguf")?;
    let (head, total) = stem.rsplit_once("-of-")?;
    let (base, _) = head.rsplit_once('-')?;
    (total.chars().all(|c| c.is_ascii_digit())).then(|| format!("{base}-{}-of-{total}.gguf", "0".repeat(total.len() - 1) + "1"))
}

/// The files of a GGUF (all its parts).
pub fn gguf_parts(first: &str) -> Vec<String> {
    let Some(stem) = first.strip_suffix(".gguf") else { return vec![first.to_owned()] };
    match stem.rsplit_once("-of-").and_then(|(head, total)| Some((head.rsplit_once('-')?.0, total))) {
        Some((base, total)) if total.chars().all(|c| c.is_ascii_digit()) => {
            let n: usize = total.parse().unwrap_or(1);
            (1..=n).map(|i| format!("{base}-{i:0w$}-of-{total}.gguf", w = total.len())).collect()
        }
        _ => vec![first.to_owned()],
    }
}

pub fn hf_url(repo: &str, file: &str) -> String {
    format!("https://huggingface.co/{}/resolve/main/{file}", repo.trim().trim_matches('/'))
}

/// A free port on this computer from `from` up.
pub fn free_port(from: u16) -> u16 {
    (from..from + 200).find(|p| std::net::TcpListener::bind(("127.0.0.1", *p)).is_ok()).unwrap_or(from)
}

/// The context a server set itself (fitting the model and context to
/// memory): llama.cpp's `/props`, or the model Ollama has loaded
/// (`/api/ps`). Neither says it in `/models`.
pub fn served_context(base_url: &str) -> Option<u64> {
    let root = base_url.trim_end_matches('/').trim_end_matches("/v1");
    if let Ok(v) = get_json(&format!("{root}/props")) {
        return v["default_generation_settings"]["n_ctx"].as_u64().or(v["n_ctx"].as_u64()).filter(|n| *n > 0);
    }
    let v = get_json(&format!("{root}/api/ps")).ok()?;
    v["models"].as_array()?.iter().filter_map(|m| m["context_length"].as_u64()).max().filter(|n| *n > 0)
}

/// A test message to a server: its answer and speed (tokens/s).
/// Pulls a model into Ollama with its API (blocking): `progress` gets its
/// status and the bytes done of all the model's parts (0 of 0 between
/// downloads: "verifying sha256 digest", "writing manifest").
pub fn ollama_pull(base_url: &str, model: &str, progress: &mut dyn FnMut(&str, u64, u64), stop: &dyn Fn() -> bool) -> Result<(), String> {
    use std::io::BufRead;
    let root = base_url.trim_end_matches('/').trim_end_matches("/v1");
    let mut resp = http(Duration::from_secs(24 * 3600))
        .post(&format!("{root}/api/pull"))
        .header("Content-Type", "application/json")
        .send(json!({ "model": model, "stream": true }).to_string())
        .map_err(|e| format!("Ollama didn't take the download: {e}"))?;
    if resp.status().as_u16() != 200 {
        let text = resp.body_mut().read_to_string().unwrap_or_default();
        let said: Value = serde_json::from_str(&text).unwrap_or_default();
        return Err(said["error"].as_str().map(str::to_owned).unwrap_or(text));
    }
    let reader = std::io::BufReader::new(resp.body_mut().with_config().limit(u64::MAX).reader());
    // Each part (layer) by its digest: (done, size).
    let mut parts: Vec<(String, u64, u64)> = Vec::new();
    let mut ok = false;
    for line in reader.lines() {
        if stop() {
            return Err("Cancelled.".into());
        }
        let line = line.map_err(|e| format!("The download broke off: {e}"))?;
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if let Some(e) = v["error"].as_str() {
            return Err(e.to_owned());
        }
        let status = v["status"].as_str().unwrap_or_default();
        if status == "success" {
            ok = true;
        }
        match (v["digest"].as_str(), v["total"].as_u64()) {
            (Some(d), Some(size)) => {
                let done = v["completed"].as_u64().unwrap_or(0);
                match parts.iter_mut().find(|p| p.0 == d) {
                    Some(p) => (p.1, p.2) = (done.max(p.1), size),
                    None => parts.push((d.to_owned(), done, size)),
                }
                progress(status, parts.iter().map(|p| p.1).sum(), parts.iter().map(|p| p.2).sum());
            }
            _ => progress(status, 0, 0),
        }
    }
    if ok { Ok(()) } else { Err(format!("Ollama didn't finish downloading {model}.")) }
}

/// The context a server gives one model: llama.cpp's router (several
/// models) says it per model (`/props?model=`), else as [`served_context`].
pub fn served_context_of(base_url: &str, model: &str) -> Option<u64> {
    let root = base_url.trim_end_matches('/').trim_end_matches("/v1");
    let q: String = model.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
    get_json(&format!("{root}/props?model={q}"))
        .ok()
        .and_then(|v| v["default_generation_settings"]["n_ctx"].as_u64())
        .filter(|n| *n > 0)
        .or_else(|| served_context(base_url))
}

// ---- llama.cpp's models: a router server's preset file ----

/// Has a running llama.cpp router read its preset again (a model added).
pub fn reload_models(base_url: &str) {
    let root = base_url.trim_end_matches('/').trim_end_matches("/v1");
    let _ = http(Duration::from_secs(30)).get(&format!("{root}/models?reload=1")).header("User-Agent", "harlo").call();
}

/// A model of a llama.cpp server (router mode): its name (what agents ask
/// for), its GGUF and its context (None: as much as fits, not under 16k).
#[derive(Debug, Clone, PartialEq)]
pub struct PresetModel {
    pub name: String,
    pub path: PathBuf,
    pub context: Option<u64>,
}

/// Where harlo keeps the preset of its llama.cpp server on `port`.
pub fn preset_path(port: u16) -> PathBuf {
    engines_dir().parent().unwrap_or(Path::new(".")).join("servers").join(format!("llamacpp-{port}.ini"))
}

/// A preset file's models (its `[*]` aside).
pub fn read_preset(file: &Path) -> Vec<PresetModel> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let mut out: Vec<PresetModel> = Vec::new();
    let mut current: Option<PresetModel> = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with(';') || line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            out.extend(current.take().filter(|m| !m.path.as_os_str().is_empty()));
            current = (name != "*").then(|| PresetModel { name: name.to_owned(), path: PathBuf::new(), context: None });
            continue;
        }
        let (Some(m), Some((k, v))) = (current.as_mut(), line.split_once('=')) else { continue };
        match k.trim() {
            "model" | "m" => m.path = PathBuf::from(v.trim()),
            "c" | "ctx-size" => m.context = v.trim().parse().ok().filter(|c| *c > 0),
            _ => {}
        }
    }
    out.extend(current.filter(|m| !m.path.as_os_str().is_empty()));
    out
}

/// Writes a llama.cpp server's preset: its models, each with its context.
pub fn write_preset(file: &Path, models: &[PresetModel]) -> std::io::Result<()> {
    let mut t = String::from("; harlo's llama.cpp server: its models (router mode), one loaded at a time.\n[*]\njinja = true\n");
    for m in models {
        t.push_str(&format!("\n[{}]\nmodel = {}\n", m.name, m.path.display()));
        match m.context {
            Some(c) => t.push_str(&format!("c = {c}\n")),
            None => t.push_str("fit-ctx = 16384\n"),
        }
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(file, t)
}

/// The models Ollama has and their sizes (`/api/tags`; it must be running).
pub fn ollama_sizes(base_url: &str) -> Vec<(String, u64)> {
    let root = base_url.trim_end_matches('/').trim_end_matches("/v1");
    get_json(&format!("{root}/api/tags"))
        .ok()
        .and_then(|v| v["models"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|m| Some((m["name"].as_str().or(m["model"].as_str())?.to_owned(), m["size"].as_u64().unwrap_or(0))))
        .collect()
}

/// Deletes a model from Ollama's store (`/api/delete`; it must be running).
pub fn ollama_delete(base_url: &str, model: &str) -> Result<(), String> {
    let root = base_url.trim_end_matches('/').trim_end_matches("/v1");
    let resp = http(Duration::from_secs(60))
        .delete(&format!("{root}/api/delete"))
        .header("Content-Type", "application/json")
        .force_send_body()
        .send(json!({ "model": model }).to_string())
        .map_err(|e| format!("{model}: {e}"))?;
    match resp.status().as_u16() {
        200 | 404 => Ok(()),
        s => Err(format!("Ollama didn't delete {model} (HTTP {s})")),
    }
}

/// A model's context on a server that doesn't list it: what the server
/// serves now (`served_context`), else, for Ollama with nothing loaded, the
/// model's own most (`/api/show`; it serves no more than that).
pub fn model_context(base_url: &str, model: &str) -> Option<u64> {
    served_context(base_url).or_else(|| {
        let root = base_url.trim_end_matches('/').trim_end_matches("/v1");
        let mut resp = http(Duration::from_secs(10)).post(&format!("{root}/api/show")).header("Content-Type", "application/json").send(json!({ "model": model }).to_string()).ok()?;
        let v: Value = serde_json::from_str(&resp.body_mut().read_to_string().ok()?).ok()?;
        v["model_info"].as_object()?.iter().find(|(k, _)| k.ends_with(".context_length")).and_then(|(_, n)| n.as_u64())
    })
}

/// The speed is the second answer's: the first one loads the model (Ollama
/// does it then), which isn't how fast it writes. Room for a model that
/// thinks first; its thinking when that's all it said. Also whether it
/// thought (its thinking apart from its answer, or in `<think>` tags).
pub fn test_chat(base_url: &str, model: &str) -> Result<(String, f64, bool), String> {
    let ask = |max_tokens: u64| -> Result<(Value, f64), String> {
        let started = std::time::Instant::now();
        let body = json!({ "model": model, "messages": [{ "role": "user", "content": "Say hello in five words." }], "max_tokens": max_tokens, "stream": false });
        let mut resp = http(Duration::from_secs(600))
            .post(&format!("{}/chat/completions", base_url.trim_end_matches('/')))
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .map_err(|e| format!("No answer: {e}"))?;
        let status = resp.status().as_u16();
        let text = resp.body_mut().read_to_string().unwrap_or_default();
        if status != 200 {
            return Err(format!("It answered HTTP {status}: {}", text.chars().take(200).collect::<String>()));
        }
        Ok((serde_json::from_str(&text).map_err(|e| e.to_string())?, started.elapsed().as_secs_f64().max(0.001)))
    };
    ask(1)?;
    let (v, secs) = ask(1024)?;
    let message = &v["choices"][0]["message"];
    let said = |k: &str| message[k].as_str().unwrap_or_default().trim().to_owned();
    let answer = Some(said("content")).filter(|a| !a.is_empty()).unwrap_or_else(|| {
        let thought = Some(said("reasoning_content")).filter(|t| !t.is_empty()).unwrap_or_else(|| said("reasoning"));
        let short: String = thought.chars().take(120).collect();
        if short.is_empty() { short } else { format!("(thinking) {short}…") }
    });
    let tokens = v["usage"]["completion_tokens"].as_f64().unwrap_or(0.0);
    let thought = !said("reasoning_content").is_empty() || !said("reasoning").is_empty() || said("content").contains("<think>");
    Ok((answer, tokens / secs, thought))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hw(vendor: &str, vram_gb: u64, ram_gb: u64, driver: &str) -> Hardware {
        let gpus = if vendor.is_empty() { vec![] } else { vec![Gpu { vendor: vendor.into(), name: "card".into(), vram_mb: vram_gb * 1024, arch: String::new() }] };
        Hardware { gpus, ram_mb: ram_gb * 1024, disk_free_mb: 500 * 1024, avx2: true, nvidia_driver: driver.into() }
    }

    #[test]
    #[ignore = "online"]
    fn latest_versions_online() {
        for e in ENGINES {
            let latest = latest_version(e).unwrap();
            let here = installed(e).map(|i| i.version).unwrap_or_default();
            println!("{}: here {here:?}, latest {latest:?}, update {}", e.id(), update_available(e, &here, &latest));
            assert!(!latest.is_empty());
        }
    }

    #[test]
    fn compares_versions() {
        assert!(update_available(Engine::Strata, "0.2.9", "v0.2.10"));
        assert!(!update_available(Engine::Strata, "0.2.9", "v0.2.9"));
        // (Strata's 0.2.7.1 came out under the tag v0.2.7.)
        assert!(!update_available(Engine::Strata, "0.2.7.1", "v0.2.7"));
        assert!(update_available(Engine::LlamaCpp, "b12024", "b12100"));
        assert!(update_available(Engine::Vllm, "0.13.6", "0.14.0"));
        assert!(!update_available(Engine::Ollama, "", "v0.40.13"));
        assert!(update_available(Engine::Ninfer, "20261029-1882c6ad", "20261031-aaaaaaaa"));
        assert!(!update_available(Engine::Ninfer, "20261029-1882c6ad", "20261029-1882c6ad"));
    }

    #[test]
    fn strata_engine_faults_are_read_from_its_latest_run() {
        let healthy = "[strata] engine started: x\nds4: CUDA0 layers [0, 39] + head\nmaya ds4 serve: prompt 31 tokens\n";
        assert!(!strata_engine_faulted(healthy));
        assert!(strata_engine_faulted(&format!("{healthy}ds4: q: an illegal memory access was encountered\n")));
        assert!(strata_engine_faulted(&format!("{healthy}ds4: cudaStreamSynchronize(s): operation not supported on global/shared address space\n")));
        assert!(!strata_engine_faulted(&format!("{healthy}ds4: q: an illegal memory access was encountered\n{healthy}")));
    }

    #[test]
    fn strata_models_name_their_family() {
        assert_eq!(strata_family("ist-das-2.00bpw"), "deepseek41");
        assert_eq!(strata_family("ud-q8_k_xl"), "deepseek41");
        assert_eq!(strata_family("iq2_xs"), "qwen");
        assert_eq!(strata_family("coder"), "qwen");
    }

    #[test]
    fn reads_the_hardware() {
        let (gpus, driver) = nvidia_gpus("NVIDIA GeForce RTX 5090, 32607, 12.0, 615.78.08\n");
        assert_eq!(gpus, vec![Gpu { vendor: "nvidia".into(), name: "NVIDIA GeForce RTX 5090".into(), vram_mb: 32607, arch: "12.0".into() }]);
        assert_eq!(driver, "615.78.08");
        let h = Hardware { gpus, ram_mb: 95637, disk_free_mb: 1_100_000, avx2: true, nvidia_driver: driver };
        assert_eq!(h.summary(), "NVIDIA GeForce RTX 5090 · 32 GB VRAM · 93 GB RAM · 1074 GB free");
        assert!(h.cuda13());
    }

    #[test]
    fn suits_the_hardware() {
        assert_eq!(Engine::Strata.fit(&hw("nvidia", 32, 94, "615.1")).0, "ok");
        assert_eq!(Engine::Strata.fit(&hw("nvidia", 8, 94, "615.1")).0, "no");
        assert_eq!(Engine::Strata.fit(&hw("nvidia", 12, 16, "615.1")).0, "warn");
        assert_eq!(Engine::Strata.fit(&hw("", 0, 64, "")).0, "no");
        assert_eq!(Engine::Ollama.fit(&hw("", 0, 16, "")).0, "warn");
        assert_eq!(Engine::LlamaCpp.fit(&hw("amd", 20, 64, "")).0, "ok");

        assert_eq!(llamacpp_build(&hw("nvidia", 32, 94, "615.1"), "b1"), ("llama-b1-bin-ubuntu-cuda-13.4-x64.tar.gz".into(), Some("cudart-llama-b1-bin-ubuntu-cuda-13.4-x64.tar.gz".into())));
        assert_eq!(llamacpp_build(&hw("nvidia", 12, 32, "560.2"), "b1").0, "llama-b1-bin-ubuntu-cuda-12.8-x64.tar.gz");
        assert_eq!(llamacpp_build(&hw("amd", 20, 64, ""), "b1"), ("llama-b1-bin-ubuntu-vulkan-x64.tar.gz".into(), None));
        assert_eq!(llamacpp_build(&hw("", 0, 64, ""), "b1").0, "llama-b1-bin-ubuntu-x64.tar.gz");
    }

    #[test]
    fn picks_checked_files() {
        let rel = json!({ "tag_name": "v1", "assets": [
            { "name": "a.tar.zst", "size": 10, "browser_download_url": "https://x/a.tar.zst" },
            { "name": "b.tar.gz", "size": 20, "browser_download_url": "https://x/b.tar.gz", "digest": "sha256:ABCDEF" },
        ] });
        let sums = "0123  a.tar.zst\n4567 *other.txt\n";
        assert_eq!(asset_named(&rel, "a.tar.zst", Some(sums)).unwrap().sha256, "0123");
        assert_eq!(asset_named(&rel, "b.tar.gz", None).unwrap(), Asset { name: "b.tar.gz".into(), url: "https://x/b.tar.gz".into(), sha256: "abcdef".into(), size: 20 });
        // No checksum: not used.
        assert!(asset_named(&rel, "a.tar.zst", None).is_none());
    }

    #[test]
    fn names_quants_and_picks_files() {
        assert_eq!(quant_of("Qwen3-4B-UD-Q4_K_XL.gguf"), "UD-Q4_K_XL");
        assert_eq!(quant_of("Q4_K_M/Qwen3-Coder-30B-A3B-Instruct-Q4_K_M-00001-of-00002.gguf"), "Q4_K_M");
        assert_eq!(quant_of("gpt-oss-20b-F16.gguf"), "F16");
        assert_eq!(quant_of("model-IQ2_XXS.gguf"), "IQ2_XXS");
        assert_eq!(quant_of("gpt-oss-20b-MXFP4.gguf"), "MXFP4");
        assert_eq!(quant_of("Qwen3.8-FN-GSQ-RCO-IQ3_S.basalt"), "GSQ-RCO-IQ3_S");
        assert_eq!(quant_of("Qwen3.8-FN-NVFP4.basalt"), "NVFP4");
        let gb = |n: u64| n * 1024 * 1024 * 1024;
        let files = vec![("a-Q2_K.gguf".to_owned(), gb(10)), ("a-Q4_K_M.gguf".to_owned(), gb(18)), ("a-Q8_0.gguf".to_owned(), gb(33)), ("a-F16.gguf".to_owned(), gb(60))];
        // Q4 where it fits, on the card or partly in RAM.
        assert_eq!(pick_file(&files, 48 * 1024, 94 * 1024).as_deref(), Some("a-Q4_K_M.gguf"));
        assert_eq!(pick_file(&files, 8 * 1024, 64 * 1024).as_deref(), Some("a-Q4_K_M.gguf"));
        // No Q4 on the card: the largest that is.
        assert_eq!(pick_file(&files, 16 * 1024, 64 * 1024).as_deref(), Some("a-Q2_K.gguf"));
        let no_q4: Vec<(String, u64)> = files.iter().filter(|(f, _)| !f.contains("Q4")).cloned().collect();
        assert_eq!(pick_file(&no_q4, 40 * 1024, 64 * 1024).as_deref(), Some("a-Q8_0.gguf"));
        assert_eq!(pick_file(&files, 0, 8 * 1024).as_deref(), Some("a-Q2_K.gguf"));
    }

    #[test]
    fn joins_split_ggufs() {
        assert_eq!(split_first("Q4/model-Q4_K_M-00002-of-00003.gguf").as_deref(), Some("Q4/model-Q4_K_M-00001-of-00003.gguf"));
        assert_eq!(split_first("model-Q4_K_M.gguf"), None);
        assert_eq!(gguf_parts("Q4/m-00001-of-00003.gguf"), ["Q4/m-00001-of-00003.gguf", "Q4/m-00002-of-00003.gguf", "Q4/m-00003-of-00003.gguf"]);
        assert_eq!(gguf_parts("m.gguf"), ["m.gguf"]);
        assert!(hf_ggufs("not-a-repo").is_err());
        let p = free_port(41000);
        assert!((41000..41200).contains(&p));
    }

    #[test]
    fn downloads_resume_and_check() {
        use sha2::{Digest, Sha256};
        use std::io::{Read, Write};
        let body: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let sha = format!("{:x}", Sha256::digest(&body));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let served = body.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(2) {
                let Ok(mut s) = stream else { continue };
                let mut buf = [0u8; 2048];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_lowercase();
                let from: usize = req.lines().find_map(|l| l.strip_prefix("range: bytes=")?.trim_end_matches('-').parse().ok()).unwrap_or(0);
                let part = &served[from..];
                let status = if from > 0 { "206 Partial Content" } else { "200 OK" };
                let _ = write!(s, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", part.len());
                let _ = s.write_all(part);
            }
        });
        let dir = std::env::temp_dir().join(format!("harlo-engines-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("file.bin");
        // Half of it already there: resumed.
        std::fs::write(dest.with_extension("part"), &body[..100_000]).unwrap();
        let mut last = (0, 0);
        download(&format!("http://{addr}/file.bin"), &dest, Some(&sha), &mut |_, d, t| last = (d, t), &|| false).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        assert_eq!(last, (300_000, 300_000));
        // A wrong checksum: not kept.
        std::fs::remove_file(&dest).unwrap();
        assert!(download(&format!("http://{addr}/file.bin"), &dest, Some("00"), &mut |_, _, _| {}, &|| false).unwrap_err().contains("checksum"));
        assert!(!dest.exists() && !dest.with_extension("part").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore = "network"]
    fn lists_basalt_packs_of_one_format() {
        let files = hf_ggufs("jesdga/Qwen3.8-Flash-Next-Basalt").unwrap();
        let formats: Vec<_> = files.iter().map(|(f, _)| basalt_format_online("jesdga/Qwen3.8-Flash-Next-Basalt", f).unwrap()).collect();
        println!("{files:?} {formats:?}");
        assert!(!files.is_empty());
        assert!(formats.iter().all(|v| *v == formats[0]));
    }

    #[test]
    fn reads_basalt_formats() {
        assert_eq!(basalt_format_in(b"BASALTMF\x02\0\0\0\x12\0\0\0"), Some(2));
        assert_eq!(basalt_format_in(b"BASALTMF\x01\0\0\0"), Some(1));
        assert_eq!(basalt_format_in(b"BASALTMF\x01"), None);
        assert_eq!(basalt_format_in(b"GGUF\x03\0\0\0\0\0\0\0"), None);
        assert_eq!(quant_of("Qwen3.8-FN-GSQ-RCO-Q2_0_v2.basalt"), "GSQ-RCO-Q2_0");

        let dir = std::env::temp_dir().join(format!("harlo-basalt-format-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::create_dir_all(dir.join("tools")).unwrap();
        std::fs::write(dir.join("tools/model_file.py"), "MAGIC = b\"BASALTMF\"\nVERSION = 2\n").unwrap();
        assert_eq!(basalt_build_format(&dir.join("bin/basalt")), Some(2));
        assert_eq!(basalt_build_format(Path::new("/usr/bin/basalt")), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ninfer_needs_an_rtx_5090() {
        let mut card = hw("nvidia", 32, 93, "580.1");
        card.gpus[0].arch = "12.0".into();
        card.gpus[0].name = "NVIDIA GeForce RTX 5080".into();
        assert_eq!(Engine::Ninfer.fit(&card).0, "no");
        card.gpus[0].name = "NVIDIA GeForce RTX 5090".into();
        // (The build tools: this computer's.)
        let fit = Engine::Ninfer.fit(&card);
        assert!(fit.0 == "ok" || fit.1.contains("installed first"), "{fit:?}");
        assert_eq!(ninfer_name("neroued/Qwen3.8-27B-nvfp4-NInfer"), "Qwen3.8-27B-NVFP4");
        assert_eq!(ninfer_name("neroued/Qwen3.8-Flash-Next-NInfer"), "Qwen3.8-Flash-Next");
    }

    #[test]
    #[ignore = "network: Hugging Face and GitHub"]
    fn ninfer_from_its_own_repository() {
        let models = hf_ninfer_models("").unwrap();
        println!("{:?}", models.iter().map(|m| (&m.repo, m.bytes)).collect::<Vec<_>>());
        assert!(models.iter().all(|m| m.repo.starts_with("neroued/") && !m.repo.contains("GLM") && m.bytes > 0));
        assert_eq!(hf_ninfer_models("flash").unwrap().len(), 1);
        let files = hf_ninfer_files("neroued/Qwen3.8-Flash-Next-NInfer").unwrap();
        assert_eq!(files.len(), 4);
        assert!(files[0].0.ends_with(".ninfer") && files[1..].iter().all(|(p, _)| p.contains(".part-")));
        let (tag, assets) = plan(Engine::Ninfer, &hw("nvidia", 32, 93, "580.1")).unwrap();
        println!("{tag} {}", assets[0].url);
        assert!(assets[0].url.contains("Neroued/ninfer") && tag.len() == 17);
    }

    #[test]
    fn basalt_needs_a_blackwell_card() {
        let mut hw = hw("nvidia", 32, 93, "580.1");
        hw.gpus[0].arch = "8.9".into();
        assert_eq!(Engine::Basalt.fit(&hw).0, "no");
        hw.gpus[0].arch = "12.0".into();
        // (The build tools: this computer's.)
        let fit = Engine::Basalt.fit(&hw);
        assert!(fit.0 == "ok" || fit.1.contains("installed first"), "{fit:?}");
        let small = { let mut h = hw.clone(); h.ram_mb = 16 * 1024; h };
        assert!(Engine::Basalt.fit(&small).0 == "no");
    }

    #[test]
    fn picks_vllm_parsers() {
        assert_eq!(vllm_parsers("qwen3", "Qwen/Qwen3-4B-AWQ"), ("hermes", Some("qwen3")));
        assert_eq!(vllm_parsers("qwen3_moe", "Qwen/Qwen3-Coder-30B-A3B-Instruct-FP8"), ("qwen3_coder", None));
        assert_eq!(vllm_parsers("gpt_oss", "openai/gpt-oss-20b"), ("openai", Some("openai_gptoss")));
        assert_eq!(vllm_parsers("something_new", "x/y"), ("hermes", None));
    }

    #[test]
    fn presets_round_trip() {
        let dir = std::env::temp_dir().join(format!("harlo-preset-{}", std::process::id()));
        let file = dir.join("llamacpp-8080.ini");
        let models = vec![
            PresetModel { name: "Qwen3-Coder-30B-A3B-Instruct-UD-Q4_K_XL".into(), path: PathBuf::from("/m/a b/coder.gguf"), context: Some(65536) },
            PresetModel { name: "Qwen3-4B-UD-Q4_K_XL".into(), path: PathBuf::from("/m/q.gguf"), context: None },
        ];
        write_preset(&file, &models).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("[*]\njinja = true") && text.contains("c = 65536") && text.contains("fit-ctx = 16384"), "{text}");
        assert_eq!(read_preset(&file), models);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A server like Hugging Face's: HEAD gives the size and SHA-256, GET
    /// gives byte ranges (or, `ranges` false, only the whole file).
    fn ranged_server(body: Vec<u8>, sha: String, ranges: bool) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let body = std::sync::Arc::new(body);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                let (body, sha) = (body.clone(), sha.clone());
                std::thread::spawn(move || {
                    let mut buf = [0u8; 4096];
                    let n = s.read(&mut buf).unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_lowercase();
                    if req.starts_with("head ") {
                        let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nX-Linked-Etag: \"{sha}\"\r\nConnection: close\r\n\r\n", body.len());
                        return;
                    }
                    let range = req.lines().find_map(|l| l.strip_prefix("range: bytes=")).and_then(|r| r.split_once('-')).map(|(a, b)| (a.trim().parse::<usize>().unwrap_or(0), b.trim().parse::<usize>().unwrap_or(body.len() - 1)));
                    match range.filter(|_| ranges) {
                        Some((a, b)) => {
                            let part = &body[a..=b.min(body.len() - 1)];
                            let _ = write!(s, "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {a}-{b}/{}\r\nConnection: close\r\n\r\n", part.len(), body.len());
                            let _ = s.write_all(part);
                        }
                        None => {
                            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                            let _ = s.write_all(&body);
                        }
                    }
                });
            }
        });
        format!("http://{addr}/model.gguf")
    }

    #[test]
    fn downloads_in_parts_resumes_and_checks() {
        use sha2::{Digest, Sha256};
        // 100 MB: 3 parts of about 33 MB.
        let body: Vec<u8> = (0..100u32 << 20).map(|i| (i % 253) as u8).collect();
        let sha = format!("{:x}", Sha256::digest(&body));
        let url = ranged_server(body.clone(), sha.clone(), true);
        let dir = std::env::temp_dir().join(format!("harlo-engines-parts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let dest = dir.join("model.gguf");
        let mut last = (String::new(), 0, 0);
        download_parallel(&url, &dest, None, 8, &mut |phase, d, t| last = (phase.to_owned(), d, t), &|| false).unwrap();
        assert!(std::fs::read(&dest).unwrap() == body, "the parts make the file");
        // (Checked against the SHA-256 it published, last.)
        assert_eq!(last, ("check".to_owned(), body.len() as u64, body.len() as u64));
        assert!(!dest.with_extension("part").exists() && !dest.with_extension("parts").exists());

        // Broken off: each part goes on from where it got.
        std::fs::remove_file(&dest).unwrap();
        let span = (body.len() as u64).div_ceil(3);
        let mut half = vec![0u8; body.len()];
        for i in 0..3u64 {
            let (a, b) = ((i * span) as usize, ((i * span) as usize + 1_000_000).min(body.len()));
            half[a..b].copy_from_slice(&body[a..b]);
        }
        std::fs::write(dest.with_extension("part"), &half).unwrap();
        std::fs::write(dest.with_extension("parts"), json!({ "size": body.len(), "done": [1_000_000, 1_000_000, 1_000_000] }).to_string()).unwrap();
        download_parallel(&url, &dest, None, 8, &mut |_, _, _| {}, &|| false).unwrap();
        assert!(std::fs::read(&dest).unwrap() == body, "resumed");

        // A wrong checksum: not kept.
        std::fs::remove_file(&dest).unwrap();
        assert!(download_parallel(&url, &dest, Some("00"), 8, &mut |_, _, _| {}, &|| false).unwrap_err().contains("checksum"));
        assert!(!dest.exists() && !dest.with_extension("part").exists());

        // A server without ranges: one connection, still checked.
        let whole = ranged_server(body.clone(), sha.clone(), false);
        download_parallel(&whole, &dest, None, 8, &mut |_, _, _| {}, &|| false).unwrap();
        assert!(std::fs::read(&dest).unwrap() == body, "one connection");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A real model from Hugging Face in parts (HARLO_DL_DIR: where to).
    #[test]
    #[ignore = "network: downloads a model from Hugging Face"]
    fn downloads_from_hugging_face() {
        let dir = PathBuf::from(std::env::var("HARLO_DL_DIR").expect("HARLO_DL_DIR"));
        let dest = dir.join("Qwen3-0.6B-Q4_K_M.gguf");
        let _ = std::fs::remove_file(&dest);
        let started = std::time::Instant::now();
        let mut checked = false;
        download_parallel(&hf_url("unsloth/Qwen3-0.6B-GGUF", "Qwen3-0.6B-Q4_K_M.gguf"), &dest, None, 8, &mut |phase, _, _| checked |= phase == "check", &|| false).unwrap();
        let mb = std::fs::metadata(&dest).unwrap().len() as f64 / 1e6;
        println!("{mb:.0} MB in {:.1}s ({:.0} MB/s), checked: {checked}", started.elapsed().as_secs_f64(), mb / started.elapsed().as_secs_f64());
        assert!(checked);
    }

    /// vLLM's models and a repository's files, from Hugging Face (reads only).
    #[test]
    #[ignore = "network: Hugging Face"]
    fn vllm_models_from_hugging_face() {
        let list = hf_vllm_models("").unwrap();
        for m in list.iter().take(10) {
            println!("{} · {:.1} GB · {}", m.repo, m.bytes as f64 / 1e9, m.model_type);
        }
        let files = hf_vllm_files("Qwen/Qwen3-4B-AWQ").unwrap();
        println!("Qwen3-4B-AWQ: {:?}", files.iter().map(|(p, s)| format!("{p} {}", s / 1_000_000)).collect::<Vec<_>>());
        assert!(!list.is_empty());
    }

    /// What would be installed here, from the real releases (metadata only).
    #[test]
    #[ignore = "network: GitHub and Hugging Face"]
    fn this_computer() {
        let hw = detect();
        println!("{}", hw.summary());
        for e in ENGINES {
            println!("{}: fit {:?}, installed {:?}", e.name(), e.fit(&hw), installed(e).map(|i| (i.program, i.version, i.harlo)));
            match plan(e, &hw) {
                Ok((tag, assets)) => {
                    for a in assets {
                        println!("  {tag}: {} ({} MB, sha256 {}…)", a.name, a.size / 1_000_000, a.sha256.get(..12).unwrap_or("none"));
                    }
                }
                Err(err) => println!("  plan failed: {err}"),
            }
        }
        let files = hf_ggufs("unsloth/Qwen3-4B-GGUF").unwrap();
        println!("unsloth/Qwen3-4B-GGUF: {} files, smallest {:?}", files.len(), files.first());
        assert!(!files.is_empty());
        let models = hf_models("").unwrap();
        for m in models.iter().take(8) {
            println!("  {} {}B ctx {} → {} at Q4", m.repo, m.params / 1_000_000_000, m.context, fit(q4_bytes(m.params), hw.vram_mb(), hw.ram_mb));
        }
        assert!(!models.is_empty());
        let coder = hf_ggufs("unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF").unwrap();
        println!("coder picks {:?}; quants {:?}", pick_file(&coder, hw.vram_mb(), hw.ram_mb), coder.iter().map(|(f, _)| quant_of(f)).collect::<Vec<_>>());
    }
}
