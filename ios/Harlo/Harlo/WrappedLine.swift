import HarloKit
import SwiftUI
import UIKit

/// A line of code, wrapped with a hanging indent: its further rows start
/// under its own code, not at the margin, so the nesting still reads (and
/// a line too long to fit after its indentation doesn't break there,
/// leaving its first row empty). `lead` is shown before the indentation
/// (a diff's + or −).
struct WrappedLine<Code: View>: View {
    var lead = ""
    let indent: Int
    let font: Font
    @ViewBuilder let code: Code

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 0) {
            if !lead.isEmpty { Text(lead).font(font).fixedSize() }
            // (Hidden digits: a monospaced font's width, which spaces
            // alone may not be given.)
            if indent > 0 { Text(String(repeating: "0", count: indent)).font(font).fixedSize().hidden() }
            code.frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
