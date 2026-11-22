//! Terminals whose shells run on the server, for servers on other computers.
//!
//! [`follow`] connects to a shell the server started, and turns it into a
//! stream of output for the terminal on screen, while sending the user's
//! input and the terminal's size the other way.

// Rust guideline compliant 2026-10-09

use std::pin::Pin;

use futures_util::stream::{self, BoxStream};
use futures_util::{Stream, StreamExt};

use crate::server::{Client, Error, PtyInput, PtyOutput};

/// What the terminal on screen sends toward the shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// What the user typed or pasted.
    Write(Vec<u8>),
    /// The terminal's size, in cells.
    Resize {
        /// Columns.
        columns: u16,
        /// Lines.
        lines: u16,
    },
}

/// What happened to the shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    /// It printed something.
    Output(Vec<u8>),
    /// It ended, or the connection to it did; with the reason when that was
    /// an error.
    Ended(Option<String>),
}

/// One step of the connection: output, its end, or input to send.
enum Step {
    Output(Result<Vec<u8>, Error>),
    Closed,
    Input(Input),
}

/// Where a connection is.
enum Phase {
    /// Not connected yet.
    Connecting(Pin<Box<dyn Stream<Item = Input> + Send>>),
    /// Connected: output and input interleaved, and where input goes.
    Running {
        steps: BoxStream<'static, Step>,
        input: PtyInput,
        size: Option<(u16, u16)>,
    },
    /// Over.
    Done,
}

/// Connects to the server's shell `pty` in `directory`'s project, and
/// streams its output, starting with what it printed before; `input` goes
/// the other way. The stream ends after [`Update::Ended`].
///
/// A run of size changes sends only the sizes that differ from the last one.
pub fn follow(
    client: Client,
    directory: String,
    pty: String,
    input: impl Stream<Item = Input> + Send + 'static,
) -> impl Stream<Item = Update> + Send {
    stream::unfold(Phase::Connecting(Box::pin(input)), move |phase| {
        let client = client.clone();
        let directory = directory.clone();
        let pty = pty.clone();
        async move { advance(phase, &client, &directory, &pty).await }
    })
}

/// Runs the connection until it has something to report.
async fn advance(
    phase: Phase,
    client: &Client,
    directory: &str,
    pty: &str,
) -> Option<(Update, Phase)> {
    let (mut steps, mut input, mut size) = match phase {
        Phase::Done => return None,
        Phase::Connecting(typed) => match client.connect_pty(directory, pty).await {
            Ok((output, input)) => (interleave(output, typed), input, None),
            Err(error) => return Some((Update::Ended(Some(error.to_string())), Phase::Done)),
        },
        Phase::Running { steps, input, size } => (steps, input, size),
    };
    loop {
        match steps.next().await {
            Some(Step::Output(Ok(bytes))) => {
                return Some((Update::Output(bytes), Phase::Running { steps, input, size }));
            }
            Some(Step::Output(Err(error))) => {
                return Some((Update::Ended(Some(error.to_string())), Phase::Done));
            }
            Some(Step::Closed) | None => return Some((Update::Ended(None), Phase::Done)),
            Some(Step::Input(Input::Write(bytes))) => {
                if let Err(error) = input.send(&bytes).await {
                    return Some((Update::Ended(Some(error.to_string())), Phase::Done));
                }
            }
            Some(Step::Input(Input::Resize { columns, lines })) => {
                if size != Some((columns, lines)) && columns > 0 && lines > 0 {
                    size = Some((columns, lines));
                    // A size the server didn't take leaves the old one; the
                    // shell still works.
                    let _ = client.resize_pty(directory, pty, columns, lines).await;
                }
            }
        }
    }
}

/// The shell's output, then a mark that it closed, interleaved with input.
fn interleave(
    output: PtyOutput,
    typed: Pin<Box<dyn Stream<Item = Input> + Send>>,
) -> BoxStream<'static, Step> {
    let output = stream::unfold(output, |mut output| async move {
        output.next().await.map(|item| (Step::Output(item), output))
    })
    .chain(stream::once(async { Step::Closed }));
    stream::select(output, typed.map(Step::Input)).boxed()
}
