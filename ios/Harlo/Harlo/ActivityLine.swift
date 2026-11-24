import HarloKit
import SwiftUI
import UIKit

/// While it works or waits for you: what it's doing, for how long.
struct ActivityLine: View {
    @Environment(\.palette) private var p
    let session: OpenSession

    var body: some View {
        if let a = session.activity {
            TimelineView(.periodic(from: .now, by: 1)) { ctx in
                HStack(spacing: 8) {
                    StatusDot(status: session.requests.isEmpty ? "busy" : "input")
                        // (Its line says it.)
                        .accessibilityHidden(true)
                    Text(a.text)
                        .scaledFont(13.5)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    if a.since > 0 {
                        Text(Style.elapsed(since: a.since, now: ctx.date))
                            .scaledFont(13.5)
                            .foregroundStyle(Color(p.muted))
                            .monospacedDigit()
                    }
                    Spacer(minLength: 0)
                }
                .foregroundStyle(Color(session.requests.isEmpty ? p.foreground : p.warning))
            }
            .padding(.horizontal, 18)
            .padding(.top, 8)
        }
    }
}
