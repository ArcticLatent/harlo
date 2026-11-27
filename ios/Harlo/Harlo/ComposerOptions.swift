import HarloKit
import SwiftUI

/// The thinking level and approval mode, in one menu, and while it works
/// whether a message steers the run or waits for it: its icon lights up
/// while nothing asks first.
struct ComposerOptions: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let session: OpenSession
    /// The thinking levels its model has.
    let levels: [String]

    var body: some View {
        let m = session.modes
        let approvals = m?.approvals ?? []
        if session.busy || !levels.isEmpty || !approvals.isEmpty {
            let yolo = m?.approval == "yolo"
            Menu {
                if session.busy {
                    Picker(selection: Bindable(session).steer) {
                        Text("Steer the current run").tag(true)
                        Text("Queue for after it").tag(false)
                    } label: {
                        Label("Messages: \(session.steer ? "Steer" : "Queue")", systemImage: session.steer ? "arrow.turn.down.right" : "text.line.last.and.arrowtriangle.forward")
                    }
                    .pickerStyle(.menu)
                }
                if !levels.isEmpty {
                    Picker(selection: Binding(get: { session.info.thinking }, set: { l in Task { await model.setThinking(l) } })) {
                        ForEach(levels, id: \.self) { Text($0.capitalized).tag($0) }
                    } label: {
                        Label("Thinking: \(session.info.thinking.capitalized)", systemImage: "brain")
                    }
                    .pickerStyle(.menu)
                }
                if let m, !approvals.isEmpty {
                    Picker(selection: Binding(get: { m.approval }, set: { a in Task { await model.setApproval(a) } })) {
                        ForEach(approvals, id: \.self) { Text(Modes.label($0)).tag($0) }
                    } label: {
                        Label("Approvals: \(Modes.short(m.approval))", systemImage: yolo ? "bolt.shield" : "shield")
                    }
                    .pickerStyle(.menu)
                }
            } label: {
                Image(systemName: yolo ? "bolt.shield" : "slider.horizontal.3")
                    .font(.system(size: 15, weight: .medium))
                    .frame(width: 34, height: 34)
                    .background { RoundedRectangle(cornerRadius: Corner.card).fill(Color(yolo ? p.accentSoft.over(p.raised) : p.raised.mix(p.foreground, 0.04))) }
                    .overlay { RoundedRectangle(cornerRadius: Corner.card).strokeBorder(Color(yolo ? p.primary.mix(p.raised, 0.4) : p.hairline), lineWidth: 1) }
                    .foregroundStyle(Color(yolo ? p.primary : p.muted))
            }
            .accessibilityLabel("Options: \(session.busy ? (session.steer ? "messages steer the run, " : "messages wait for the run, ") : "")thinking \(session.info.thinking), approvals \(Modes.label(m?.approval ?? ""))")
        }
    }
}
