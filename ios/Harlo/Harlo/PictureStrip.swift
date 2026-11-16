import HarloKit
import SwiftUI

/// A row's pictures, as thumbnails in a row that wraps.
struct PictureStrip: View {
    @Environment(HarloModel.self) private var model
    let paths: [String]
    var side: CGFloat = 84

    var body: some View {
        if model.showsPictures && !paths.isEmpty {
            Flow(spacing: 8) {
                ForEach(paths, id: \.self) { path in
                    PictureTile(path: path, side: side)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
