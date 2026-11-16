import HarloKit
import SwiftUI

/// The thinking level, approval mode and Plan, in one menu: its icon
/// lights up while Plan is on or nothing asks first.
struct ComposerOptions: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let session: OpenSession
    /// The thinking levels its model has.
    let levels: [String]

    var body: some View {
        let m = session.modes
        let approvals = m?.approvals ?? []
        if !levels.isEmpty || !approvals.isEmpty {
            let plan = m?.plan == true, yolo = m?.approval == "yolo"
            Menu {
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
                    Toggle(isOn: Binding(get: { m.plan }, set: { on in Task { await model.setPlan(on) } })) {
                        Label("Plan first", systemImage: "list.bullet.clipboard")
                    }
                }
            } label: {
                Image(systemName: plan ? "list.bullet.clipboard" : yolo ? "bolt.shield" : "slider.horizontal.3")
                    .font(.system(size: 15, weight: .medium))
                    .frame(width: 34, height: 34)
                    .background { Circle().fill(Color(plan || yolo ? p.accentSoft.over(p.raised) : p.raised.mix(p.foreground, 0.04))) }
                    .overlay { Circle().strokeBorder(Color(plan || yolo ? p.primary.mix(p.raised, 0.4) : p.hairline), lineWidth: 1) }
                    .foregroundStyle(Color(plan || yolo ? p.primary : p.muted))
            }
            .accessibilityLabel("Options: thinking \(session.info.thinking), approvals \(Modes.label(m?.approval ?? "")), Plan \(plan ? "on" : "off")")
        }
    }
}
