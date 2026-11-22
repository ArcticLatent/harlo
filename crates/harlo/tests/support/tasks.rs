//! Runs the tasks a test window's app starts, feeding their messages back,
//! as the real window's runtime would.

// Rust guideline compliant 2026-10-09

use std::time::{Duration, Instant};

use futures_util::StreamExt as _;
use futures_util::stream::{BoxStream, SelectAll};
use harlo::app::Message;
use harlo::task::Action;

use super::Ui;

/// How long a step waits for a task to say something before checking again.
const STEP: Duration = Duration::from_millis(50);

/// The tasks running for one window.
pub struct Tasks {
    runtime: tokio::runtime::Runtime,
    running: SelectAll<BoxStream<'static, Action<Message>>>,
}

impl Tasks {
    /// A pool to run tasks on.
    ///
    /// # Panics
    ///
    /// Panics if the pool can't start.
    pub fn new() -> Self {
        Self {
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("a pool for the tasks"),
            running: SelectAll::new(),
        }
    }

    /// Runs `future` to its end on the pool.
    pub fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        self.runtime.block_on(future)
    }

    /// Runs `ui`'s tasks until `done` holds or `within` passes; whether it
    /// held.
    pub fn until(&mut self, ui: &Ui, within: Duration, done: impl Fn(&Ui) -> bool) -> bool {
        let deadline = Instant::now() + within;
        loop {
            for task in ui.driver.take_tasks() {
                if let Some(stream) = task.into_stream() {
                    self.running.push(stream);
                }
            }
            // The window's timers report its terminal's size.
            slint::platform::update_timers_and_animations();
            if done(ui) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            if self.running.is_empty() {
                std::thread::sleep(STEP);
                continue;
            }
            let running = &mut self.running;
            let next = self
                .runtime
                .block_on(async { tokio::time::timeout(STEP, running.next()).await });
            if let Ok(Some(Action::Output(message))) = next {
                ui.send(message);
            }
        }
    }
}
