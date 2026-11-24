import HarloKit
import SwiftUI

/// The widget's background: harlo's own, light or dark.
struct SessionsBackground: View {
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        let p = Theme.standard.palette(dark: scheme == .dark)
        let c = p.background.mix(p.foreground, scheme == .dark ? 0.03 : 0)
        Color(.sRGB, red: c.r, green: c.g, blue: c.b, opacity: 1)
    }
}
