import HarloKit
import SwiftUI
import UIKit

/// A section label: small, uppercase, spaced (as the desktop's).
struct SectionLabel: View {
    @Environment(\.palette) private var p
    let text: String

    var body: some View {
        Text(text.uppercased())
            .scaledFont(12, .medium)
            .tracking(0.8)
            .foregroundStyle(Color(p.label))
            .textCase(nil)
    }
}
