import HarloKit
import SwiftUI

/// What waits on you, in every session: the Approvals tab (and, from a
/// session, the others' as a sheet, `except` its own).
struct ApprovalsView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.dismiss) private var dismiss
    /// The open session's keys (its requests show in it already).
    var except: [String] = []
    /// A sheet (from a session): a Close button, and opening one goes there.
    var sheet = false

    var waiting: [(session: SessionSummary, request: UIRequest)] {
        model.waiting.filter { !except.contains($0.session.key) }
    }

    var subtitle: String {
        switch waiting.count {
        case 0: return "Nothing needs you right now"
        case 1: return "1 request needs your review"
        default: return "\(waiting.count) requests need your review"
        }
    }

    var body: some View {
        List {
            VStack(alignment: .leading, spacing: 22) {
                if !sheet { AppHeader() }
                BigTitle(text: "Approvals", subtitle: subtitle)
            }
            .plainRow(top: 8, bottom: 8)
            ForEach(waiting, id: \.request.id) { w in
                ApprovalCard(session: w.session, request: w.request) {
                    if sheet { dismiss() }
                    model.open(w.session)
                }
                .plainRow(top: 6, bottom: 6)
            }
            if waiting.isEmpty {
                VStack(spacing: 10) {
                    Image(systemName: "checkmark.shield")
                        .scaledFont(40)
                        .foregroundStyle(Color(p.success))
                    Text("Questions and approvals from your sessions show here.")
                        .scaledFont(15)
                        .foregroundStyle(Color(p.muted))
                        .multilineTextAlignment(.center)
                }
                .frame(maxWidth: .infinity)
                .padding(.top, 60)
                .plainRow()
            }
        }
        .listStyle(.plain)
        .screen()
        .toolbarVisibility(sheet ? .visible : .hidden, for: .navigationBar)
        .toolbar {
            if sheet {
                ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } }
            }
        }
        .refreshable { await model.reconnect() }
    }
}
