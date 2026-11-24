import HarloKit
import SwiftUI

/// A new session's agent tile and model name; its menu picks either.
/// (Used before first, then every model the agent offers; another
/// machine's agent picks its own.)
struct AgentModelMenu: View {
    @Environment(\.palette) private var p
    let choices: NewSessionChoices
    let place: SessionPlace
    @Binding var agent: String
    @Binding var modelId: String
    /// The agent's models on a stopped model server: model → its server.
    let stopped: [String: String]
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
                    Text(stopped[m.id].map { "\(m.name) (\($0) stopped)" } ?? m.name).tag(m.id)
                }
            } label: {
                Label("Model: \(modelName)", systemImage: "cpu")
            }
            .pickerStyle(.menu)
        } label: {
            HStack(spacing: 6) {
                AgentTile(harness: agent, size: 20)
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
            .background { Capsule().fill(Color(p.raised.mix(p.foreground, 0.04))) }
            .overlay { Capsule().strokeBorder(Color(p.hairline), lineWidth: 1) }
            .foregroundStyle(Color(p.foreground))
            .contentShape(Capsule())
        }
        .accessibilityLabel("\(agentName), \(modelName)")
    }
}
