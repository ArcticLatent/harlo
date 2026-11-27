import HarloKit
import SwiftUI
import UIKit

/// One thumbnail: square, the picture filling it; a tap opens it. Says so
/// when harlo can't send it (the file's gone, say).
struct PictureTile: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.openPicture) private var openPicture
    let path: String
    var side: CGFloat = 84
    @State private var image: UIImage?
    @State private var why = ""

    var body: some View {
        Button {
            openPicture?(path)
        } label: {
            ZStack {
                Rectangle().fill(Color(p.codeBackground))
                if let image {
                    Image(uiImage: image)
                        .resizable()
                        .interpolation(.high)
                        .scaledToFill()
                        .frame(width: side, height: side)
                        .clipped()
                } else if !why.isEmpty {
                    VStack(spacing: 4) {
                        Image(systemName: "photo")
                            .font(.system(size: 18, weight: .medium))
                        Text(why.contains("no longer") ? "No longer on disk" : "Can't show")
                            .font(.caption2.weight(.medium))
                            .multilineTextAlignment(.center)
                            .lineLimit(2)
                            .minimumScaleFactor(0.8)
                    }
                    .foregroundStyle(Color(p.muted))
                    .padding(.horizontal, 6)
                } else {
                    ProgressView()
                }
            }
            .frame(width: side, height: side)
            .clipShape(RoundedRectangle(cornerRadius: Corner.small, style: .continuous))
            .overlay { RoundedRectangle(cornerRadius: Corner.small, style: .continuous).strokeBorder(Color(p.hairline), lineWidth: 1) }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("pictureTile")
        .accessibilityLabel("Picture: " + PictureNames.spoken(model.session?.pictureName(path) ?? PictureNames.name(path)))
        .accessibilityHint(why.isEmpty ? "Opens it" : why)
        .task(id: path) {
            let r = await model.picture(path, thumb: true)
            image = r.picture.flatMap { UIImage(data: $0.data) }
            why = image == nil ? (r.why.isEmpty ? "harlo couldn't read that picture." : r.why) : ""
        }
    }
}
