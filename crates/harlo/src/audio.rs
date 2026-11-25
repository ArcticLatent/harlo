//! Recording a microphone for dictation, with cpal: the chosen input
//! device, or the system's default, on a thread of its own until it's
//! stopped; and the list of microphones to choose from.

// Rust guideline compliant 2026-10-09

use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{DeviceId, FromSample, I24, Sample, SampleFormat, SizedSample, StreamConfig, U24};

/// How the sound system's names for what the speakers play end. They are
/// listed with the microphones, and nobody dictates into one.
const MONITOR: &str = ".monitor";

/// A microphone this computer has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Microphone {
    /// What the sound system calls it; the preferences keep this.
    pub id: String,
    /// Its name, as the desktop's sound settings show it.
    pub name: String,
}

/// The microphones to choose from, and which one the system prefers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Microphones {
    /// Every microphone the sound system lists.
    pub all: Vec<Microphone>,
    /// The name of the system's default one, when it has one.
    pub default: Option<String>,
}

impl Microphones {
    /// Asks the sound system for its microphones.
    ///
    /// Talks to the sound server, so call it off the UI's thread. A
    /// system without sound has none.
    #[must_use]
    pub fn list() -> Self {
        let host = cpal::default_host();
        let default = host.default_input_device().and_then(|d| name(&d));
        let all = host
            .input_devices()
            .into_iter()
            .flatten()
            .filter_map(|device| {
                let id = device.id().ok()?;
                (!id.id().ends_with(MONITOR)).then_some(())?;
                Some(Microphone {
                    id: id.to_string(),
                    name: name(&device)?,
                })
            })
            .collect();
        Self { all, default }
    }
}

/// The device's name, as the desktop shows it.
fn name(device: &cpal::Device) -> Option<String> {
    Some(device.description().ok()?.name().to_owned())
}

/// What a recording heard.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Captured {
    /// The samples, channels interleaved.
    pub samples: Vec<f32>,
    /// Samples a second.
    pub rate: u32,
    /// How many channels.
    pub channels: u16,
    /// The name of the microphone it came from.
    pub microphone: String,
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
    /// Starts recording the microphone `chosen` (a [`Microphone::id`]), or
    /// the system's default one when it's empty.
    ///
    /// # Errors
    ///
    /// When there's no microphone, the chosen one isn't connected, or it
    /// can't be opened.
    pub fn start(chosen: &str) -> Result<Self, String> {
        let chosen = chosen.to_owned();
        let (stop, stopped) = mpsc::channel::<bool>();
        let (heard_tx, heard) = mpsc::channel();
        let (started_tx, started) = mpsc::channel();
        std::thread::Builder::new()
            .name("harlo-microphone".to_owned())
            .spawn(move || {
                let samples = Arc::new(Mutex::new(Vec::new()));
                let opened = open(Arc::clone(&samples), &chosen);
                let (stream, rate, channels, microphone) = match opened {
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
                        microphone,
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

/// What opening a microphone gives: the stream, its rate and channels, and
/// the microphone's name.
type Opened = (cpal::Stream, u32, u16, String);

/// Opens the microphone `chosen`, or the default one when it's empty,
/// adding what it hears to `samples`.
fn open(samples: Arc<Mutex<Vec<f32>>>, chosen: &str) -> Result<Opened, String> {
    let host = cpal::default_host();
    let device = if chosen.is_empty() {
        host.default_input_device()
            .ok_or("There's no microphone.")?
    } else {
        chosen
            .parse::<DeviceId>()
            .ok()
            .and_then(|id| host.device_by_id(&id))
            .ok_or("The chosen microphone isn't connected. Choose another in Settings → Voice.")?
    };
    let supported = device
        .default_input_config()
        .map_err(|e| format!("Couldn't open the microphone: {e}"))?;
    let config = supported.config();
    let (rate, channels) = (config.sample_rate, config.channels);
    let stream = match supported.sample_format() {
        SampleFormat::F32 => input::<f32>(&device, config, samples),
        SampleFormat::F64 => input::<f64>(&device, config, samples),
        SampleFormat::I8 => input::<i8>(&device, config, samples),
        SampleFormat::I16 => input::<i16>(&device, config, samples),
        SampleFormat::I24 => input::<I24>(&device, config, samples),
        SampleFormat::I32 => input::<i32>(&device, config, samples),
        SampleFormat::I64 => input::<i64>(&device, config, samples),
        SampleFormat::U8 => input::<u8>(&device, config, samples),
        SampleFormat::U16 => input::<u16>(&device, config, samples),
        SampleFormat::U24 => input::<U24>(&device, config, samples),
        SampleFormat::U32 => input::<u32>(&device, config, samples),
        SampleFormat::U64 => input::<u64>(&device, config, samples),
        other => {
            return Err(format!(
                "The microphone gives {other} samples, which Harlo can't read."
            ));
        }
    }?;
    stream
        .play()
        .map_err(|e| format!("Couldn't start the microphone: {e}"))?;
    Ok((stream, rate, channels, name(&device).unwrap_or_default()))
}

/// An input stream of `T` samples, each turned into a float.
fn input<T>(
    device: &cpal::Device,
    config: StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                samples
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .extend(data.iter().map(|&s| f32::from_sample(s)));
            },
            // A broken stream records nothing more; what it has stays.
            |_| {},
            None,
        )
        .map_err(|e| format!("Couldn't open the microphone: {e}"))
}
