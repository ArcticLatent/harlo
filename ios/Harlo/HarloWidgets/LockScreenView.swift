import HarloKit
import SwiftUI

struct LockScreenView: View {
    let attributes: SessionActivityAttributes
    let state: SessionActivityAttributes.ContentState
    /// No news from harlo for a while.
    var stale = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Live.mark(state.phase)
                Text(attributes.title)
                    .font(.system(size: 15, weight: .semibold))
                    .lineLimit(1)
                Spacer(minLength: 6)
                // (A running clock is a live timer, which has no width of its
                // own: held to its room, or the card can't be laid out and
                // shows empty. "Took 4m 11s" keeps its whole width.)
                Group {
                    if Live.running(state) {
                        Live.clock(state)
                            .frame(maxWidth: 80, alignment: .trailing)
                    } else {
                        Live.clock(state, took: true)
                            .fixedSize()
                    }
                }
                .font(.system(size: 14, weight: .medium).monospacedDigit())
                .foregroundStyle(.secondary)
                .lineLimit(1)
            }
            Text(Live.words(state, stale: stale))
                .font(.system(size: 13, design: .monospaced))
                .foregroundStyle(stale ? .secondary : Live.color(state.phase))
                .lineLimit(2)
            HStack(spacing: 8) {
                Text(Live.agent(attributes.harness))
                    .font(.system(size: 11, weight: .medium, design: .monospaced))
                    .foregroundStyle(.secondary)
                if state.total > 0 {
                    ProgressView(value: Double(state.done), total: Double(state.total))
                        .tint(Live.accent)
                    Text("\(state.done)/\(state.total)")
                        .font(.system(size: 11, weight: .medium, design: .monospaced))
                        .foregroundStyle(.secondary)
                }
            }
        }
        .foregroundStyle(.white)
    }
}
