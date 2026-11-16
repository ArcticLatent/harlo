import HarloKit
import SwiftUI

/// A drawn picture opened to zoom (Identifiable, for `fullScreenCover`).
struct ZoomedPicture: Identifiable {
    let id = UUID()
    let image: UIImage
    let size: CGSize
}
