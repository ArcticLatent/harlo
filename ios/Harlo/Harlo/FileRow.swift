import HarloKit
import SwiftUI

/// A changed file: its status letter, name and folder.
struct FileRow: View {
    @Environment(\.palette) private var p
    let file: Changes.File

    var color: RGBA {
        switch file.status {
        case "A", "?": return p.token(9) ?? p.success
        case "D": return p.token(10) ?? p.error
        default: return p.token(11) ?? p.warning
        }
    }

    var body: some View {
        HStack(spacing: 10) {
            Text(file.status == "?" ? "U" : file.status)
                .font(Style.mono(12, .bold))
                .foregroundStyle(Color(color))
                .frame(width: 16)
            VStack(alignment: .leading, spacing: 2) {
                Text(file.name).font(Style.mono(13.5, .medium)).lineLimit(1)
                if !file.folder.isEmpty {
                    Text(file.folder).font(Style.mono(11)).foregroundStyle(Color(p.muted)).lineLimit(1).truncationMode(.head)
                }
            }
        }
    }
}
