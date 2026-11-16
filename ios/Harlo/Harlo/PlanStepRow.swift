import HarloKit
import SwiftUI

/// A plan step: its mark, title and note, whether it runs with the step
/// before, who does it, and its commit.
struct PlanStepRow: View {
    @Environment(\.palette) private var p
    let step: PlanStep
    /// Done, and a reviewer has it.
    let reviewed: Bool

    var body: some View {
        let (icon, color) = mark
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: icon)
                .scaledFont(16)
                .foregroundStyle(Color(color))
                .padding(.top, 1)
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Text(step.title)
                        .scaledFont(15.5, step.status == "active" ? .semibold : .regular)
                        .foregroundStyle(Color(step.status == "skipped" ? p.muted : p.foreground))
                        .strikethrough(step.status == "skipped")
                    if step.editedByUser {
                        Text("yours").scaledFont(11.5).foregroundStyle(Color(p.muted))
                    }
                }
                let note = step.summary.isEmpty ? step.detail : step.summary
                if !note.isEmpty {
                    Text(note).scaledFont(13.5).foregroundStyle(Color(p.muted))
                }
                if step.alongside {
                    Label("With the step before", systemImage: "rectangle.split.2x1")
                        .scaledFont(12)
                        .foregroundStyle(Color(p.info))
                }
                if reviewed {
                    Label("Done, being reviewed", systemImage: "checkmark.seal")
                        .scaledFont(12)
                        .foregroundStyle(Color(p.info))
                        .lineLimit(1)
                } else if !step.helper.isEmpty {
                    Label((step.status == "active" ? "A helper is on it" : "Done by a helper") + (step.branch.isEmpty ? "" : " · \(step.branch)"), systemImage: "person.2")
                        .scaledFont(12)
                        .foregroundStyle(Color(p.muted))
                        .lineLimit(1)
                }
                // Its own helper (chosen on the desktop's plan card).
                if !step.agent.isEmpty, step.open {
                    Label("Done by \(Style.harnessLabel(step.agent))" + (step.model.isEmpty ? "" : " · \(step.model.split(separator: "/").last ?? "")"), systemImage: "person.crop.circle")
                        .scaledFont(12)
                        .foregroundStyle(Color(p.muted))
                        .lineLimit(1)
                }
                if !step.commit.isEmpty {
                    Label("Committed \(step.commit.prefix(8))", systemImage: "point.3.filled.connected.trianglepath.dotted")
                        .font(Style.mono(11.5))
                        .foregroundStyle(Color(p.success))
                } else if !step.commitNote.isEmpty {
                    Text(step.commitNote)
                        .scaledFont(12)
                        .foregroundStyle(Color(step.commitNote.hasPrefix("Not committed") ? p.warning : p.muted))
                }
            }
        }
        .padding(.vertical, 3)
        .accessibilityElement(children: .combine)
        .accessibilityValue(step.status == "active" ? "In hand" : step.status == "done" ? "Done" : step.status == "skipped" ? "Skipped" : "To do")
    }

    var mark: (String, RGBA) {
        switch step.status {
        case "done": return ("checkmark.circle.fill", p.success)
        case "active": return ("arrow.right.circle.fill", p.primary)
        case "skipped": return ("xmark.circle", p.muted)
        default: return ("circle", p.muted)
        }
    }
}
