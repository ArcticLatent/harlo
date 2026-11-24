import HarloKit
import SwiftUI
import UIKit

/// Puts the desktop's theme in the environment, for the phone's light or
/// dark appearance.
struct Themed: ViewModifier {
    @Environment(HarloModel.self) private var model
    @Environment(\.colorScheme) private var scheme

    func body(content: Content) -> some View {
        let p = (model.theme ?? .standard).palette(dark: scheme == .dark)
        content
            .environment(\.palette, p)
            .tint(Color(p.primary))
            .foregroundStyle(Color(p.foreground))
    }
}
