import HarloKit
import SwiftUI

/// The picture the viewer opens at (Identifiable, for `fullScreenCover`),
/// or with `grid`, every picture at once.
struct PictureOpening: Identifiable {
    let path: String
    var grid = false
    var id: String { path }
}
