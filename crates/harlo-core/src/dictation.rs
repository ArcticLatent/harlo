//! Dictation: speech to text on this computer with whisper.cpp, built into
//! harlo (`whisper-rs`). The app records the microphone (Qt Multimedia) and
//! hands the samples here; the model downloads on first use.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, bail};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// whisper's input: 16 kHz mono.
pub const SAMPLE_RATE: u32 = 16_000;

/// Models offered: name, file, size in bytes, description.
pub const MODELS: [(&str, &str, u64, &str); 2] = [
    ("base", "ggml-base.bin", 147_951_465, "Base · 148 MB · quick, good for clear speech"),
    ("small", "ggml-small.bin", 487_601_967, "Small · 488 MB · slower, more accurate"),
];

/// Where downloaded models live.
pub fn models_dir() -> PathBuf {
    crate::db::default_path().parent().map(|p| p.join("whisper")).unwrap_or_else(|| PathBuf::from("whisper"))
}

fn model(name: &str) -> Option<(&'static str, &'static str, u64, &'static str)> {
    MODELS.iter().copied().find(|m| m.0 == name)
}

pub fn model_path(name: &str) -> Option<PathBuf> {
    model(name).map(|m| models_dir().join(m.1))
}

pub fn model_size(name: &str) -> u64 {
    model(name).map_or(0, |m| m.2)
}

/// Where a model downloads from (`HARLO_TEST_MODEL_URL`, a base URL,
/// replaces Hugging Face in tests).
pub fn model_url(name: &str) -> Option<String> {
    let base = std::env::var("HARLO_TEST_MODEL_URL").unwrap_or_else(|_| "https://huggingface.co/ggerganov/whisper.cpp/resolve/main".into());
    model(name).map(|m| format!("{}/{}", base.trim_end_matches('/'), m.1))
}

/// Downloaded (a download only lands under its name once complete).
pub fn model_ready(name: &str) -> bool {
    model_path(name).is_some_and(|p| std::fs::metadata(p).is_ok_and(|m| m.len() > 0))
}

