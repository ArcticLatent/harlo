//! System fonts and the installed font families (Qt's font database).

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
        include!("harlo_fonts.h");

        /// The desktop's interface (`fixed` false) or monospace font family.
        #[namespace = "harlo"]
        fn system_font_family(fixed: bool) -> QString;
        /// Installed families; only fixed-pitch ones when `fixed`.
        #[namespace = "harlo"]
        fn font_families(fixed: bool) -> QStringList;
        /// Sets the application font family ("" = the desktop's), keeping the size.
        #[namespace = "harlo"]
        fn set_application_font_family(family: &QString);
        /// Adds a font file (a resource path) to the application's fonts.
        #[namespace = "harlo"]
        fn add_application_font(path: &QString) -> bool;
    }
}

pub use ffi::{font_families, set_application_font_family, system_font_family};

/// harlo's interface font unless another is chosen (Settings → Appearance):
/// bundled, so it's there whether or not it's installed. Code is in
/// JetBrains Mono (bundled too).
pub const DEFAULT_UI_FONT: &str = "Geist";

/// Loads the bundled Geist and JetBrains Mono (every weight harlo uses).
pub fn load_bundled() {
    for family in ["Geist", "JetBrainsMono"] {
        for w in ["Regular", "Italic", "Medium", "MediumItalic", "SemiBold", "SemiBoldItalic", "Bold", "BoldItalic"] {
            let path = format!(":/qt/qml/harlo/fonts/{family}-{w}.ttf");
            if !ffi::add_application_font(&cxx_qt_lib::QString::from(&path)) {
                tracing::warn!("couldn't load the bundled font {path}");
            }
        }
    }
}

/// The interface font for a setting ("" = harlo's default).
pub fn ui_family(setting: &str) -> String {
    if setting.is_empty() { DEFAULT_UI_FONT.to_owned() } else { setting.to_owned() }
}
