import HarloKit
import SwiftUI

/// What a session has going beside its conversation: commands and
/// subagents in the background, its goal. Each row says where it stands
/// and offers the one thing to do to it (Stop, Pause, Resume, Dismiss).
struct JobRows: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let rows: [JobRow]
    /// Rows whose action is on its way to harlo.
    @State private var acting: Set<String> = []

    var body: some View {
        ForEach(rows) { row in
            HStack(spacing: 8) {
                Image(systemName: row.symbol.isEmpty ? "circle.dashed" : row.symbol)
                    .scaledFont(12)
                    .foregroundStyle(Color(row.working ? p.primary : p.muted))
                    .frame(width: 20)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 1) {
                    HStack(spacing: 6) {
                        Text(row.title)
                            .scaledFont(12, .semibold)
                            .foregroundStyle(Color(p.primary))
                        Text(row.note)
                            .scaledFont(12)
                            .foregroundStyle(Color(p.muted))
                            .lineLimit(1)
                    }
                    Text(row.text)
                        .scaledFont(13)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    if !row.detail.isEmpty {
                        Text(row.detail)
                            .scaledFont(12)
                            .foregroundStyle(Color(p.muted))
                            .lineLimit(2)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(row.spoken)
                if !row.action.isEmpty {
                    Button {
                        act(row)
                    } label: {
                        Text(row.action)
                            .scaledFont(13, .medium)
                            .foregroundStyle(Color(row.action == "Stop" ? p.error : p.primary))
                            .frame(minWidth: 44, minHeight: 32)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(acting.contains(row.id))
                    .opacity(acting.contains(row.id) ? 0.55 : 1)
                    .accessibilityLabel("\(row.action): \(row.text)")
                }
            }
        }
    }

    func act(_ row: JobRow) {
        acting.insert(row.id)
        Task {
            await model.act(on: row)
            acting.remove(row.id)
        }
    }
}
