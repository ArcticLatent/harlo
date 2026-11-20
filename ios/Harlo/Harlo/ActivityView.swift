import HarloKit
import SwiftUI

/// The session's todo list, by phase.
struct ActivityView: View {
    @Environment(\.palette) private var p
    @Environment(\.dismiss) private var dismiss
    let session: OpenSession

    var body: some View {
        NavigationStack {
            List {
                ForEach(session.todos) { phase in
                    Section {
                        ForEach(phase.tasks) { t in
                            HStack(alignment: .firstTextBaseline, spacing: 10) {
                                Image(systemName: icon(t.status))
                                    .foregroundStyle(Color(color(t.status)))
                                Text(t.content)
                                    .scaledFont(15)
                                    .strikethrough(t.status == "completed", color: Color(p.muted))
                                    .foregroundStyle(Color(t.status == "completed" ? p.muted : p.foreground))
                            }
                            .listRowBackground(Color(p.elevated))
                        }
                    } header: {
                        HStack {
                            Text(phase.name)
                            Spacer()
                            Text("\(phase.done)/\(phase.tasks.count)")
                        }
                        .font(Style.mono(11.5, .semibold))
                        .foregroundStyle(Color(p.muted))
                        .textCase(nil)
                    }
                }
            }
            .screen()
            .navigationTitle("Todos")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } }
            }
        }
    }

    func icon(_ status: String) -> String {
        switch status {
        case "completed": return "checkmark.circle.fill"
        case "in_progress": return "circle.dotted.circle"
        case "blocked": return "exclamationmark.circle"
        default: return "circle"
        }
    }

    func color(_ status: String) -> RGBA {
        switch status {
        case "completed": return p.success
        case "in_progress": return p.primary
        case "blocked": return p.warning
        default: return p.muted
        }
    }
}
