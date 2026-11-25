import HarloKit
import SwiftUI
import UIKit

/// While it works or waits for you: what it's doing, for how long. A
/// command it waits for can be sent to the background from here: it keeps
/// running, and the agent goes on.
struct ActivityLine: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let session: OpenSession
    /// On its way to harlo.
    @State private var moving = false

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
                    if !session.backgroundCall.isEmpty {
                        Button {
                            moving = true
                            Task {
                                await model.sendToBackground()
                                moving = false
                            }
                        } label: {
                            Text("Background")
                                .scaledFont(13.5, .medium)
                                .foregroundStyle(Color(p.primary))
                                .lineLimit(1)
                                .fixedSize()
                                .frame(minHeight: 32)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .disabled(moving)
                        .opacity(moving ? 0.55 : 1)
                        .accessibilityLabel("Send to the background")
                        .accessibilityHint("The command keeps running and the agent goes on")
                    }
                }
                .foregroundStyle(Color(session.requests.isEmpty ? p.foreground : p.warning))
            }
            .padding(.horizontal, 18)
            .padding(.top, 8)
        }
    }
}
