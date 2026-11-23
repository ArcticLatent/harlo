//! Dictation: Settings → Voice downloads the speech model (from a local web
//! server, into a scratch folder), then the composer's microphone records
//! and puts the words in at the cursor. The microphone is scripted, so
//! nothing is recorded and whisper.cpp doesn't run here; `harlo-core`'s
//! tests run it on real speech.

// Rust guideline compliant 2026-10-09

mod support;

use harlo::app::{Listening, Message, SettingChoice, SheetEvent, VoiceEvent, shortcut};
use harlo::keys::KeyPress;
use harlo::text::TextBox;
use harlo::window::window_key;
use harlo_core::voice::Models;
use std::io::{Read, Write};
use std::time::Duration;
use support::tasks::Tasks;
use support::{Ui, demo, ui};

/// How long a step may take.
const WAIT: Duration = Duration::from_secs(20);

/// Serves `size` bytes to every request on a local port; gives its address.
fn serve(size: usize) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let address = format!("http://{}", listener.local_addr().expect("its address"));
    std::thread::spawn(move || {
        for mut socket in listener.incoming().map_while(Result::ok) {
            let mut request = [0_u8; 4096];
            let _ = socket.read(&mut request);
            let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {size}\r\n\r\n");
            let _ = socket.write_all(head.as_bytes());
            let _ = socket.write_all(&vec![5_u8; size]);
        }
    });
    address
}

/// The window, with speech models kept in `folder` and served locally, and
/// a microphone that hears `heard`.
fn window(folder: &std::path::Path, heard: &str, text: &str) -> Ui {
    let mut app = demo::app();
    app.voice.models = Some(Models::new(folder, &serve(200_000)));
    app.composer = TextBox::with_text(text);
    app.voice.heard = Some(heard.to_owned());
    ui(app)
}

/// Whether `message` is `event` for dictation.
fn is_voice(message: Option<Message>, event: &VoiceEvent) -> bool {
    matches!(message, Some(Message::Voice(e)) if e == *event)
}

#[test]
fn the_shortcut_and_escape_reach_dictation() {
    let press = KeyPress::new("d", true, true, false);
    assert!(is_voice(shortcut(&press), &VoiceEvent::Toggle));
    assert!(shortcut(&KeyPress::new("d", true, false, false)).is_none());

    let mut app = demo::app();
    let escape = KeyPress::new("\u{1b}", false, false, false);
    assert!(!is_voice(window_key(&app, &escape), &VoiceEvent::Cancel));
    app.voice.listening = Listening::Recording;
    assert!(is_voice(window_key(&app, &escape), &VoiceEvent::Cancel));
}

#[test]
fn dictation_needs_a_model_then_puts_the_words_at_the_cursor() {
    let mut tasks = Tasks::new();
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let ui = window(scratch.path(), "add a cache to the loader", "Please");
    assert!(
        ui.find("Dictate").is_empty(),
        "no microphone without a model"
    );

    // Without a model, the shortcut shows where to get one.
    ui.send(Message::Voice(VoiceEvent::Toggle));
    assert_eq!(ui.app().voice.listening, Listening::Idle);
    assert!(ui.shows("Not downloaded"));
    ui.press("Download 148 MB");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("Downloaded")),
        "{:?}",
        ui.app().voice.download_error
    );
    assert!(scratch.path().join("ggml-base.bin").exists());

    ui.send(Message::Sheet(SheetEvent::Close));
    ui.press("Dictate");
    assert_eq!(ui.app().voice.listening, Listening::Recording);
    assert!(ui.shows("Listening… Ctrl+Shift+D to stop, Esc to drop"));
    ui.press("Stop dictating");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.app().voice.listening == Listening::Idle),
        "transcribed"
    );
    assert_eq!(
        ui.app().composer.as_str(),
        "Please add a cache to the loader"
    );
}

#[test]
fn escape_drops_the_recording_and_silence_says_so() {
    let mut tasks = Tasks::new();
    let scratch = tempfile::tempdir().expect("a scratch folder");
    std::fs::write(scratch.path().join("ggml-base.bin"), b"model").expect("a model");
    let ui = window(scratch.path(), "", "");

    ui.send(Message::Voice(VoiceEvent::Toggle));
    ui.send(Message::Voice(VoiceEvent::Cancel));
    assert_eq!(ui.app().voice.listening, Listening::Idle);

    ui.press("Dictate");
    ui.press("Stop dictating");
    assert!(
        tasks.until(&ui, WAIT, |ui| ui.shows("No speech heard.")),
        "the empty recording"
    );
    assert_eq!(ui.app().composer.as_str(), "");
}

#[test]
fn the_model_and_language_are_saved_choices() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let ui = window(scratch.path(), "hello", "");
    ui.send(Message::Sheet(SheetEvent::Choose(
        SettingChoice::VoiceLanguage,
        1,
    )));
    ui.send(Message::Sheet(SheetEvent::Choose(
        SettingChoice::VoiceModel,
        1,
    )));
    assert_eq!(ui.app().preferences.voice_language, "en");
    assert_eq!(ui.app().preferences.voice_model, "small");
    ui.send(Message::Sheet(SheetEvent::Open(harlo::app::Section::Voice)));
    assert!(ui.shows("Download 488 MB"), "the small model's size");
}
