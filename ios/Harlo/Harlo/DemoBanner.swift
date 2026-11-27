import HarloKit
import SwiftUI

/// In the demo: what it is, and the way out to pairing.
struct DemoBanner: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "sparkles")
                .scaledFont(18)
                .foregroundStyle(Color(p.primary))
            VStack(alignment: .leading, spacing: 4) {
                Text("This is the demo")
                    .scaledFont(16, .semibold)
                Text("Sample sessions, nothing connected. Install harlo on your computer and pair this phone to work with your own agents.")
                    .scaledFont(14)
                    .foregroundStyle(Color(p.muted))
                    .fixedSize(horizontal: false, vertical: true)
                Button("Pair with harlo") { model.endDemo() }
                    .scaledFont(15, .medium)
                    .padding(.top, 4)
            }
            Spacer(minLength: 0)
        }
        .padding(14)
        .card(p, radius: Corner.large, fill: p.accentSoft.over(p.raised))
    }
}
