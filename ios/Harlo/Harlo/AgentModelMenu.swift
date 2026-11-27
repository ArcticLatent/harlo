import HarloKit
import SwiftUI

/// A new session's model, with its provider's tile; its menu picks the
/// agent or the model.
/// (Used before first, then every model the agent offers; another
/// machine's agent picks its own.)
struct AgentModelMenu: View {
    @Environment(\.palette) private var p
    let choices: NewSessionChoices
    let place: SessionPlace
    @Binding var agent: String
    @Binding var modelId: String
    /// "Default model", or the project's own for this agent.
    let defaultModel: String
    /// At the largest text sizes: a row of its own, the name uncut.
    let big: Bool

    var body: some View {
        let c = choices
        let remote = !c.host(place).isEmpty
        let agentName = c.agents.first { $0.id == agent }?.label ?? "Agent"
        let modelName = c.modelChoices(for: agent).first { $0.id == modelId }?.name ?? defaultModel
        Menu {
            Picker(selection: $agent) {
                ForEach(c.agents(for: place)) { a in
                    Text(a.label).tag(a.id)
                }
            } label: {
                Label("Agent: \(agentName)", systemImage: "person.crop.circle")
            }
            .pickerStyle(.menu)
            Picker(selection: $modelId) {
                Text(defaultModel).tag("")
                ForEach(remote ? [] : c.modelChoices(for: agent), id: \.id) { m in
                    Text(m.name).tag(m.id)
                }
            } label: {
                Label("Model: \(modelName)", systemImage: "cpu")
            }
            .pickerStyle(.menu)
        } label: {
            HStack(spacing: 6) {
                AgentTile(maker: c.maker(of: modelId), size: 20)
                Text(modelName)
                    .scaledFont(13.5, .medium)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .frame(maxWidth: big ? nil : 130, alignment: .leading)
                    .fixedSize(horizontal: !big, vertical: false)
                Image(systemName: "chevron.down").scaledFont(9, .bold).foregroundStyle(Color(p.muted))
            }
            .padding(.leading, 6)
            .padding(.trailing, 10)
            .frame(minHeight: 34)
            .background { RoundedRectangle(cornerRadius: Corner.small).fill(Color(p.raised.mix(p.foreground, 0.04))) }
            .overlay { RoundedRectangle(cornerRadius: Corner.small).strokeBorder(Color(p.hairline), lineWidth: 1) }
            .foregroundStyle(Color(p.foreground))
            .contentShape(RoundedRectangle(cornerRadius: Corner.small))
        }
        .accessibilityLabel("\(agentName), \(modelName)")
    }
}
