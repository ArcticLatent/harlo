import HarloKit
import SwiftUI

struct ActionLogRow: View {
    @Environment(\.palette) private var p
    let entry: ActionLog.Entry

    var symbol: String {
        switch entry.kind {
        case "command": return "terminal"
        case "edit": return "pencil"
        case "write": return "doc.badge.plus"
        case "read": return "eye"
        case "search": return "magnifyingglass"
        case "web": return "globe"
        case "agent": return "person.2"
        default: return "sparkles"
        }
    }

    var detail: String {
        let how = entry.running ? "running…" : entry.failed ? (entry.result.isEmpty || entry.result == "failed" ? "failed" : "failed · \(entry.result)") : entry.result
        return [entry.tool, how].filter { !$0.isEmpty }.joined(separator: " · ")
    }

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: symbol)
                .scaledFont(13, .semibold)
                .foregroundStyle(Color(entry.failed ? p.error : p.muted))
                .frame(width: 20)
                .padding(.top, 2)
            VStack(alignment: .leading, spacing: 3) {
                Text(entry.target.isEmpty ? entry.tool : entry.target)
                    .font(Style.mono(13))
                    .lineLimit(entry.isFile ? 1 : 3)
                    .truncationMode(entry.isFile ? .head : .tail)
                Text(detail)
                    .scaledFont(12)
                    .foregroundStyle(Color(entry.failed ? p.error : p.muted))
            }
            Spacer(minLength: 4)
            if let t = entry.time {
                Text(Date(timeIntervalSince1970: Double(t) / 1000).formatted(date: .omitted, time: .shortened))
                    .scaledFont(11.5)
                    .foregroundStyle(Color(p.muted))
            }
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .combine)
    }
}
