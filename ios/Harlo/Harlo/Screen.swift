import HarloKit
import SwiftUI
import UIKit

/// A screen's background and its navigation bar in the theme's colours.
struct Screen: ViewModifier {
    @Environment(\.palette) private var p
    var sidebar = false

    func body(content: Content) -> some View {
        let bg = Color(sidebar ? p.sidebar : p.background)
        content
            .scrollContentBackground(.hidden)
            .background { bg.ignoresSafeArea() }
            .toolbarBackground(bg, for: .navigationBar)
            .toolbarBackgroundVisibility(.visible, for: .navigationBar)
    }
}
