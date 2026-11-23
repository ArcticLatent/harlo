//! Dictation's speech models and text: downloads from a local web server
//! into scratch folders, Whisper's output cleaned up, recordings turned
//! into 16 kHz mono, and (when given a model) real speech transcribed.

// Rust guideline compliant 2026-10-09

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use harlo_core::voice::{
    Download, Model, Models, SAMPLE_RATE, Transcriber, to_whisper_input, transcript_text,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// What the test server answers.
#[derive(Clone)]
enum Answer {
    /// The file, in full.
    File(Vec<u8>),
    /// A promise of `n` bytes, then fewer.
    Short(usize, Vec<u8>),
    /// Not found.
    Missing,
}

/// Serves `answer` to every request on a local port; returns its address.
async fn serve(answer: Answer) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = format!("http://{}", listener.local_addr().expect("its address"));
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let answer = answer.clone();
            tokio::spawn(async move {
                let mut request = [0_u8; 4096];
                let _ = socket.read(&mut request).await;
                let (head, body) = match answer {
                    Answer::File(body) => (
                        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len()),
                        body,
                    ),
                    Answer::Short(promised, body) => (
                        format!("HTTP/1.1 200 OK\r\nContent-Length: {promised}\r\n\r\n"),
                        body,
                    ),
                    Answer::Missing => (
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_owned(),
                        Vec::new(),
                    ),
                };
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(&body).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    address
}

/// Every step of downloading `model` into `models`.
async fn download(models: &Models, model: Model) -> Vec<Download> {
    models.download(model).collect().await
}

#[tokio::test]
async fn a_model_downloads_with_progress_and_only_then_is_ready() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let body = vec![7_u8; 300_000];
    let models = Models::new(
        scratch.path().join("whisper"),
        &serve(Answer::File(body)).await,
    );
    assert!(!models.ready(Model::Base));

    let steps = download(&models, Model::Base).await;
    assert_eq!(steps.last(), Some(&Download::Done(Ok(()))));
    let fractions: Vec<f64> = steps
        .iter()
        .filter_map(|s| match s {
            Download::Progress(f) => Some(*f),
            Download::Done(_) => None,
        })
        .collect();
    assert!(fractions.len() > 2, "{fractions:?}");
    assert!(fractions.windows(2).all(|w| w[0] <= w[1]), "{fractions:?}");
    assert!(models.ready(Model::Base));
    assert!(!models.ready(Model::Small));
    let saved = std::fs::metadata(models.path(Model::Base)).expect("the file");
    assert_eq!(saved.len(), 300_000);
    assert!(!models.path(Model::Base).with_extension("part").exists());
}

#[tokio::test]
async fn a_broken_download_leaves_nothing_behind() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let short = serve(Answer::Short(500_000, vec![1_u8; 100_000])).await;
    let models = Models::new(scratch.path(), &short);
    let steps = download(&models, Model::Small).await;
    assert!(
        matches!(steps.last(), Some(Download::Done(Err(_)))),
        "{:?}",
        steps.last()
    );
    assert!(!models.ready(Model::Small));
    assert!(!models.path(Model::Small).with_extension("part").exists());

    let missing = Models::new(scratch.path(), &serve(Answer::Missing).await);
    let steps = download(&missing, Model::Base).await;
    let Some(Download::Done(Err(why))) = steps.last() else {
        panic!("a failure, not {steps:?}");
    };
    assert!(why.contains("404"), "{why}");
}

#[test]
fn models_have_names_and_files() {
    assert_eq!(Model::named("small"), Some(Model::Small));
    assert_eq!(Model::named("huge"), None);
    let models = Models::new("/x/whisper", "https://example.com/");
    assert_eq!(
        models.path(Model::Base),
        PathBuf::from("/x/whisper/ggml-base.bin")
    );
}

