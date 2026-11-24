import HarloKit
import SwiftUI
import UIKit

struct ProgressRing: View {
    @Environment(\.palette) private var p
    let fraction: Double

    var body: some View {
        ZStack {
            Circle().stroke(Color(p.border), lineWidth: 2.5)
            Circle().trim(from: 0, to: fraction).stroke(Color(p.success), style: StrokeStyle(lineWidth: 2.5, lineCap: .round)).rotationEffect(.degrees(-90))
        }
        .frame(width: 14, height: 14)
    }
}
