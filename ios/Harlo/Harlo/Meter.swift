import HarloKit
import SwiftUI

/// A thin bar: how much of something is used.
struct Meter: View {
    @Environment(\.palette) private var p
    let fraction: Double
    let color: RGBA

    var body: some View {
        GeometryReader { g in
            ZStack(alignment: .leading) {
                Capsule().fill(Color(p.hairline))
                Capsule().fill(Color(color)).frame(width: g.size.width * max(0, min(1, fraction)))
            }
        }
        .frame(height: 5)
    }
}