#[test]
fn whisper_markers_are_left_out() {
    assert_eq!(
        transcript_text(" Add a cache to the loader.\n And write tests (unit ones).\n"),
        "Add a cache to the loader. And write tests (unit ones)."
    );
    assert_eq!(transcript_text("[BLANK_AUDIO]\n"), "");
    assert_eq!(
        transcript_text(" (music) Hello [MUSIC] there\n"),
        "Hello there"
    );
    assert_eq!(transcript_text(""), "");
}

#[test]
fn recordings_become_16k_mono() {
    // 48 kHz stereo, one second: 16 000 samples, channels averaged.
    let stereo: Vec<f32> = (0..48_000).flat_map(|_| [0.2_f32, 0.4]).collect();
    let out = to_whisper_input(&stereo, 48_000, 2);
    assert_eq!(out.len(), 16_000);
    assert!(out.iter().all(|s| (s - 0.3).abs() < 1e-6));
    assert_eq!(to_whisper_input(&[0.1, 0.2], 16_000, 1), vec![0.1, 0.2]);
    // A ramp keeps its shape.
    let ramp: Vec<f32> = (0..44_100_u16).map(|i| f32::from(i) / 44_100.0).collect();
    let out = to_whisper_input(&ramp, 44_100, 1);
    assert_eq!(out.len(), 16_000);
    assert!((out[8_000] - 0.5).abs() < 0.001);
}

#[test]
fn too_little_sound_is_refused_before_loading() {
    let mut transcriber = Transcriber::new();
    let why = transcriber
        .transcribe(Path::new("/nowhere/ggml-base.bin"), &[0.0; 100], "auto")
        .expect_err("too short");
    assert!(why.contains("Nothing was recorded"), "{why}");
    let why = transcriber
        .transcribe(
            Path::new("/nowhere/ggml-base.bin"),
            &vec![0.0; SAMPLE_RATE as usize],
            "auto",
        )
        .expect_err("no model");
    assert!(why.contains("Couldn't load"), "{why}");
}

/// The samples of a 16-bit PCM WAV file, with its rate and channels.
fn read_wav(path: &Path) -> (Vec<f32>, u32, u16) {
    let wav = std::fs::read(path).expect("the recording");
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
            samples = wav[i + 8..end]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| f32::from(i16::from_le_bytes(*b)) / 32_768.0)
                .collect();
        }
        i += 8 + len + (len & 1);
    }
    (samples, rate, channels)
}

/// Downloads a real model from a local server and transcribes real speech.
///
/// `HARLO_TEST_WHISPER=<folder with ggml-base.bin and speech.wav saying
/// "Add a cache to the loader…">`, then
/// `cargo test -p harlo-core --release --test voice -- --ignored`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a Whisper model and a recording, given by HARLO_TEST_WHISPER"]
async fn real_speech_is_transcribed() {
    let fixture = PathBuf::from(std::env::var("HARLO_TEST_WHISPER").expect("HARLO_TEST_WHISPER"));
    let model = std::fs::read(fixture.join("ggml-base.bin")).expect("the model");
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let models = Models::new(scratch.path(), &serve(Answer::File(model)).await);
    let steps = download(&models, Model::Base).await;
    assert_eq!(steps.last(), Some(&Download::Done(Ok(()))));

    let (samples, rate, channels) = read_wav(&fixture.join("speech.wav"));
    let input = to_whisper_input(&samples, rate, channels);
    let path = models.path(Model::Base);
    let text = tokio::task::spawn_blocking(move || {
        let mut transcriber = Transcriber::new();
        let first = transcriber.transcribe(&path, &input, "en");
        // The loaded model is kept for the next one.
        let again = transcriber.transcribe(&path, &input, "auto");
        (first, again)
    })
    .await
    .expect("the transcription");
    let (first, again) = (text.0.expect("text"), text.1.expect("text"));
    assert!(first.to_lowercase().contains("cache"), "{first}");
    assert!(again.to_lowercase().contains("loader"), "{again}");
}
