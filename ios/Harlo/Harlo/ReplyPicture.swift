import HarloKit
import SwiftUI
import UIKit

/// A picture a reply shows on its own line, in its place: as wide as the
/// reply (at most four screen pixels to each of its own, as in the
/// viewer), a tap opens the viewer. Its
/// words when it isn't one of the session's pictures (a web address, a
/// file that's gone).
struct ReplyPicture: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.openPicture) private var openPicture
    @Environment(\.displayScale) private var displayScale
    @Environment(\.dynamicTypeSize) private var typeSize
    let src: String
    let alt: String
    @State private var image: UIImage?
    @State private var why = ""

    var path: String? {
        guard model.showsPictures, let s = model.session else { return nil }
        return PictureNames.match(src, in: s.pictures)
    }

    var body: some View {
        if let path {
            Button {
                openPicture?(path)
            } label: {
                Group {
                    if let image {
                        let px = CGSize(width: image.size.width * image.scale, height: image.size.height * image.scale)
                        Image(uiImage: image)
                            .resizable()
                            .interpolation(.high)
                            .aspectRatio(px, contentMode: .fit)
                            .frame(maxWidth: px.width * 4 / max(displayScale, 1))
                    } else if !why.isEmpty {
                        Label(alt.isEmpty ? why : "\(alt): \(why)", systemImage: "photo")
                            .font(.subheadline)
                            .foregroundStyle(Color(p.muted))
                            .padding(12)
                    } else {
                        Rectangle()
                            .fill(Color(p.codeBackground))
                            .aspectRatio(16 / 10, contentMode: .fit)
                            .overlay { ProgressView() }
                    }
                }
                .clipShape(RoundedRectangle(cornerRadius: Corner.small, style: .continuous))
                .overlay { RoundedRectangle(cornerRadius: Corner.small, style: .continuous).strokeBorder(Color(p.hairline), lineWidth: 1) }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityIdentifier("replyPicture")
            .accessibilityLabel("Picture: " + (alt.isEmpty ? PictureNames.spoken(PictureNames.name(path)) : alt))
            .accessibilityHint(why.isEmpty ? "Opens it" : why)
            .task(id: path) {
                // (The thumbnail first: it's quick, and gives the shape.)
                if let t = await model.picture(path, thumb: true).picture { image = UIImage(data: t.data) }
                let r = await model.picture(path, thumb: false)
                if let full = r.picture.flatMap({ UIImage(data: $0.data) }) {
                    image = full
                } else if image == nil {
                    why = r.why.isEmpty ? "harlo couldn't read that picture." : r.why
                }
            }
        } else {
            Text(Style.attributed([Span(alt, italic: true)], p, typeSize: typeSize))
                .textSelection(.enabled)
        }
    }
}
