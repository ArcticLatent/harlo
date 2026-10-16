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
                        .font(.system(size: 40))
                        .foregroundStyle(Color(p.success))
                    Text("Questions and approvals from your sessions show here.")
                        .font(.system(size: 15))
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
        .toolbar(sheet ? .visible : .hidden, for: .navigationBar)
        .toolbar {
            if sheet {
                ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } }
            }
        }
        .refreshable { await model.reconnect() }
    }
}

/// A sheet of the other sessions' requests (a session's toolbar opens it).
struct InboxView: View {
    let except: [String]

    var body: some View {
        NavigationStack {
            ApprovalsView(except: except, sheet: true)
        }
    }
}

/// A request on a card: the agent, how long it has waited, the session
/// (tap to open it), what it wants to do, and the answer (Deny / Approve,
/// or the question's own controls).
struct ApprovalCard: View {
    @Environment(\.palette) private var p
    let session: SessionSummary
    let request: UIRequest
    let open: () -> Void
    @State private var now = Date()

    /// "Allow tool: bash" → "bash".
    var tool: String {
        request.title.hasPrefix("Allow tool: ") ? String(request.title.dropFirst("Allow tool: ".count)) : ""
    }

    /// What it would run or change ("bash: npm test" → "npm test").
    var command: String {
        let m = request.message.trimmingCharacters(in: .whitespacesAndNewlines)
        return !tool.isEmpty && m.hasPrefix(tool + ": ") ? String(m.dropFirst(tool.count + 2)) : m
    }

    var what: String {
        guard request.approval, !tool.isEmpty else { return request.title.isEmpty ? "The agent asks" : request.title }
        switch Style.toolSymbol(tool) {
        case "terminal": return "Wants to run a command"
        case "pencil.line": return "Wants to change a file"
        case "doc.text": return "Wants to read a file"
        case "globe": return "Wants to go online"
        default: return "Wants to use \(tool)"
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                AgentTile(harness: session.harness, size: 42)
                Text(Style.harnessLabel(session.harness))
                    .font(.system(size: 18, weight: .semibold))
                Spacer(minLength: 6)
                if request.asked > 0 {
                    Text(Style.ago(request.asked / 1000, now: now) + (Style.ago(request.asked / 1000, now: now) == "now" ? "" : " ago"))
                        .font(.system(size: 14))
                        .foregroundStyle(Color(p.muted))
                }
            }
            Button(action: open) {
                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 6) {
                        Text(session.title.isEmpty ? "New session" : session.title)
                            .font(.system(size: 17, weight: .semibold))
                            .multilineTextAlignment(.leading)
                            .lineLimit(2)
                        Image(systemName: "chevron.right")
                            .font(.system(size: 12, weight: .semibold))
                            .foregroundStyle(Color(p.muted))
                    }
                    Text(what)
                        .font(.system(size: 15))
                        .foregroundStyle(Color(p.muted))
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityHint("Opens the session")
            if request.approval && !command.isEmpty {
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: tool.isEmpty ? "doc.text" : Style.toolSymbol(tool))
                        .font(.system(size: 14))
                        .foregroundStyle(Color(p.muted))
                        .padding(.top, 1)
                    Text(command)
                        .font(Style.mono(13.5))
                        .lineLimit(5)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .textSelection(.enabled)
                }
                .padding(12)
                .background(RoundedRectangle(cornerRadius: 10).fill(Color(p.codeBackground)))
            }
            if request.approval || request.method == "confirm" {
                ApprovalButtons(request: request, key: session.key)
            } else {
                RequestCard(request: request, key: session.key, open: open, bare: true)
            }
        }
        .padding(16)
        .card(p, radius: 18)
    }
}
