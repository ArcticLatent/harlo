import HarloKit
import SwiftUI
import UIKit

/// An agent's name in a small capsule.
struct AgentTag: View {
    @Environment(\.palette) private var p
    let harness: String

    var body: some View {
        Text(Style.harnessLabel(harness))
            .scaledFont(12, .medium)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background { RoundedRectangle(cornerRadius: Corner.small).fill(Color(p.subtle)) }
            .overlay { RoundedRectangle(cornerRadius: Corner.small).strokeBorder(Color(p.border), lineWidth: 1) }
            .foregroundStyle(Color(p.muted))
    }
}
