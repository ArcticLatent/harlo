import HarloKit
import SwiftUI
import UIKit

/// A status's dot, pulsing while it works.
struct PulsingDot: View {
    let color: RGBA
    var pulsing = false
    @State private var dim = false

    var body: some View {
        Circle()
            .fill(Color(color))
            .frame(width: 8, height: 8)
            .opacity(pulsing && dim ? 0.35 : 1)
            .animation(pulsing ? .easeInOut(duration: 0.8).repeatForever(autoreverses: true) : .default, value: dim)
            .onAppear { dim = true }
    }
}
