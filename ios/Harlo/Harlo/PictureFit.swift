import HarloKit
import SwiftUI

/// How big a picture is drawn: as big as fits, but at most four screen pixels
/// to each of its own. A phone's screenshot fills the width, a 300-pixel one
/// nearly does, an icon stays an icon (past that its text goes soft; zooming
/// still goes further).
enum PictureFit {
    static func size(_ pixels: CGSize, in box: CGSize, scale: CGFloat) -> CGSize {
        guard pixels.width > 0, pixels.height > 0, box.width > 0, box.height > 0 else { return .zero }
        let most = min(box.width / pixels.width, box.height / pixels.height, 4 / max(scale, 1))
        return CGSize(width: (pixels.width * most).rounded(), height: (pixels.height * most).rounded())
    }
}
