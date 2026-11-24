import HarloKit
import SwiftUI
import UIKit

extension View {
    func screen(sidebar: Bool = false) -> some View { modifier(Screen(sidebar: sidebar)) }

    /// A text field in a card: the page's colour (not the system's, which
    /// is black in dark mode) inside a hairline.
    func answerField(_ p: Palette) -> some View {
        textFieldStyle(.plain)
            .padding(.horizontal, 10)
            .padding(.vertical, 10)
            .frame(minHeight: 44)
            .background { RoundedRectangle(cornerRadius: 10).fill(Color(p.background)) }
            .overlay { RoundedRectangle(cornerRadius: 10).strokeBorder(Color(p.border), lineWidth: 1) }
    }

    /// A card: the theme's raised surface with a hairline.
    func card(_ p: Palette, radius: CGFloat = 12, fill: RGBA? = nil) -> some View {
        background { RoundedRectangle(cornerRadius: radius).fill(Color(fill ?? p.raised)) }
            .overlay { RoundedRectangle(cornerRadius: radius).strokeBorder(Color(p.hairline), lineWidth: 1) }
    }
}
