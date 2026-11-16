import HarloKit
import SwiftUI

/// While it works: a message steers the run, or waits for it to end.
struct SteerSwitch: View {
    @Environment(\.palette) private var p
    let session: OpenSession

    var body: some View {
        HStack(spacing: 0) {
            ForEach([true, false], id: \.self) { steer in
                Button {
                    session.steer = steer
                } label: {
                    Text(steer ? "Steer" : "Queue")
                        .scaledFont(13, .semibold)
                        .padding(.horizontal, 10)
                        .frame(minHeight: 32)
                        .background(Color(session.steer == steer ? p.primary : p.subtle))
                        .foregroundStyle(Color(session.steer == steer ? p.onPrimary : p.muted))
                }
                .buttonStyle(.plain)
            }
        }
        .clipShape(Capsule())
        .overlay { Capsule().strokeBorder(Color(p.border), lineWidth: 1) }
        .sensoryFeedback(.selection, trigger: session.steer)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(session.steer ? "Steer the run" : "Queue for after the run")
    }
}
