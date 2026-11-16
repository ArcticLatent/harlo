import HarloKit
import PhotosUI
import SwiftUI
import UIKit

/// Photos for a message: from the library or the camera, made small
/// enough for the agent (and the relay) on the phone.
enum Photos {
    /// The longest side agents use.
    static let maxSide: CGFloat = 1568

    static func photo(_ image: UIImage) -> Photo? {
        let size = image.size
        let scale = min(1, maxSide / max(size.width, size.height, 1))
        let target = CGSize(width: (size.width * scale).rounded(), height: (size.height * scale).rounded())
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        let small = UIGraphicsImageRenderer(size: target, format: format).image { _ in
            image.draw(in: CGRect(origin: .zero, size: target))
        }
        guard let data = small.jpegData(compressionQuality: 0.8) else { return nil }
        return Photo(data: data, mimeType: "image/jpeg")
    }
}