/// Downloads a model, reporting progress (0–1); `cancel` stops it.
pub fn download(name: &str, cancel: &AtomicBool, mut progress: impl FnMut(f64)) -> Result<()> {
    let (Some(url), Some(path)) = (model_url(name), model_path(name)) else { bail!("unknown model {name}") };
    let dir = path.parent().context("model folder")?;
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let part = path.with_extension("part");
    let result = (|| -> Result<()> {
        let mut response = ureq::get(&url).call().with_context(|| format!("downloading {url}"))?;
        let total = response.body().content_length().unwrap_or(model_size(name)).max(1);
        let mut reader = response.body_mut().as_reader();
        let mut file = std::fs::File::create(&part).with_context(|| format!("creating {}", part.display()))?;
        let mut buf = vec![0u8; 1 << 16];
        let (mut got, mut reported) = (0u64, 0.0);
        loop {
            if cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            let n = reader.read(&mut buf).context("downloading")?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).context("saving the model")?;
            got += n as u64;
            let fraction = got as f64 / total as f64;
            if fraction - reported >= 0.01 {
                reported = fraction;
                progress(fraction.min(1.0));
            }
        }
        file.sync_all().ok();
        if got < total {
            bail!("the download stopped early ({got} of {total} bytes)");
        }
        std::fs::rename(&part, &path).context("saving the model")
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

/// Interleaved samples at any rate → 16 kHz mono (averaged channels,
/// linear interpolation; fine for speech).
pub fn to_whisper_input(samples: &[f32], rate: u32, channels: u16) -> Vec<f32> {
    let channels = channels.max(1) as usize;
    let mono: Vec<f32> = samples.chunks(channels).map(|c| c.iter().sum::<f32>() / c.len() as f32).collect();
    if rate == SAMPLE_RATE || rate == 0 || mono.is_empty() {
        return mono;
    }
    let step = rate as f64 / SAMPLE_RATE as f64;
    let len = (mono.len() as f64 / step).floor() as usize;
    (0..len)
        .map(|i| {
            let pos = i as f64 * step;
            let (j, frac) = (pos.floor() as usize, (pos - pos.floor()) as f32);
            let a = mono[j];
            let b = mono.get(j + 1).copied().unwrap_or(a);
            a + (b - a) * frac
        })
        .collect()
}

/// The loaded model, kept between dictations.
static CONTEXT: Mutex<Option<(PathBuf, WhisperContext)>> = Mutex::new(None);

/// Transcribes 16 kHz mono samples with the named model; `language` is a
/// code like "en", or "auto".
pub fn transcribe(model_name: &str, samples: &[f32], language: &str) -> Result<String> {
    let path = model_path(model_name).filter(|_| model_ready(model_name)).context("The speech model isn't downloaded yet.")?;
    if samples.len() < SAMPLE_RATE as usize / 4 {
        bail!("Nothing was recorded. Check the microphone.");
    }
    whisper_rs::install_logging_hooks();
    let mut guard = CONTEXT.lock().unwrap_or_else(|e| e.into_inner());
    if guard.as_ref().is_none_or(|(p, _)| *p != path) {
        *guard = None;
        let ctx = WhisperContext::new_with_params(&path, WhisperContextParameters::default())
            .map_err(|e| anyhow::anyhow!("Could not load the speech model: {e}"))?;
        *guard = Some((path, ctx));
    }
    let (_, ctx) = guard.as_ref().expect("loaded above");
    let mut state = ctx.create_state().map_err(|e| anyhow::anyhow!("speech model: {e}"))?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    let language = if language.trim().is_empty() { "auto" } else { language.trim() };
    params.set_language(Some(language));
    params.set_n_threads(std::thread::available_parallelism().map_or(4, |n| n.get().min(8)) as i32);
    params.set_no_timestamps(true);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);
    state.full(params, samples).map_err(|e| anyhow::anyhow!("transcription failed: {e}"))?;
    let text: Vec<String> = state.as_iter().filter_map(|s| s.to_str_lossy().ok().map(|t| t.into_owned())).collect();
    Ok(transcript_text(&text.join("\n")))
}

/// The recognised text: lines joined, markers for silence and sounds
/// (`[BLANK_AUDIO]`, `(music)`) left out.
pub fn transcript_text(raw: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in raw.lines() {
        let mut rest = line.trim();
        let mut kept = String::new();
        while let Some(start) = rest.find(['[', '(']) {
            let close = if rest.as_bytes()[start] == b'[' { ']' } else { ')' };
            let Some(len) = rest[start..].find(close) else { break };
            let inner = &rest[start + 1..start + len];
            kept.push_str(&rest[..start]);
            // Markers are in capitals or a sound word; real parentheses stay.
            let marker = inner.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c == ' ')
                || ["music", "silence", "applause", "laughter", "noise", "inaudible", "blank_audio"].contains(&inner.to_ascii_lowercase().as_str());
            if !marker {
                kept.push_str(&rest[start..=start + len]);
            }
            rest = &rest[start + len + 1..];
        }
        kept.push_str(rest);
        lines.push(kept);
    }
    lines.join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_whisper_output() {
        assert_eq!(transcript_text(" Add a cache to the loader.\n And write tests (unit ones).\n"), "Add a cache to the loader. And write tests (unit ones).");
        assert_eq!(transcript_text("[BLANK_AUDIO]\n"), "");
        assert_eq!(transcript_text(" (music) Hello [MUSIC] there\n"), "Hello there");
        assert_eq!(transcript_text(""), "");
    }

    #[test]
    fn converts_to_16k_mono() {
        // 48 kHz stereo, one second: 16 000 mono samples, channels averaged.
        let stereo: Vec<f32> = (0..48_000).flat_map(|_| [0.2f32, 0.4]).collect();
        let out = to_whisper_input(&stereo, 48_000, 2);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|s| (s - 0.3).abs() < 1e-6));
        // Already right: unchanged.
        assert_eq!(to_whisper_input(&[0.1, 0.2], 16_000, 1), vec![0.1, 0.2]);
        // A ramp keeps its shape when downsampled.
        let ramp: Vec<f32> = (0..44_100).map(|i| i as f32 / 44_100.0).collect();
        let out = to_whisper_input(&ramp, 44_100, 1);
        assert_eq!(out.len(), 16_000);
        assert!((out[8_000] - 0.5).abs() < 0.001);
    }

    #[test]
    fn models() {
        assert!(model_url("base").unwrap().ends_with("/ggml-base.bin"));
        assert!(model_path("small").unwrap().ends_with("whisper/ggml-small.bin"));
        assert!(model_url("huge").is_none());
        assert_eq!(model_size("base"), 147_951_465);
    }

    #[test]
    fn needs_a_model_and_audio() {
        assert!(transcribe("huge", &[0.0; 16_000], "auto").is_err());
    }

    /// End to end with the real model (not run by default):
    /// `HARLO_E2E_WAV=speech.wav XDG_DATA_HOME=<folder with harlo/whisper/ggml-base.bin>
    /// cargo test -p harlo-core --release -- --ignored transcribes_speech --nocapture`
    #[test]
    #[ignore]
    fn transcribes_speech() {
        let wav = std::fs::read(std::env::var("HARLO_E2E_WAV").expect("HARLO_E2E_WAV")).unwrap();
        // A 16-bit PCM WAV: its rate, channels and data chunk.
        let u16_at = |i: usize| u16::from_le_bytes([wav[i], wav[i + 1]]);
        let u32_at = |i: usize| u32::from_le_bytes([wav[i], wav[i + 1], wav[i + 2], wav[i + 3]]);
        let (mut i, mut rate, mut channels, mut samples) = (12, 0, 1, Vec::new());
        while i + 8 <= wav.len() {
            let (id, len) = (&wav[i..i + 4], u32_at(i + 4) as usize);
            if id == b"fmt " {
                channels = u16_at(i + 10);
                rate = u32_at(i + 12);
            } else if id == b"data" {
                let end = (i + 8 + len).min(wav.len());
                samples = wav[i + 8..end].as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b) as f32 / 32768.0).collect();
            }
            i += 8 + len + (len & 1);
        }
        let started = std::time::Instant::now();
        let text = transcribe("base", &to_whisper_input(&samples, rate, channels), "auto").unwrap();
        println!("{rate} Hz × {channels}: {text:?} in {:?}", started.elapsed());
        assert!(text.to_lowercase().contains("cache"), "{text}");
    }
}
