//! Terminals the server runs for its clients: a shell on a pseudo-terminal
//! in a folder, its output streamed over a WebSocket that also takes what's
//! typed. A terminal outlives its connections, and a new connection starts
//! with what it printed before, until it's removed or its shell ends.

// Rust guideline compliant 2026-10-09

use std::io::{Read as _, Write as _};
use std::sync::{Arc, Mutex, mpsc};

use axum::extract::ws::{Message as Frame, WebSocket};
use futures_util::{SinkExt as _, StreamExt as _};
use harlo_engine::ascending_id;
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde_json::{Value, json};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::server::{Refusal, Server, lock};

/// The most of a terminal's output kept for new connections: its end.
const KEPT: usize = 2 * 1024 * 1024;

/// How many chunks of output a slow connection may fall behind.
const BACKLOG: usize = 1024;

/// A terminal the server runs.
pub(crate) struct Terminal {
    /// What clients see of it.
    info: Mutex<Value>,
    /// What's typed, for the thread writing it to the shell.
    input: mpsc::Sender<Vec<u8>>,
    /// The terminal's controlling side, for its size.
    master: Mutex<Box<dyn MasterPty + Send>>,
    /// Stops the shell.
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    /// The end of what it printed.
    shown: Mutex<Vec<u8>>,
    /// What it prints from now on.
    output: broadcast::Sender<Vec<u8>>,
    /// Cancelled when its shell has ended.
    ended: CancellationToken,
}

/// A terminal to start.
#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct NewTerminal {
    cwd: Option<String>,
    command: Option<String>,
    args: Option<Vec<String>>,
    title: Option<String>,
}

/// A change to a terminal: its size or title.
#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct TerminalChange {
    size: Option<Size>,
    title: Option<String>,
}

/// A terminal's size in characters.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
struct Size {
    cols: u16,
    rows: u16,
}

/// Keeps the end of `kept` after adding `more`, at most [`KEPT`] bytes.
fn keep(kept: &mut Vec<u8>, more: &[u8]) {
    kept.extend_from_slice(more);
    if kept.len() > KEPT {
        let cut = kept.len() - KEPT;
        kept.drain(..cut);
    }
}

