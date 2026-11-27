import HarloKit
import SwiftUI
import UIKit

/// Your message, laid out as the agent's reply is: your penguin, "You" and
/// when on the first line, the message full width under it, on a quiet
/// card so a long conversation still shows where each turn starts.
struct UserMessage: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let row: Row

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                UserAvatar(size: Transcript.tile)
                    // ("You" beside it says it.)
                    .accessibilityHidden(true)
                Text("You")
                    .scaledFont(16.5, .semibold)
                if row.time > 0 {
                    Text(Style.clock(row.time))
                        .scaledFont(13.5)
                        .foregroundStyle(Color(p.muted))
                }
            }
            VStack(alignment: .leading, spacing: 6) {
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
                            Text(row.subtitle)
                                .scaledFont(12.5, .medium)
                                .foregroundStyle(Color(p.muted))
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
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .card(p)
        }
        .padding(.top, 10)
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
        .background { RoundedRectangle(cornerRadius: Corner.small).fill(Color(p.subtle)) }
    }
}
