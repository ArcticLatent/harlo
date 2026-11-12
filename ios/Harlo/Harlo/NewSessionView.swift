import HarloKit
import SwiftUI

/// The New Session tab: a greeting, and a composer that starts a session in
/// harlo: where (a project, or a machine's home folder; another machine is
/// connected first), with which agent, model
/// and permissions, and its first message (with photos). The session then
/// opens in this tab.
struct NewSessionView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Binding var tab: AppTab
    @State private var choices: NewSessionChoices?
    @State private var loading = true
    /// A project, or a machine's home folder.
    @State private var place: SessionPlace = .home("")
    /// The machine being connected to.
    @State private var connecting: String?
    /// The agent's models on a stopped model server: model → its server.
    @State private var stopped: [String: String] = [:]
    @State private var agent = ""
    /// `provider/model`; "": the agent's default.
    @State private var modelId = ""
    /// The shield's mode; "": the one it starts in.
    @State private var approval = ""
    @State private var text = ""
    @State private var photos: [Photo] = []
    @State private var starting = false
    /// A template just changed the agent: keep its model and permissions.
    @State private var keepPicks = false
    @FocusState private var focused: Bool
    @Environment(\.dynamicTypeSize) private var typeSize
    /// At the largest text sizes the agent and model get a row of their
    /// own, above the buttons, so the model's name isn't cut.
    var big: Bool { typeSize.isAccessibilitySize }
    /// Your name, for the greeting (Settings).
    @AppStorage("yourName") private var name = ""

    var project: Project? { choices?.project(place) }
    /// The machine it starts on ("" harlo's computer).
    var host: String { choices?.host(place) ?? "" }

    var canStart: Bool {
        !starting && !agent.isEmpty && model.state == .connected && connecting == nil && choices?.needsConnecting(place) != true
            && (!text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !photos.isEmpty)
    }

    /// The place picker's text.
    var placeText: String {
        if let h = connecting { return "Connecting to \(h)…" }
        if let pr = project { return pr.host.isEmpty ? pr.name : "\(pr.name) · \(pr.host)" }
        return host.isEmpty ? "Home folder" : "Home · \(host)"
    }

    /// Where it starts, under the composer.
    var placePath: String {
        if let pr = project { return pr.location }
        guard !host.isEmpty else { return "~" }
        guard let home = choices?.machine(host)?.home, !home.isEmpty else { return "" }
        return home
    }

    /// Connects to the machine picked, then its agents are the choices
    /// (back to this computer if it doesn't connect: harlo says why).
    func connect(_ host: String) {
        connecting = host
        Task {
            let m = await model.connect(host)
            if m != nil, var c = await model.newSessionChoices() {
                // (A harlo that hasn't looked yet: what the connect said.)
                if let m, c.machine(host)?.connected != true { c.connected(m) }
                choices = c
                agent = c.agent(for: place)
            } else if m == nil {
                place = .home("")
            }
            connecting = nil
        }
    }

    /// "Default model", or the project's own for this agent (what harlo
    /// starts it with when none is picked).
    func defaultModel(_ c: NewSessionChoices) -> String {
        guard let own = project?.ownModels[agent], !own.isEmpty else { return "Default model" }
        return c.modelChoices(for: agent).first { $0.id == own }?.name ?? own
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                AppHeader { tab = .approvals }
                    .padding(.top, 8)
                VStack(alignment: .leading, spacing: 14) {
                    UserAvatar(size: 64)
                    greeting
                }
                .padding(.top, 72)
                if choices == nil && !loading {
                    Label("Connect to harlo to start a session.", systemImage: "wifi.exclamationmark")
                        .scaledFont(15)
                        .foregroundStyle(Color(p.warning))
                        .padding(.top, 24)
                }
            }
            // (The other tabs' margin, so the header lines up with theirs.)
            .padding(.horizontal, 16)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollDismissesKeyboard(.interactively)
        // Which of this computer's models wait on a stopped server.
        .task(id: "\(agent)|\(host)|\(choices != nil)") {
            guard let c = choices, host.isEmpty, !agent.isEmpty else { stopped = [:]; return }
            stopped = await model.stoppedModels(agent: agent, models: c.modelChoices(for: agent).map(\.id))
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if let c = choices { composer(c) }
        }
        .screen()
        .toolbar(.hidden, for: .navigationBar)
        // The choices: when it connects (again).
        .task(id: model.state == .connected) {
            guard model.state == .connected || choices == nil else { return }
            let c = await model.newSessionChoices()
            loading = false
            guard let c else { return }
            choices = c
            if c.project(place) == nil, case .project = place { place = .home("") }
            if agent.isEmpty || !c.agents(for: place).contains(where: { $0.id == agent }) { agent = c.agent(for: place) }
        }
    }

    /// "Good morning, Robin / What are we building today?"
    var greeting: some View {
        let first = name.trimmingCharacters(in: .whitespaces)
        return (Text(Style.greeting() + (first.isEmpty ? "" : ", "))
            + Text(first).foregroundColor(Color(p.primary))
            + Text("\nWhat are we building today?"))
            .scaledFont(32, .medium)
            .tracking(-0.3)
            .lineSpacing(4)
            .fixedSize(horizontal: false, vertical: true)
    }

    func composer(_ c: NewSessionChoices) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            if !c.templates.isEmpty { templates(c) }
            // As the session's composer: where it starts on top (a "To:"
            // line), the message, then photos, the agent and its model, what
            // runs without asking, dictation and start, all on one line (the
            // agent and model on a line of their own at the largest sizes).
            VStack(alignment: .leading, spacing: 8) {
                placeRow(c)
                Rectangle().fill(Color(p.hairline)).frame(height: 1)
                if !photos.isEmpty { PhotoStrip(photos: $photos) }
                TextField("What should the agent do?", text: $text, axis: .vertical)
                    .lineLimit(1...8)
                    .scaledFont(17)
                    .focused($focused)
                    .padding(.top, 2)
                if big {
                    HStack(spacing: 0) {
                        agentAndModel(c)
                        Spacer(minLength: 0)
                    }
                }
                HStack(spacing: 6) {
                    PhotoButton(photos: $photos, size: 34)
                    if !big { agentAndModel(c) }
                    permissions(c)
                    Spacer(minLength: 0)
                    MicButton(text: $text, size: 34)
                    Button(action: start) {
                        Group {
                            if starting {
                                ProgressView().tint(Color(p.onPrimary))
                            } else {
                                Image(systemName: "arrow.up").font(.system(size: 16, weight: .bold))
                            }
                        }
                        .frame(width: 36, height: 36)
                        .background(Circle().fill(Color(canStart || starting ? p.primary : p.subtle)))
                        .foregroundStyle(Color(canStart ? p.onPrimary : p.muted))
                    }
                    .buttonStyle(.plain)
                    .disabled(!canStart)
                    .accessibilityLabel("Start the session")
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .card(p, radius: 22, fill: p.background)
            if let server = stopped[modelId] {
                HStack(spacing: 4) {
                    Image(systemName: "pause.circle")
                    Text("\(server) is stopped: harlo starts it before your message")
                }
                .scaledFont(12.5)
                .foregroundStyle(Color(p.warning))
                .padding(.horizontal, 6)
            }
        }
        .padding(10)
        .card(p, radius: 26)
        .padding(.horizontal, 10)
        .padding(.bottom, 8)
        .onChange(of: place) { _, now in
            agent = c.agent(for: now)
            modelId = ""
            approval = ""
            if c.needsConnecting(now) && model.canConnect { connect(c.host(now)) }
        }
        .onChange(of: agent) { _, _ in
            if keepPicks { keepPicks = false; return }
            modelId = ""
            approval = ""
        }
    }

    /// Where it starts: a project, or a machine's home folder (by machine:
    /// this computer's, then each other one's), with its folder.
    func placeRow(_ c: NewSessionChoices) -> some View {
        Menu {
            Picker("Where", selection: $place) {
                ForEach(c.machines, id: \.host) { m in
                    Section(machineTitle(m.host, c)) {
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
                        Image(systemName: !host.isEmpty ? "server.rack" : project == nil ? "house" : "folder")
                            .scaledFont(14, .medium)
                            .foregroundStyle(Color(p.primary))
                    }
                }
                .frame(width: 20)
                Text(placeText)
                    .scaledFont(15, .semibold)
                    .lineLimit(1)
                    .layoutPriority(1)
                Text(placePath)
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
        .accessibilityLabel("Where: \(placeText)")
    }

    /// The agent's tile and the model's name; its menu picks either.
    /// (Used before first, then every model the agent offers; another
    /// machine's agent picks its own.)
    func agentAndModel(_ c: NewSessionChoices) -> some View {
        let agentName = c.agents.first { $0.id == agent }?.label ?? "Agent"
        let modelName = c.modelChoices(for: agent).first { $0.id == modelId }?.name ?? defaultModel(c)
        return Menu {
            Picker(selection: $agent) {
                ForEach(c.agents(for: place)) { a in
                    Text(a.label).tag(a.id)
                }
            } label: {
                Label("Agent: \(agentName)", systemImage: "person.crop.circle")
            }
            .pickerStyle(.menu)
            Picker(selection: $modelId) {
                Text(defaultModel(c)).tag("")
                ForEach(!host.isEmpty ? [] : c.modelChoices(for: agent), id: \.id) { m in
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
            .background(Capsule().fill(Color(p.raised.mix(p.foreground, 0.04))))
            .overlay(Capsule().strokeBorder(Color(p.hairline), lineWidth: 1))
            .foregroundStyle(Color(p.foreground))
            .contentShape(Capsule())
        }
        .accessibilityLabel("\(agentName), \(modelName)")
    }

    /// What runs without asking (agents that ask): a shield, lit when
    /// nothing asks first.
    @ViewBuilder
    func permissions(_ c: NewSessionChoices) -> some View {
        let starts = c.approval(for: agent, in: project)
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
                    .background(Circle().fill(Color(yolo ? p.accentSoft.over(p.raised) : p.raised.mix(p.foreground, 0.04))))
                    .overlay(Circle().strokeBorder(Color(yolo ? p.primary.mix(p.raised, 0.4) : p.hairline), lineWidth: 1))
                    .foregroundStyle(Color(yolo ? p.primary : p.muted))
            }
            .accessibilityLabel("Permissions: \(Modes.label(mode))")
        }
    }

    /// A machine's section in the place picker.
    func machineTitle(_ host: String, _ c: NewSessionChoices) -> String {
        if host.isEmpty { return "This computer" }
        return c.machine(host)?.connected == false ? "\(host) · connects first" : host
    }

    /// Templates saved in harlo: one fills the composer (send it when ready).
    func templates(_ c: NewSessionChoices) -> some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(c.templates) { t in
                    Button { apply(t, c) } label: {
                        HStack(spacing: 6) {
                            Image(systemName: "bookmark").scaledFont(12, .semibold).foregroundStyle(Color(p.primary))
                            Text(t.name).scaledFont(14, .medium).lineLimit(1)
                        }
                        .padding(.horizontal, 12)
                        .frame(minHeight: 32)
                        .background(Capsule().fill(Color(p.raised)))
                        .overlay(Capsule().strokeBorder(Color(p.hairline), lineWidth: 1))
                        .foregroundStyle(Color(p.foreground))
                    }
                    .buttonStyle(.plain)
                    .accessibilityHint("Fills in the agent, model, permissions and message")
                }
            }
            .padding(.horizontal, 2)
        }
    }

    func apply(_ t: SessionTemplate, _ c: NewSessionChoices) {
        let wanted = c.agents(for: place).contains { $0.id == t.agent } ? t.agent : c.agent(for: place)
        if wanted != agent { keepPicks = true }
        agent = wanted
        // (Another machine's agent picks its own model.)
        modelId = !host.isEmpty ? "" : t.model
        approval = c.approval(for: wanted, in: project).isEmpty ? "" : t.approval
        text = t.text
        focused = true
    }

    func start() {
        starting = true
        focused = false
        Task {
            let starts = choices?.approval(for: agent, in: project) ?? ""
            let ok = await model.startSession(project: project, host: host, agent: agent, model: modelId, approval: starts.isEmpty ? "" : approval,
                                              text: text, photos: photos)
            starting = false
            if ok {
                text = ""
                photos = []
                if let s = model.session { LiveActivities.start(for: s, model: model) }
            }
        }
    }
}
