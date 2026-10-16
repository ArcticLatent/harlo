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

        #[namespace = "harlo"]
        fn set_window_icon(path: &QString);
        #[namespace = "harlo"]
        fn set_desktop_file_name(name: &QString);
        /// A key press as shortcut text (`Ctrl+Shift+T`).
        #[namespace = "harlo"]
        fn key_sequence_text(key: i32, modifiers: i32) -> QString;
        /// Copies Qt/QML messages into the log file at `path`.
        #[namespace = "harlo"]
        fn install_message_log(path: &QString);
    }
}

pub use ffi::{create_application, exec_application, install_message_log, key_sequence_text, set_desktop_file_name, set_window_icon};
