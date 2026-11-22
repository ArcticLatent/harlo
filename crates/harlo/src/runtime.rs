//! Runs tasks: their work on a background thread pool, and what it produces
//! handed back to the window's thread.

// Rust guideline compliant 2026-10-09

use futures_util::StreamExt as _;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::task::{Action, Task};

/// Runs tasks' work in the background; what it produces arrives on the
/// receiver [`Runtime::new`] returns.
#[derive(Debug)]
pub struct Runtime<M> {
    pool: tokio::runtime::Runtime,
    sender: UnboundedSender<Action<M>>,
}

impl<M: Send + 'static> Runtime<M> {
    /// A runtime, and where what its tasks produce arrives.
    ///
    /// # Errors
    ///
    /// Returns an error if the thread pool can't start.
    pub fn new() -> std::io::Result<(Self, UnboundedReceiver<Action<M>>)> {
        let pool = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("harlo-work")
            .build()?;
        let (sender, receiver) = mpsc::unbounded_channel();
        Ok((Self { pool, sender }, receiver))
    }

    /// Starts `task`'s work; each piece runs on its own until it ends or the
    /// window goes.
    pub fn spawn(&self, task: Task<M>) {
        for mut unit in task.into_units() {
            let sender = self.sender.clone();
            self.pool.spawn(async move {
                while let Some(action) = unit.next().await {
                    if sender.send(action).is_err() {
                        break;
                    }
                }
            });
        }
    }

    /// A handle to the thread pool, for work that must run inside it.
    #[must_use]
    pub fn handle(&self) -> tokio::runtime::Handle {
        self.pool.handle().clone()
    }
}
