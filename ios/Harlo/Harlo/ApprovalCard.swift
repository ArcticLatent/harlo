import HarloKit
import SwiftUI

/// A request on a card: the agent, how long it has waited, the session
/// (tap to open it), what it wants to do, and the answer (Deny / Approve,
/// or the question's own controls).
struct ApprovalCard: View {
    @Environment(\.palette) private var p
    let session: SessionSummary
    let request: UIRequest
    let open: () -> Void
    @State private var now = Date.now
    /// A refusal's reason is being typed.
    @State private var explaining = false

    /// "Allow tool: bash" → "bash".
    var tool: String { request.tool }

    /// What it would run or change ("bash: npm test" → "npm test").
    var command: String {
        let m = request.message.trimmingCharacters(in: .whitespacesAndNewlines)
        return !tool.isEmpty && m.hasPrefix(tool + ": ") ? String(m.dropFirst(tool.count + 2)) : m
    }

    /// Harlo on the computer words the approval itself (an older one
    /// sends only the tool and what it acts on).
    var worded: Bool { request.approval && !request.wants.isEmpty }

    var what: String {
        if worded { return request.asks }
        guard request.approval, !tool.isEmpty else { return request.title.isEmpty ? "The agent asks" : request.title }
        switch Style.toolSymbol(tool) {
        case "terminal": return "Wants to run a command"
        case "pencil.line": return "Wants to change a file"
        case "doc.text": return "Wants to read a file"
        case "globe": return "Wants to go online"
        case "chevron.left.forwardslash.chevron.right": return "Wants to run a program"
        default:
            if let m = ToolName.mcp(tool) { return "Wants to use \(m.server)'s \(m.tool.replacing("_", with: " "))" }
            return "Wants to use \(tool)"
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                AgentTile(maker: session.maker, size: 42)
                Text(Style.harnessLabel(session.harness))
                    .scaledFont(18, .semibold)
                Spacer(minLength: 6)
                if request.asked > 0 {
                    Text(Style.ago(request.asked / 1000, now: now) + (Style.ago(request.asked / 1000, now: now) == "now" ? "" : " ago"))
                        .scaledFont(14)
                        .foregroundStyle(Color(p.muted))
                }
            }
            Button(action: open) {
                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 6) {
                        Text(session.title.isEmpty ? "New session" : session.title)
                            .scaledFont(17, .semibold)
                            .multilineTextAlignment(.leading)
                            .lineLimit(2)
                        Image(systemName: "chevron.right")
                            .scaledFont(12, .semibold)
                            .foregroundStyle(Color(p.muted))
                    }
                    Text(what)
                        .scaledFont(15)
                        .foregroundStyle(Color(p.muted))
                    // Why it asks (Harlo's: a destructive command, one
                    // outside the sandbox).
                    if request.approval && !request.note.isEmpty {
                        Label(request.note, systemImage: "exclamationmark.triangle")
                            .scaledFont(14, .medium)
                            .foregroundStyle(Color(p.warning))
                    }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityHint("Opens the session")
            if worded {
                ApprovalDetails(request: request, brief: true)
            } else if request.approval && !command.isEmpty {
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: tool.isEmpty ? "doc.text" : Style.toolSymbol(tool))
                        .scaledFont(14)
                        .foregroundStyle(Color(p.muted))
                        .padding(.top, 1)
                    Text(command)
                        .font(Style.mono(13.5))
                        .lineLimit(5)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .textSelection(.enabled)
                }
                .padding(12)
                .background { RoundedRectangle(cornerRadius: 10).fill(Color(p.codeBackground)) }
            }
            if request.approval || request.method == "confirm" {
                ApprovalButtons(request: request, key: session.key, explaining: $explaining)
            } else {
                RequestCard(request: request, key: session.key, open: open, bare: true)
            }
        }
        .padding(16)
        .card(p, radius: 18)
    }
}
