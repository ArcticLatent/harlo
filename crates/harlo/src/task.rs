//! Work an update starts: background requests whose answers come back as
//! messages, and changes to the window such as moving the focus.
//!
//! [`App::update`](crate::App::update) returns a [`Task`]. The runtime runs
//! its streams and futures on a background thread pool and feeds what they
//! produce back into the app; tests run them with [`Task::into_stream`].

// Rust guideline compliant 2026-10-09

use std::sync::Arc;

use futures_util::future::{AbortHandle, Abortable};
use futures_util::stream::{self, BoxStream};
use futures_util::{FutureExt as _, Stream, StreamExt as _};

/// A change to the window that only the window can make.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    /// Give the keyboard focus to the field with this name.
    Focus(&'static str),
    /// Scroll the list with this name to its end, and follow what's added.
    ToEnd(&'static str),
    /// Scroll the list with this name so its marked row (the lit hunk, the
    /// selected file) shows.
    Reveal(&'static str),
    /// Scroll the list with this name so this far from its top is at the top.
    ScrollTo {
        /// The list.
        list: &'static str,
        /// How far from its top, in logical pixels.
        y: f32,
    },
}

/// What a task produces: a message for the app, or an effect on the window.
#[derive(Debug, Clone, PartialEq)]
pub enum Action<M> {
    /// A message for the app's update.
    Output(M),
    /// A change to the window.
    Effect(Effect),
}

impl<M> Action<M> {
    /// Changes the message, if this is one.
    fn map<N>(self, f: &(impl Fn(M) -> N + ?Sized)) -> Action<N> {
        match self {
            Self::Output(message) => Action::Output(f(message)),
            Self::Effect(effect) => Action::Effect(effect),
        }
    }
}

/// Work started by an update, producing messages and effects.
pub struct Task<M> {
    /// Each piece of work: a stream of what it produces.
    units: Vec<BoxStream<'static, Action<M>>>,
}

impl<M> std::fmt::Debug for Task<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Task")
            .field("units", &self.units.len())
            .finish()
    }
}

impl<M: Send + 'static> Task<M> {
    /// No work.
    #[must_use]
    pub fn none() -> Self {
        Self { units: Vec::new() }
    }

    /// Produces `message` straight away.
    #[must_use]
    pub fn done(message: M) -> Self {
        Self::stream(stream::once(async move { message }))
    }

    /// Changes the window.
    #[must_use]
    pub fn effect(effect: Effect) -> Self {
        Self {
            units: vec![stream::once(async move { Action::Effect(effect) }).boxed()],
        }
    }

    /// Runs `future` and produces its output.
    pub fn future(future: impl Future<Output = M> + Send + 'static) -> Self {
        Self::stream(future.into_stream())
    }

    /// Runs `future` and produces `f` of its output.
    pub fn perform<T>(
        future: impl Future<Output = T> + Send + 'static,
        f: impl FnOnce(T) -> M + Send + 'static,
    ) -> Self {
        Self::future(future.map(f))
    }

    /// Produces each item of `stream`.
    pub fn stream(stream: impl Stream<Item = M> + Send + 'static) -> Self {
        Self {
            units: vec![stream.map(Action::Output).boxed()],
        }
    }

    /// Produces `f` of each item of `stream`.
    pub fn run<T>(
        stream: impl Stream<Item = T> + Send + 'static,
        f: impl Fn(T) -> M + Send + 'static,
    ) -> Self {
        Self::stream(stream.map(f))
    }

    /// All of `tasks` at once.
    pub fn batch(tasks: impl IntoIterator<Item = Self>) -> Self {
        Self {
            units: tasks.into_iter().flat_map(|task| task.units).collect(),
        }
    }

    /// Changes each message the task produces with `f`.
    #[must_use]
    pub fn map<N: Send + 'static>(self, f: impl Fn(M) -> N + Send + Sync + 'static) -> Task<N> {
        let f: Arc<dyn Fn(M) -> N + Send + Sync> = Arc::new(f);
        Task {
            units: self
                .units
                .into_iter()
                .map(|unit| {
                    let f = Arc::clone(&f);
                    unit.map(move |action| action.map(&*f)).boxed()
                })
                .collect(),
        }
    }

    /// Runs the task for its effects on the world, dropping its messages.
    #[must_use]
    pub fn discard<N: Send + 'static>(self) -> Task<N> {
        Task {
            units: self
                .units
                .into_iter()
                .map(|unit| {
                    unit.filter_map(|action| async move {
                        match action {
                            Action::Output(_) => None,
                            Action::Effect(effect) => Some(Action::Effect(effect)),
                        }
                    })
                    .boxed()
                })
                .collect(),
        }
    }

    /// The task, and a handle that stops it.
    #[must_use]
    pub fn abortable(self) -> (Self, Handle) {
        let (abort, registration) = AbortHandle::new_pair();
        let units = stream::select_all(self.units);
        let task = Self {
            units: vec![Abortable::new(units, registration).boxed()],
        };
        (
            task,
            Handle {
                abort,
                on_drop: false,
            },
        )
    }

    /// Whether the task does nothing.
    #[must_use]
    pub fn is_none(&self) -> bool {
        self.units.is_empty()
    }

    /// Everything the task produces, in the order it comes; `None` when it
    /// does nothing.
    #[must_use]
    pub fn into_stream(self) -> Option<BoxStream<'static, Action<M>>> {
        (!self.units.is_empty()).then(|| stream::select_all(self.units).boxed())
    }

    /// Each piece of work, to run separately.
    pub(crate) fn into_units(self) -> Vec<BoxStream<'static, Action<M>>> {
        self.units
    }
}

/// Stops a task made [`Task::abortable`].
#[derive(Debug)]
pub struct Handle {
    abort: AbortHandle,
    on_drop: bool,
}

impl Handle {
    /// Stops the task.
    pub fn abort(&self) {
        self.abort.abort();
    }

    /// Makes dropping the handle stop the task.
    #[must_use]
    pub fn abort_on_drop(mut self) -> Self {
        self.on_drop = true;
        self
    }

    /// Whether the task was stopped.
    #[must_use]
    pub fn is_aborted(&self) -> bool {
        self.abort.is_aborted()
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if self.on_drop {
            self.abort.abort();
        }
    }
}
