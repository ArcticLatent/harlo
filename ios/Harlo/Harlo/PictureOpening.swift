import HarloKit
import SwiftUI

/// The picture the viewer opens at (Identifiable, for `fullScreenCover`).
struct PictureOpening: Identifiable {
    let path: String
    var id: String { path }
}
