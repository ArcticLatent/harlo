import HarloKit
import SwiftUI

/// What runs without asking in a new session (agents that ask): a shield,
/// lit when nothing asks first.
struct PermissionsMenu: View {
    @Environment(\.palette) private var p
    /// The mode the agent starts in ("": it doesn't ask).
    let starts: String
    /// The mode picked; "": the one it starts in.
    @Binding var approval: String

    var body: some View {
        if !starts.isEmpty {
            let mode = approval.isEmpty ? starts : approval
            let yolo = mode == "yolo"
            Menu {
                Picker("Permissions", selection: Binding(get: { mode }, set: { approval = $0 })) {
                    ForEach(["always-ask", "write", "yolo"], id: \.self) { m in
                        Text(Modes.label(m)).tag(m)
                    }
                }
            } label: {
                Image(systemName: yolo ? "bolt.shield" : "shield")
                    .font(.system(size: 15, weight: .medium))
                    .frame(width: 34, height: 34)
                    .background { RoundedRectangle(cornerRadius: Corner.card).fill(Color(yolo ? p.accentSoft.over(p.raised) : p.raised.mix(p.foreground, 0.04))) }
                    .overlay { RoundedRectangle(cornerRadius: Corner.card).strokeBorder(Color(yolo ? p.primary.mix(p.raised, 0.4) : p.hairline), lineWidth: 1) }
                    .foregroundStyle(Color(yolo ? p.primary : p.muted))
            }
            .accessibilityLabel("Permissions: \(Modes.label(mode))")
        }
    }
}
