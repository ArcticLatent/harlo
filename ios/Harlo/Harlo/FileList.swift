import HarloKit
import SwiftUI
import UIKit

/// Files matching an `@` mention; tapping one puts it in the message.
struct FileList: View {
    @Environment(\.palette) private var p
    let files: [String]
    var pick: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(files, id: \.self) { f in
                Button { pick(f) } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "doc.text").scaledFont(12).foregroundStyle(Color(p.muted))
                        Text(f.split(separator: "/").last.map(String.init) ?? f).font(Style.mono(13.5, .semibold)).lineLimit(1)
                        if f.contains("/") {
                            Text(f.split(separator: "/").dropLast().joined(separator: "/"))
                                .font(Style.mono(11.5))
                                .foregroundStyle(Color(p.muted))
                                .lineLimit(1)
                                .truncationMode(.head)
                        }
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                // (Its path, whole: the name and the folder are read apart.)
                .accessibilityLabel(f)
                .accessibilityIdentifier("mention:" + f)
                if f != files.last { Divider().overlay(Color(p.border)) }
            }
        }
        .card(p, radius: 12)
    }
}
