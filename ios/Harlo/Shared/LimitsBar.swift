import HarloKit
import SwiftUI

/// A thin bar: how much of a window is used.
struct LimitsBar: View {
    let fraction: Double
    let tint: Color
    let track: Color

    var body: some View {
        GeometryReader { g in
            ZStack(alignment: .leading) {
                Capsule().fill(track)
                Capsule().fill(tint).frame(width: max(fraction > 0 ? 4 : 0, g.size.width * min(max(fraction, 0), 1)))
            }
        }
        .frame(height: 5)
    }
}
