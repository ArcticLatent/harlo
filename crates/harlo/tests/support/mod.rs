//! Shared helpers for the UI tests: the real window on Slint's testing
//! backend, driven from an app connected to a demo snapshot.

// Each test binary uses some of these helpers; `expect` would fail in the
// ones that use them all.
#![allow(dead_code, reason = "shared by test binaries that each use a part")]

// Rust guideline compliant 2026-10-09

pub mod demo;
pub mod tasks;

use harlo::Driver;
use harlo::app::{App, Message};
use harlo::ui::AppWindow;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle as _, LogicalPosition};

/// The window size the design was drawn at.
pub const WINDOW: (f32, f32) = (1588.0, 947.0);

/// The window showing an app, headless.
pub struct Ui {
    pub window: AppWindow,
    pub driver: Driver,
}

/// Opens the window on the testing backend, showing `app`.
pub fn ui(app: App) -> Ui {
    ui_at(app, WINDOW)
}

/// Opens the window at `size`, showing `app`.
pub fn ui_at(app: App, size: (f32, f32)) -> Ui {
    // The backend is set up once per thread; a test may open several windows.
    thread_local! {
        static BACKEND: () = i_slint_backend_testing::init_no_event_loop();
    }
    BACKEND.with(|()| ());
    let window = AppWindow::new().expect("the window opens");
    let driver = Driver::new(&window, app, size);
    window.show().expect("the window shows");
    // Showing it reports its size and the composer's cursor.
    let _ = driver.take_messages();
    Ui { window, driver }
}

impl Ui {
    /// The app as it is now.
    pub fn app(&self) -> std::cell::Ref<'_, App> {
        self.driver.app()
    }

    /// Applies `message`, as if a task had sent it.
    pub fn send(&self, message: Message) {
        self.driver.send(message);
    }

    /// The messages the window sent since the last call.
    pub fn messages(&self) -> Vec<Message> {
        self.driver.take_messages()
    }

    /// Every element labelled `label`, such as a text showing it.
    pub fn find(&self, label: &str) -> Vec<ElementHandle> {
        ElementHandle::find_by_accessible_label(&self.window, label).collect()
    }

    /// Whether the window shows `text`.
    pub fn shows(&self, text: &str) -> bool {
        !self.find(text).is_empty()
    }

    /// The button labelled `label`.
    ///
    /// # Panics
    ///
    /// Panics if there's none.
    pub fn button(&self, label: &str) -> ElementHandle {
        self.find(label)
            .into_iter()
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .unwrap_or_else(|| panic!("no button labelled {label:?}"))
    }

    /// Presses the button labelled `label`.
    pub fn press(&self, label: &str) {
        self.button(label).invoke_accessible_default_action();
    }

    /// Types `text` into the field labelled `label`, replacing what's there.
    ///
    /// # Panics
    ///
    /// Panics if there's no such field.
    pub fn fill(&self, label: &str, text: &str) {
        let field = self
            .find(label)
            .into_iter()
            .find(|e| e.accessible_role() == Some(AccessibleRole::TextInput))
            .unwrap_or_else(|| panic!("no field labelled {label:?}"));
        field.set_accessible_value(text);
    }

    /// Drags the element labelled `label` by `by` with the left button, from
    /// its middle, and lets go.
    ///
    /// # Panics
    ///
    /// Panics if there's no such element.
    pub fn drag(&self, label: &str, by: (f32, f32)) {
        let handle = self
            .find(label)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("nothing labelled {label:?}"));
        let at = handle.absolute_position();
        let size = handle.size();
        let from = LogicalPosition::new(at.x + size.width / 2.0, at.y + size.height / 2.0);
        let to = LogicalPosition::new(from.x + by.0, from.y + by.1);
        let window = self.window.window();
        let button = PointerEventButton::Left;
        for event in [
            WindowEvent::PointerMoved { position: from },
            WindowEvent::PointerPressed {
                position: from,
                button,
            },
            WindowEvent::PointerMoved { position: to },
            WindowEvent::PointerReleased {
                position: to,
                button,
            },
        ] {
            window.dispatch_event(event);
        }
    }

    /// Moves the pointer onto the middle of the element showing `text`.
    ///
    /// # Panics
    ///
    /// Panics if nothing shows it.
    pub fn hover(&self, text: &str) {
        let handle = self
            .find(text)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("nothing shows {text:?}"));
        let at = handle.absolute_position();
        let size = handle.size();
        let position = LogicalPosition::new(at.x + size.width / 2.0, at.y + size.height / 2.0);
        self.window
            .window()
            .dispatch_event(WindowEvent::PointerMoved { position });
    }

    /// Makes the window `width` wide, at the design's height.
    pub fn resize(&self, width: f32) {
        self.window
            .window()
            .set_size(slint::LogicalSize::new(width, WINDOW.1));
        // The window reports its new size from its change handlers, which run
        // with the event loop's next turn.
        slint::platform::update_timers_and_animations();
    }

    /// The tabs titled `title`, in the center or the terminals' strip.
    pub fn tabs(&self, title: &str) -> Vec<ElementHandle> {
        self.find(title)
            .into_iter()
            .filter(|e| e.accessible_role() == Some(AccessibleRole::Tab))
            .collect()
    }
}
