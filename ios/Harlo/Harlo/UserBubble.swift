import HarloKit
import SwiftUI
import UIKit

/// Your message: a bubble on the right in the accent's colour (when you
/// sent it under it), and your penguin.
struct UserBubble: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let row: Row

    var body: some View {
        let (fill, ink) = p.sentBubble
        HStack(alignment: .bottom, spacing: 8) {
            Spacer(minLength: 36)
            VStack(alignment: .trailing, spacing: 5) {
                // What came with it: photos (as pictures, from a harlo that
                // sends them; else by name) and files, and tags ("1 image").
                let pictures = model.showsPictures ? row.images : []
                if !pictures.isEmpty {
                    PictureStrip(paths: pictures)
                }
                let files = row.title.split(separator: "\n").map(String.init).filter { !$0.isEmpty && (pictures.isEmpty || !Self.isPhoto($0)) }
                if !files.isEmpty || !row.subtitle.isEmpty {
                    Flow(spacing: 6) {
                        ForEach(files, id: \.self) { f in
                            attachment(f)
                        }
                        if !row.subtitle.isEmpty {
                            Text(row.subtitle).scaledFont(12.5, .medium).opacity(0.92)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                if !row.text.isEmpty {
                    Text(row.text)
                        .scaledFont(16.5)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .fixedSize(horizontal: false, vertical: true)
                        .textSelection(.enabled)
                }
                if row.time > 0 {
                    Text(Style.clock(row.time))
                        .scaledFont(12.5, .semibold)
                }
            }
            .foregroundStyle(Color(ink))
            .padding(.horizontal, 15)
            .padding(.vertical, 11)
            .background { RoundedRectangle(cornerRadius: 20, style: .continuous).fill(Color(fill)) }
            .layoutPriority(1)
            UserAvatar(size: 30)
        }
        .padding(.top, 8)
    }

    /// An attachment: a photo (as the phone names them, or an image file)
    /// or a file, by its name.
    static func isPhoto(_ path: String) -> Bool {
        let name = path.split(separator: "/").last.map(String.init) ?? path
        return name.hasPrefix("Photo ") || PictureNames.isPicture(name)
    }

    func attachment(_ path: String) -> some View {
        let name = path.split(separator: "/").last.map(String.init) ?? path
        let photo = Self.isPhoto(path)
        return HStack(spacing: 5) {
            Image(systemName: photo ? "photo" : "doc").scaledFont(12, .semibold)
            Text(name).scaledFont(13, .medium).lineLimit(1).truncationMode(.middle)
        }
        .padding(.horizontal, 9)
        .frame(minHeight: 26)
        .background { Capsule().fill(.primary.opacity(0.15)) }
    }
}
