//! Qt base classes declared once. cxx-qt emits casting helpers per
//! declaration, so declaring the same base in two bridges fails to link;
//! other bridges alias the types from here.

// Rust guideline compliant 2026-10-09

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++Qt" {
        include!(<QtCore/QAbstractListModel>);
        #[qobject]
        type QAbstractListModel;
    }
}
