import HarloKit
import SwiftUI
import UIKit

/// One picture of the grid: its thumbnail, square, over its name.
struct PictureGridCell: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.dynamicTypeSize) private var typeSize
    let path: String
    let name: String
    let open: Bool
    let tap: () -> Void
    @State private var image: UIImage?
    @State private var why = ""

    var body: some View {
        Button(action: tap) {
            VStack(alignment: .leading, spacing: 6) {
                Color.white.opacity(0.08)
                    .aspectRatio(1, contentMode: .fit)
                    .overlay {
                        if let image {
                            Image(uiImage: image).resizable().interpolation(.high).scaledToFill()
                        } else if !why.isEmpty {
                            VStack(spacing: 4) {
                                Image(systemName: "photo").font(.system(size: 20, weight: .light))
                                Text(why.contains("no longer") ? "No longer on disk" : "Can't show")
                                    .font(.caption2.weight(.medium))
                                    .multilineTextAlignment(.center)
                            }
                            .foregroundStyle(PictureViewer.caption)
                            .padding(6)
                        } else {
                            ProgressView().tint(.white)
                        }
                    }
                    .clipShape(RoundedRectangle(cornerRadius: Corner.small, style: .continuous))
                    .overlay { RoundedRectangle(cornerRadius: Corner.small, style: .continuous).strokeBorder(.white, lineWidth: open ? 2 : 0) }
                Text(name)
                    .font(.caption)
                    .foregroundStyle(open ? Color.white : Color(white: 0.85))
                    .lineLimit(typeSize >= .xLarge ? 2 : 1)
                    .truncationMode(.tail)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("pictureGridCell")
        .accessibilityLabel("Picture: " + PictureNames.spoken(name))
        .accessibilityAddTraits(open ? .isSelected : [])
        .task(id: path) {
            let r = await model.picture(path, thumb: true)
            image = r.picture.flatMap { UIImage(data: $0.data) }
            why = image == nil ? (r.why.isEmpty ? "harlo couldn't read that picture." : r.why) : ""
        }
    }
}
