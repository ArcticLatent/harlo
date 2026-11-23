//! Recording the microphone for dictation, with cpal: the default input
//! device, on a thread of its own, until it's stopped.

// Rust guideline compliant 2026-10-09

use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SizedSample, StreamConfig};

/// What a recording heard.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Captured {
    /// The samples, channels interleaved.
    pub samples: Vec<f32>,
    /// Samples a second.
    pub rate: u32,
    /// How many channels.
    pub channels: u16,
}

/// A recording under way; dropping it stops and forgets it.
#[derive(Debug)]
pub struct Recording {
    /// Ends the recording thread: true keeps what it heard.
    stop: mpsc::Sender<bool>,
    /// What it heard, once it ends.
    heard: mpsc::Receiver<Captured>,
}

impl Recording {
    /// Starts recording the default microphone.
    ///
    /// # Errors
    ///
    /// When there's no microphone or it can't be opened.
    pub fn start() -> Result<Self, String> {
        let (stop, stopped) = mpsc::channel::<bool>();
        let (heard_tx, heard) = mpsc::channel();
        let (started_tx, started) = mpsc::channel();
        std::thread::Builder::new()
            .name("harlo-microphone".to_owned())
            .spawn(move || {
                let samples = Arc::new(Mutex::new(Vec::new()));
                let opened = open(Arc::clone(&samples));
                let (stream, rate, channels) = match opened {
                    Ok(opened) => opened,
                    Err(why) => {
                        let _ = started_tx.send(Err(why));
                        return;
                    }
                };
                let _ = started_tx.send(Ok(()));
                // Records until told to stop, or until the recording's gone.
                let keep = stopped.recv().unwrap_or(false);
                drop(stream);
                if keep {
                    let samples = std::mem::take(
                        &mut *samples.lock().unwrap_or_else(PoisonError::into_inner),
                    );
                    let _ = heard_tx.send(Captured {
                        samples,
                        rate,
                        channels,
                    });
                }
            })
            .map_err(|e| format!("Couldn't start recording: {e}"))?;
        started
            .recv()
            .map_err(|_gone| "The microphone stopped.".to_owned())??;
        Ok(Self { stop, heard })
    }

    /// Stops and gives what was heard; waits for the device to close.
    ///
    /// # Errors
    ///
    /// When the recording thread ended without it.
    pub fn finish(self) -> Result<Captured, String> {
        let _ = self.stop.send(true);
        self.heard
            .recv()
            .map_err(|_gone| "The microphone stopped.".to_owned())
    }
}

/// Opens the default microphone, adding what it hears to `samples`; gives
/// the stream with its rate and channels.
fn open(samples: Arc<Mutex<Vec<f32>>>) -> Result<(cpal::Stream, u32, u16), String> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or("There's no microphone.")?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("Couldn't open the microphone: {e}"))?;
    let config = supported.config();
    let (rate, channels) = (config.sample_rate, config.channels);
    let stream = match supported.sample_format() {
        SampleFormat::F32 => input::<f32>(&device, config, samples, |s| s),
        SampleFormat::I16 => input::<i16>(&device, config, samples, |s| f32::from(s) / 32_768.0),
        SampleFormat::U16 => input::<u16>(&device, config, samples, |s| {
            (f32::from(s) - 32_768.0) / 32_768.0
        }),
        #[expect(
            clippy::cast_precision_loss,
            reason = "a sample's loudness; 24 bits of it are plenty"
        )]
        SampleFormat::I32 => input::<i32>(&device, config, samples, |s| s as f32 / 2_147_483_648.0),
        other => {
            return Err(format!(
                "The microphone gives {other} samples, which Harlo can't read."
            ));
        }
    }?;
    stream
        .play()
        .map_err(|e| format!("Couldn't start the microphone: {e}"))?;
    Ok((stream, rate, channels))
}

/// An input stream of `T` samples, each turned into a float by `float`.
fn input<T: SizedSample + Copy + Send + 'static>(
    device: &cpal::Device,
    config: StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    float: fn(T) -> f32,
) -> Result<cpal::Stream, String> {
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                samples
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .extend(data.iter().map(|&s| float(s)));
            },
            // A broken stream records nothing more; what it has stays.
            |_| {},
            None,
        )
        .map_err(|e| format!("Couldn't open the microphone: {e}"))
}
