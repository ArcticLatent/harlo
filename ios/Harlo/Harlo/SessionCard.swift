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

    /// Its model as people say it ("claude-opus-5-5" → "Opus 5.5"); ""
    /// when it isn't known (its tile shows the agent either way).
    var model: String {
        session.modelName.isEmpty ? "" : WorkflowStage.modelName(session.modelName)
    }

    /// Its project and the machine it's on (grouped by project, the group
    /// says those).
    var place: String {
        guard showProject, !session.project.isEmpty else { return "" }
        return session.host.isEmpty ? session.project : session.project + " on " + session.host
    }

    /// Whose helper it is, and how it ended.
    var extra: String {
        var parts: [String] = []
        if !nested && !session.helperOf.isEmpty { parts.append("Helper of “\(session.helperOf)”") }
        if !session.settled.isEmpty { parts.append(session.settled) }
        return parts.joined(separator: " · ")
    }

    /// A detail with its kind as a symbol (a folder for the project, a
    /// chip for the model, a branch), so they don't read as one sentence.
    func fact(_ text: String, _ symbol: String, spoken: String, strong: Bool = false) -> some View {
        HStack(spacing: 5) {
            Image(systemName: symbol)
                .scaledFont(12, .medium)
                .foregroundStyle(Color(p.muted))
                .accessibilityHidden(true)
            Text(text)
                .scaledFont(14.5, strong ? .medium : .regular)
                .foregroundStyle(Color(strong ? p.foreground.mix(p.background, 0.2) : p.muted))
                .lineLimit(1)
                .accessibilityLabel(spoken)
        }
    }

    /// The project, then the model, always on one line: a long project
    /// name is cut short, the model isn't.
    @ViewBuilder var facts: some View {
        let agent = Style.harnessLabel(session.harness)
        if !place.isEmpty || !model.isEmpty {
            HStack(spacing: 14) {
                if !place.isEmpty {
                    fact(place, "folder", spoken: model.isEmpty ? "Project \(place), \(agent)" : "Project \(place)", strong: true)
                }
                // (Read out with its agent: on screen only its tile says it.)
                if !model.isEmpty {
                    fact(model, "cpu", spoken: "\(model) with \(agent)")
                        .layoutPriority(1)
                }
            }
        }
    }

    /// Its branch, then whose helper it is and how it ended.
    @ViewBuilder var more: some View {
        if !session.branch.isEmpty || !extra.isEmpty {
            HStack(spacing: 8) {
                if !session.branch.isEmpty {
                    fact(session.branch, "arrow.triangle.branch", spoken: "Branch \(session.branch)")
                }
                if !extra.isEmpty {
                    Text(extra)
                        .scaledFont(13.5)
                        .foregroundStyle(Color(p.muted))
                        .lineLimit(1)
                        .layoutPriority(1)
                }
            }
        }
    }

    var body: some View {
        HStack(spacing: 14) {
            AgentTile(harness: session.harness, maker: session.maker, size: 46, ring: p.raised)
            VStack(alignment: .leading, spacing: 5) {
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
                facts
                more
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
