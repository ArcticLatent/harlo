import HarloKit
import SwiftUI
import UIKit

/// A card, unless it's inside one already.
struct CardUnless: ViewModifier {
    @Environment(\.palette) private var p
    let bare: Bool
    var radius: CGFloat = 12
    var fill: RGBA?

    @ViewBuilder func body(content: Content) -> some View {
        if bare { content } else { content.card(p, radius: radius, fill: fill) }
    }
}
