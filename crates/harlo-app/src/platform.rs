//! App-level platform settings (window icon, desktop app id).

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
        include!("harlo_platform.h");

        /// Creates the (widgets-capable) application object from `args`.
        #[namespace = "harlo"]
        fn create_application(args: &QStringList);
        /// Runs the event loop; returns the exit code.
        #[namespace = "harlo"]
        fn exec_application() -> i32;

        /// After the event loop and the QML engine: what's waiting to be
        /// deleted, then the application.
        #[namespace = "harlo"]
        fn shutdown_application();

        #[namespace = "harlo"]
        fn set_window_icon(path: &QString);
        #[namespace = "harlo"]
        fn set_desktop_file_name(name: &QString);
        /// A key press as shortcut text (`Ctrl+Shift+T`).
        #[namespace = "harlo"]
        fn key_sequence_text(key: i32, modifiers: i32) -> QString;
        /// The clipboard's files, if it holds only files on this machine.
        #[namespace = "harlo"]
        fn clipboard_files() -> QStringList;
        /// Saves the clipboard's image as `base`.png (.jpg if that'd be too
        /// big to send), and returns its path: "" if it holds text, or nothing.
        #[namespace = "harlo"]
        fn clipboard_image_to(base: &QString) -> QString;
        /// Copies Qt/QML messages into the log file at `path`.
        #[namespace = "harlo"]
        fn install_message_log(path: &QString);
    }
}

pub use ffi::{clipboard_files, clipboard_image_to, create_application, exec_application, install_message_log, key_sequence_text, set_desktop_file_name, set_window_icon, shutdown_application};
