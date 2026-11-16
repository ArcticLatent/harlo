import HarloKit
import SwiftUI
import UIKit

/// A run of tool calls and thoughts: its summary line, and (open) each
/// step on a timeline: a mark per step (done, failed, running) joined by a
/// line, and the step's card.
struct RunView: View {
    @Environment(\.palette) private var p
    let group: String
    let summary: String
    let rows: [Row]
    let open: Bool
    let active: Bool
    let session: OpenSession

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Button {
                withAnimation(.easeOut(duration: 0.15)) { session.toggleRun(group, open: open) }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: open ? "chevron.down" : "chevron.right")
                        .font(.caption2.weight(.semibold))
                        .frame(width: 12)
                    Text(summary.isEmpty ? "\(rows.count) steps" : summary)
                        .scaledFont(14)
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                    // How long it took, or (still going) for how long.
                    if active, let start = rows.first?.time, start > 0 {
                        TimelineView(.periodic(from: .now, by: 1)) { ctx in
                            Text("· " + Style.elapsed(since: start, now: ctx.date)).scaledFont(14).monospacedDigit()
                        }
                    } else if let ms = rows.first?.runMs, ms > 0 {
                        Text("· " + Style.duration(Double(ms))).scaledFont(14)
                    }
                    if active {
                        StatusDot(status: "busy")
                    }
                }
                .foregroundStyle(Color(p.muted))
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .padding(.leading, (Transcript.rail - 12) / 2)
            if open {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(rows.enumerated()), id: \.element.id) { i, r in
                        TimelineStep(row: r, first: i == 0, last: i == rows.count - 1)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
