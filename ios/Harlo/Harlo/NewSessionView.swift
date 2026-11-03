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
        guard !host.isEmpty, let home = choices?.machine(host)?.home, !home.isEmpty else { return "" }
        return "\(host): \(home)"
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
                        .font(.system(size: 15))
                        .foregroundStyle(Color(p.warning))
                        .padding(.top, 24)
                }
            }
            .padding(.horizontal, 20)
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
            .font(.system(size: 32, weight: .medium))
            .tracking(-0.3)
            .lineSpacing(4)
            .fixedSize(horizontal: false, vertical: true)
    }

    func composer(_ c: NewSessionChoices) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            if !c.templates.isEmpty { templates(c) }
            // Where, which agent and model, what runs without asking (onto a
            // second line when they don't fit, all in sight).
            Flow(spacing: 8) {
                    Menu {
                        // By machine: this computer's, then each other one's
                        // (its home folder too, from a harlo that starts there).
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
                        PickerChip(text: placeText) {
                            if connecting != nil {
                                ProgressView().controlSize(.mini)
                            } else {
                                Image(systemName: !host.isEmpty ? "server.rack" : project == nil ? "house" : "folder").font(.system(size: 13))
                            }
                        }
                    }
                    .disabled(connecting != nil)
                    .onChange(of: place) { _, now in
                        agent = c.agent(for: now)
                        modelId = ""
                        approval = ""
                        if c.needsConnecting(now) && model.canConnect { connect(c.host(now)) }
                    }
                    Menu {
                        Picker("Agent", selection: $agent) {
                            ForEach(c.agents(for: place)) { a in
                                Text(a.label).tag(a.id)
                            }
                        }
                    } label: {
                        PickerChip(text: c.agents.first { $0.id == agent }?.label ?? "Agent") {
                            AgentTile(harness: agent, size: 22)
                        }
                    }
                    .onChange(of: agent) { _, _ in
                        if keepPicks { keepPicks = false; return }
                        modelId = ""
                        approval = ""
                    }
                    // Used before first, then every model the agent offers.
                    Menu {
                        Picker("Model", selection: $modelId) {
                            Text(defaultModel(c)).tag("")
                            ForEach(!host.isEmpty ? [] : c.modelChoices(for: agent), id: \.id) { m in
                                Text(stopped[m.id].map { "\(m.name) (\($0) stopped)" } ?? m.name).tag(m.id)
                            }
                        }
                    } label: {
                        PickerChip(text: c.modelChoices(for: agent).first { $0.id == modelId }?.name ?? defaultModel(c)) {
                            Image(systemName: "cpu").font(.system(size: 13))
                        }
                    }
                    let starts = c.approval(for: agent, in: project)
                    if !starts.isEmpty {
                        Menu {
                            Picker("Permissions", selection: Binding(get: { approval.isEmpty ? starts : approval }, set: { approval = $0 })) {
                                ForEach(["always-ask", "write", "yolo"], id: \.self) { m in
                                    Text(Modes.label(m)).tag(m)
                                }
                            }
                        } label: {
                            PickerChip(text: Modes.short(approval.isEmpty ? starts : approval)) {
                                Image(systemName: (approval.isEmpty ? starts : approval) == "yolo" ? "bolt.shield" : "shield").font(.system(size: 13))
                            }
                        }
                    }
            }
            .padding(.horizontal, 2)
            VStack(alignment: .leading, spacing: 10) {
                if !photos.isEmpty { PhotoStrip(photos: $photos) }
                TextField("What should the agent do?", text: $text, axis: .vertical)
                    .lineLimit(1...8)
                    .font(.system(size: 17))
                    .focused($focused)
                    .padding(.top, 4)
                HStack {
                    PhotoButton(photos: $photos, size: 40)
                    Spacer()
                    MicButton(text: $text, size: 40)
                    Button(action: start) {
                        Group {
                            if starting {
                                ProgressView().tint(Color(p.onPrimary))
                            } else {
                                Image(systemName: "arrow.right").font(.system(size: 18, weight: .bold))
                            }
                        }
                        .frame(width: 48, height: 44)
                        .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color(canStart || starting ? p.primary : p.subtle)))
                        .foregroundStyle(Color(canStart ? p.onPrimary : p.muted))
                    }
                    .buttonStyle(.plain)
                    .disabled(!canStart)
                    .accessibilityLabel("Start the session")
                }
            }
            .padding(12)
            .card(p, radius: 18, fill: p.background)
            if let server = stopped[modelId] {
                HStack(spacing: 4) {
                    Image(systemName: "pause.circle")
                    Text("\(server) is stopped: harlo starts it before your message")
                }
                    .font(.system(size: 12.5))
                    .foregroundStyle(Color(p.warning))
                    .padding(.horizontal, 4)
            }
            if !placePath.isEmpty {
                Text(placePath)
                    .font(Style.mono(11))
                    .foregroundStyle(Color(p.faint))
                    .lineLimit(1)
                    .truncationMode(.head)
                    .padding(.horizontal, 4)
            }
        }
        .padding(10)
        .card(p, radius: 24)
        .padding(.horizontal, 10)
        .padding(.bottom, 8)
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
                            Image(systemName: "bookmark").font(.system(size: 12, weight: .semibold)).foregroundStyle(Color(p.primary))
                            Text(t.name).font(.system(size: 14, weight: .medium)).lineLimit(1)
                        }
                        .padding(.horizontal, 12)
                        .frame(height: 32)
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

/// A picker's chip: an icon (or a tile), what's chosen, and a chevron.
struct PickerChip<Lead: View>: View {
    @Environment(\.palette) private var p
    let text: String
    let lead: Lead

    init(text: String, @ViewBuilder lead: () -> Lead) {
        self.text = text
        self.lead = lead()
    }

    var body: some View {
        HStack(spacing: 7) {
            lead
            Text(text)
                .font(.system(size: 15, weight: .medium))
                .lineLimit(1)
                .truncationMode(.middle)
                .frame(maxWidth: 170, alignment: .leading)
                .fixedSize(horizontal: true, vertical: false)
            Image(systemName: "chevron.down")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(Color(p.muted))
        }
        .padding(.leading, 8)
        .padding(.trailing, 11)
        .frame(height: 38)
        .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color(p.raised)))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Color(p.hairline), lineWidth: 1))
        .foregroundStyle(Color(p.foreground))
        .contentShape(RoundedRectangle(cornerRadius: 12))
    }
}