impl Server {
    /// Starts a terminal in `directory` (or the `cwd` asked for), running
    /// the shell asked for or the person's own.
    ///
    /// # Errors
    ///
    /// When the folder isn't there or the shell can't start.
    pub(crate) fn create_pty(
        self: &Arc<Self>,
        directory: &str,
        asked: NewTerminal,
    ) -> Result<Value, Refusal> {
        let cwd = asked.cwd.unwrap_or_else(|| directory.to_owned());
        if !std::path::Path::new(&cwd).is_dir() {
            return Err(Refusal::Bad(format!("{cwd} isn't a folder.")));
        }
        let command = asked
            .command
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| "/bin/sh".to_owned());
        // A shell starts as a login shell, as a terminal window's does.
        let args = asked.args.unwrap_or_else(|| {
            if command.ends_with("sh") {
                vec!["-l".to_owned()]
            } else {
                Vec::new()
            }
        });
        let refused =
            |e: &dyn std::fmt::Display| Refusal::Bad(format!("Couldn't start {command}: {e}"));
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| refused(&e))?;
        let mut builder = CommandBuilder::new(&command);
        builder.args(&args);
        builder.cwd(&cwd);
        builder.env("TERM", "xterm-256color");
        let child = pair.slave.spawn_command(builder).map_err(|e| refused(&e))?;
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().map_err(|e| refused(&e))?;
        let writer = pair.master.take_writer().map_err(|e| refused(&e))?;
        let id = ascending_id("pty");
        let tail: String = id
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let info = json!({
            "id": id, "title": asked.title.unwrap_or_else(|| format!("Terminal {tail}")),
            "command": command, "args": args, "cwd": cwd, "status": "running",
            "pid": child.process_id().unwrap_or_default(),
        });
        let (input, typed) = mpsc::channel::<Vec<u8>>();
        let terminal = Arc::new(Terminal {
            info: Mutex::new(info.clone()),
            input,
            master: Mutex::new(pair.master),
            killer: Mutex::new(child.clone_killer()),
            shown: Mutex::new(Vec::new()),
            output: broadcast::channel(BACKLOG).0,
            ended: CancellationToken::new(),
        });
        std::thread::spawn(move || {
            let mut writer = writer;
            for bytes in typed {
                if writer
                    .write_all(&bytes)
                    .and_then(|()| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        let (server, term) = (Arc::clone(self), Arc::clone(&terminal));
        std::thread::spawn(move || server.pump(&term, reader, child));
        lock(&self.live).ptys.insert(id, terminal);
        self.emit(
            "pty.created",
            json!({ "info": info }),
            Some(directory),
            None,
        );
        Ok(info)
    }

    /// Passes terminal `term`'s output on until its shell ends, then says
    /// how it ended.
    fn pump(
        &self,
        term: &Terminal,
        mut reader: Box<dyn std::io::Read + Send>,
        mut child: Box<dyn portable_pty::Child + Send + Sync>,
    ) {
        let mut buf = vec![0; 16 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    // Kept and sent under one lock, so a new connection
                    // gets each byte once.
                    let mut shown = lock(&term.shown);
                    keep(&mut shown, &buf[..n]);
                    let _ = term.output.send(buf[..n].to_vec());
                }
            }
        }
        let code = child.wait().map_or(0, |s| s.exit_code());
        let (id, info) = {
            let mut info = lock(&term.info);
            info["status"] = json!("exited");
            info["exitCode"] = json!(code);
            (
                info["id"].as_str().unwrap_or_default().to_owned(),
                info.clone(),
            )
        };
        term.ended.cancel();
        self.emit(
            "pty.exited",
            json!({ "id": id, "exitCode": code }),
            None,
            None,
        );
        self.emit("pty.updated", json!({ "info": info }), None, None);
    }

    /// Terminal `id`.
    ///
    /// # Errors
    ///
    /// When there's no such terminal.
    pub(crate) fn pty(&self, id: &str) -> Result<Arc<Terminal>, Refusal> {
        lock(&self.live)
            .ptys
            .get(id)
            .cloned()
            .ok_or_else(|| Refusal::NotFound(format!("There's no terminal {id}.")))
    }

    /// The terminals, as clients see them.
    pub(crate) fn ptys(&self) -> Vec<Value> {
        let mut list: Vec<Value> = lock(&self.live)
            .ptys
            .values()
            .map(|t| lock(&t.info).clone())
            .collect();
        list.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        list
    }

    /// Resizes or renames terminal `id`; returns it.
    ///
    /// # Errors
    ///
    /// When there's no such terminal, or it can't be resized.
    pub(crate) fn change_pty(&self, id: &str, change: TerminalChange) -> Result<Value, Refusal> {
        let term = self.pty(id)?;
        if let Some(size) = change.size {
            lock(&term.master)
                .resize(PtySize {
                    rows: size.rows,
                    cols: size.cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| Refusal::Bad(format!("Couldn't resize it: {e}")))?;
        }
        let info = {
            let mut info = lock(&term.info);
            if let Some(title) = change.title {
                info["title"] = json!(title);
            }
            info.clone()
        };
        self.emit("pty.updated", json!({ "info": info }), None, None);
        Ok(info)
    }

    /// Stops terminal `id`'s shell and forgets it.
    ///
    /// # Errors
    ///
    /// When there's no such terminal.
    pub(crate) fn remove_pty(&self, id: &str) -> Result<(), Refusal> {
        let term = lock(&self.live)
            .ptys
            .remove(id)
            .ok_or_else(|| Refusal::NotFound(format!("There's no terminal {id}.")))?;
        let _ = lock(&term.killer).kill();
        term.ended.cancel();
        self.emit("pty.deleted", json!({ "id": id }), None, None);
        Ok(())
    }
}

/// Serves one connection to terminal `term`: what it printed before, then
/// its output as it comes, and what's typed to its shell.
pub(crate) async fn serve(term: Arc<Terminal>, socket: WebSocket) {
    let (mut out, mut typed) = socket.split();
    let (before, mut output) = {
        let shown = lock(&term.shown);
        (shown.clone(), term.output.subscribe())
    };
    if !before.is_empty() && out.send(Frame::Binary(before.into())).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            printed = output.recv() => match printed {
                Ok(bytes) => {
                    if out.send(Frame::Binary(bytes.into())).await.is_err() {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => break,
            },
            frame = typed.next() => match frame {
                Some(Ok(Frame::Binary(bytes))) => {
                    let _ = term.input.send(bytes.to_vec());
                }
                Some(Ok(Frame::Text(text))) => {
                    let _ = term.input.send(text.as_bytes().to_vec());
                }
                Some(Ok(Frame::Close(_)) | Err(_)) | None => return,
                Some(Ok(_)) => {}
            },
            () = term.ended.cancelled() => break,
        }
    }
    let _ = out.send(Frame::Close(None)).await;
}
