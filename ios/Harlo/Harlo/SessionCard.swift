import HarloKit
import SwiftUI

/// A session on a card: its agent's tile, its name, where it runs, and its
/// state (a pill), or when it was last used.
struct SessionCard: View {
    @Environment(\.palette) private var p
    let session: SessionSummary
    /// Finished since you looked, or marked unread.
    var unread = false
    /// Its project shows (the recent ones, or grouped by day).
    var showProject = false
    /// A helper shown under the session that started it.
    var nested = false
    @State private var now = Date.now
    @Environment(\.dynamicTypeSize) private var typeSize

    /// At the largest text sizes the title gets the row, and its state goes
    /// under it.
    var stacked: Bool { typeSize.isAccessibilitySize }

    @ViewBuilder var state: some View {
        if StatusPill.shows(session.status, unread: unread) {
            StatusPill(status: session.status, unread: unread)
        } else {
            Text(Style.ago(session.modified, now: now))
                .scaledFont(13.5)
                .foregroundStyle(Color(p.muted))
        }
    }

    var detail: String {
        // (Its model, when known, before its agent: "GLM 5.3 · Claude".)
        var parts = [session.modelName.isEmpty ? Style.harnessLabel(session.harness) : session.modelName + " · " + Style.harnessLabel(session.harness)]
        if !nested && !session.helperOf.isEmpty { parts.append("Helper of “\(session.helperOf)”") }
        if showProject && !session.project.isEmpty { parts.append(session.project) }
        // (Grouped by project, the group says its machine.)
        if showProject && !session.host.isEmpty { parts.append(session.host) }
        if !session.branch.isEmpty { parts.append(session.branch) }
        if !session.settled.isEmpty { parts.append(session.settled) }
        return parts.joined(separator: " · ")
    }

    var body: some View {
        HStack(spacing: 14) {
            AgentTile(harness: session.harness, maker: session.maker, size: 46, ring: p.raised)
            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .firstTextBaseline, spacing: 5) {
                    if nested {
                        Image(systemName: "arrow.turn.down.right")
                            .scaledFont(11, .semibold)
                            .foregroundStyle(Color(p.muted))
                            .accessibilityLabel("Helper of \(session.helperOf)")
                    }
                    if session.pinned {
                        Image(systemName: "pin.fill")
                            .scaledFont(10)
                            .foregroundStyle(Color(p.muted))
                    }
                    Text(session.title.isEmpty ? "New session" : session.title)
                        .scaledFont(17, unread ? .semibold : .medium)
                        .lineLimit(stacked ? 4 : 2)
                        .multilineTextAlignment(.leading)
                }
                Text(detail)
                    .scaledFont(14.5)
                    .foregroundStyle(Color(p.muted))
                    // (With the model first it runs longer: a second line
                    // rather than cut off.)
                    .lineLimit(stacked ? 3 : 2)
                if stacked { state }
            }
            Spacer(minLength: 6)
            if !stacked { state }
            Image(systemName: "chevron.right")
                .scaledFont(14, .semibold)
                .foregroundStyle(Color(p.muted))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 14)
        .card(p, radius: 18)
        .contentShape(RoundedRectangle(cornerRadius: 18))
        .accessibilityElement(children: .combine)
    }
}
