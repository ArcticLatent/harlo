import HarloKit
import SwiftUI
import UIKit

/// What a tool call came to: lines added and removed, tests passed, a
/// failure, coloured as the desktop's diffs and statuses.
struct ResultBadge: View {
    @Environment(\.palette) private var p
    let text: String

    var body: some View {
        Group {
            if text.hasPrefix("+"), let space = text.firstIndex(of: " ") {
                HStack(spacing: 4) {
                    Text(text[..<space]).foregroundStyle(Color(p.token(9) ?? p.success))
                    Text(text[text.index(after: space)...]).foregroundStyle(Color(p.token(10) ?? p.error))
                }
            } else if text.hasPrefix("+") {
                Text(text).foregroundStyle(Color(p.token(9) ?? p.success))
            } else {
                Text(text).foregroundStyle(Color(bad ? p.error : text.hasSuffix("passed") ? p.success : p.muted))
            }
        }
        .scaledFont(13, .medium)
        .lineLimit(1)
        .shortOrCut(text)
    }

    var bad: Bool { text.hasSuffix("failed") || text.hasPrefix("exit") }
}
