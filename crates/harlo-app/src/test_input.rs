//! Synthetic input for self-tests (`TestInput` in QML): real mouse and key
//! events through Qt Test, see `cpp/harlo_testinput.h`.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("harlo_testinput.h");

        #[namespace = "harlo"]
        fn test_move(x: f64, y: f64);
        #[namespace = "harlo"]
        fn test_click(x: f64, y: f64, button: i32);
        #[namespace = "harlo"]
        fn test_key(key: i32, modifiers: i32);
        #[namespace = "harlo"]
        fn test_type(text: &QString);
        #[namespace = "harlo"]
        fn test_grab_window(path: &QString) -> bool;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        type TestInput = super::TestInputRust;

        /// Moves the pointer to window coordinates (hover).
        #[qinvokable]
        #[cxx_name = "moveTo"]
        fn move_to(self: &TestInput, x: f64, y: f64);
        /// Left click at window coordinates.
        #[qinvokable]
        fn click(self: &TestInput, x: f64, y: f64);
        /// A key press and release (`Qt.Key_*`, `Qt.*Modifier` flags).
        #[qinvokable]
        fn key(self: &TestInput, key: i32, modifiers: i32);
        /// Types text into whatever has the focus.
        #[qinvokable]
        #[cxx_name = "typeText"]
        fn type_text(self: &TestInput, text: &QString);
        /// Saves the whole window (popups included) as an image.
        #[qinvokable]
        #[cxx_name = "grabWindow"]
        fn grab_window(self: &TestInput, path: &QString) -> bool;
        /// A text file's contents ("" if it can't be read): what a stand-in
        /// recorded.
        #[qinvokable]
        #[cxx_name = "readFile"]
        fn read_file(self: &TestInput, path: &QString) -> QString;
        /// Writes a text file (a change "on the host", say).
        #[qinvokable]
        #[cxx_name = "writeFile"]
        fn write_file(self: &TestInput, path: &QString, text: &QString) -> bool;
    }
}

use cxx_qt_lib::QString;

#[derive(Default)]
pub struct TestInputRust;

impl qobject::TestInput {
    pub fn move_to(&self, x: f64, y: f64) {
        qobject::test_move(x, y);
    }

    pub fn click(&self, x: f64, y: f64) {
        qobject::test_click(x, y, 1);
    }

    pub fn key(&self, key: i32, modifiers: i32) {
        qobject::test_key(key, modifiers);
    }

    pub fn grab_window(&self, path: &QString) -> bool {
        qobject::test_grab_window(path)
    }

    pub fn write_file(&self, path: &QString, text: &QString) -> bool {
        std::fs::write(path.to_string(), text.to_string()).is_ok()
    }

    pub fn read_file(&self, path: &QString) -> QString {
        QString::from(&std::fs::read_to_string(path.to_string()).unwrap_or_default())
    }

    pub fn type_text(&self, text: &QString) {
        qobject::test_type(text);
    }
}
