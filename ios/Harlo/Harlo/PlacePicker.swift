import HarloKit
import SwiftUI

/// Where a new session starts: a project, or a machine's home folder (by
/// machine: this computer's, then each other one's), with its folder.
struct PlacePicker: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let choices: NewSessionChoices
    @Binding var place: SessionPlace
    /// The machine being connected to.
    let connecting: String?
    let text: String
    let path: String

    var body: some View {
        let c = choices
        let host = c.host(place)
        Menu {
            Picker("Where", selection: $place) {
                ForEach(c.machines, id: \.host) { m in
                    Section(title(m.host)) {
                        if m.host.isEmpty || (c.machine(m.host) != nil && model.canConnect) {
                            Label("Home folder", systemImage: "house").tag(SessionPlace.home(m.host))
                        }
                        ForEach(m.projects) { pr in
                            Label(pr.name, systemImage: "folder").tag(SessionPlace.project(pr.id))
                        }
                    }
                }
            }
        } label: {
            HStack(spacing: 8) {
                Group {
                    if connecting != nil {
                        ProgressView().controlSize(.mini)
                    } else {
                        Image(systemName: !host.isEmpty ? "server.rack" : c.project(place) == nil ? "house" : "folder")
                            .scaledFont(14, .medium)
                            .foregroundStyle(Color(p.primary))
                    }
                }
                .frame(width: 20)
                Text(text)
                    .scaledFont(15, .semibold)
                    .lineLimit(1)
                    .layoutPriority(1)
                Text(path)
                    .font(Style.mono(12))
                    // (The smallest words here: muted, but readable on the card.)
                    .foregroundStyle(Color(p.muted.legible(on: p.raised, toward: p.foreground)))
                    .lineLimit(1)
                    .truncationMode(.head)
                Spacer(minLength: 4)
                Image(systemName: "chevron.down")
                    .scaledFont(11, .semibold)
                    .foregroundStyle(Color(p.muted))
            }
            .foregroundStyle(Color(p.foreground))
            .padding(.horizontal, 4)
            .frame(minHeight: 32)
            .contentShape(Rectangle())
        }
        .disabled(connecting != nil)
        .accessibilityLabel("Where: \(text)")
    }

    /// A machine's section in the picker.
    func title(_ host: String) -> String {
        if host.isEmpty { return "This computer" }
        return choices.machine(host)?.connected == false ? "\(host) · connects first" : host
    }
}
