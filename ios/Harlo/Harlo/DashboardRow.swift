import HarloKit
import SwiftUI

/// A session's card on the dashboard, with its swipe and long-press
/// actions.
struct DashboardRow: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let session: SessionSummary
    /// Its project shows (the recent ones, or grouped by day).
    var showProject: Bool
    var rename: (SessionSummary) -> Void

    var body: some View {
        let s = session
        let unread = model.isUnread(s)
        Button {
            model.open(s)
        } label: {
            SessionCard(session: s, unread: unread, showProject: showProject)
        }
        .buttonStyle(.plain)
        .plainRow(top: 5, bottom: 5)
        .swipeActions(edge: .trailing) {
            Button {
                Task { await model.archive(s) }
            } label: {
                Label("Archive", systemImage: "archivebox")
            }
            .tint(Color(p.muted))
        }
        .swipeActions(edge: .leading) {
            Button {
                Task { await model.pin(s, !s.pinned) }
            } label: {
                Label(s.pinned ? "Unpin" : "Pin", systemImage: s.pinned ? "pin.slash" : "pin")
            }
            .tint(Color(p.primary))
            Button {
                model.markUnread(s, !unread)
            } label: {
                Label(unread ? "Read" : "Unread", systemImage: unread ? "envelope.open" : "envelope.badge")
            }
            .tint(Color(p.info))
        }
        .contextMenu {
            Button {
                rename(s)
            } label: {
                Label("Rename", systemImage: "pencil")
            }
            Button {
                Task { await model.pin(s, !s.pinned) }
            } label: {
                Label(s.pinned ? "Unpin" : "Pin", systemImage: s.pinned ? "pin.slash" : "pin")
            }
            Button {
                model.markUnread(s, !unread)
            } label: {
                Label(unread ? "Mark as read" : "Mark as unread", systemImage: unread ? "envelope.open" : "envelope.badge")
            }
            Button {
                Task { await model.archive(s) }
            } label: {
                Label("Archive", systemImage: "archivebox")
            }
        }
    }
}
