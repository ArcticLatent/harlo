import HarloKit
import SwiftUI

/// A pi extension's lines, as plain text in the code font: wide lines
/// scroll sideways rather than wrap, so what it lines up stays lined up.
struct ExtLines: View {
    @Environment(\.palette) private var p
    let lines: [String]

    var body: some View {
        ScrollView(.horizontal) {
            Text(lines.joined(separator: "\n"))
                .font(Style.mono(13))
                .foregroundStyle(Color(p.foreground))
                .fixedSize(horizontal: true, vertical: false)
                .textSelection(.enabled)
        }
        .scrollIndicators(.hidden)
    }
}
