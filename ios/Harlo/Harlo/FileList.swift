import HarloKit
import SwiftUI
import UIKit

/// Files, and the folder's other sessions, matching an `@` mention;
/// tapping one puts it in the message (a session as `@session:1a2b3c4d`).
struct FileList: View {
    @Environment(\.palette) private var p
    let files: [String]
    var sessions: [SessionMention] = []
    var pick: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(sessions) { s in
                Button { pick(s.inserted) } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "bubble.left.and.text.bubble.right").scaledFont(12).foregroundStyle(Color(p.muted))
                        Text(s.title.isEmpty ? "Untitled session" : s.title).scaledFont(14.5, .medium).lineLimit(1)
                        Text(Style.harnessLabel(s.harness))
                            .scaledFont(12)
                            .foregroundStyle(Color(p.muted))
                            .lineLimit(1)
                            .fixedSize()
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Session: \(s.title)")
                if s != sessions.last || !files.isEmpty { Divider().overlay(Color(p.border)) }
            }
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
                if f != files.last { Divider().overlay(Color(p.border)) }
            }
        }
        .card(p, radius: 12)
    }
}
