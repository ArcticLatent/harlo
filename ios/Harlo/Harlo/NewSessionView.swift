import HarloKit
import SwiftUI

/// The New Session tab: a greeting, and a composer that starts a session in
/// harlo: where (a project, or the home folder), with which agent, model
/// and permissions, and its first message (with photos). The session then
/// opens in this tab.
struct NewSessionView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Binding var tab: AppTab
    @State private var choices: NewSessionChoices?
    @State private var loading = true
    /// The project's id; -1: the home folder.
    @State private var projectId: Int64 = -1
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

    var project: Project? { choices?.projects.first { $0.id == projectId } }

    var canStart: Bool {
        !starting && !agent.isEmpty && model.state == .connected
            && (!text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !photos.isEmpty)
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
            if agent.isEmpty || !c.agents.contains(where: { $0.id == agent }) { agent = c.agent(for: project) }
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
            // Where, which agent and model, what runs without asking.
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    Menu {
                        // By machine: this computer's, then each remote host's.
                        Picker("Project", selection: $projectId) {
                            Label("Home folder", systemImage: "house").tag(Int64(-1))
                            ForEach(c.machines, id: \.host) { m in
                                Section(m.host.isEmpty ? "This computer" : m.host) {
                                    ForEach(m.projects) { pr in
                                        Label(pr.name, systemImage: m.host.isEmpty ? "folder" : "server.rack").tag(pr.id)
                                    }
                                }
                            }
                        }
                    } label: {
                        PickerChip(text: project.map { $0.host.isEmpty ? $0.name : "\($0.name) · \($0.host)" } ?? "Home folder") {
                            Image(systemName: project == nil ? "house" : project?.host.isEmpty == false ? "server.rack" : "folder").font(.system(size: 13))
                        }
                    }
                    .onChange(of: projectId) { _, _ in
                        agent = c.agent(for: project)
                        modelId = ""
                        approval = ""
                    }
                    Menu {
                        Picker("Agent", selection: $agent) {
                            ForEach(c.agents(for: project)) { a in
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
                            Text("Default model").tag("")
                            ForEach(project?.host.isEmpty == false ? [] : c.modelChoices(for: agent), id: \.id) { m in
                                Text(m.name).tag(m.id)
                            }
                        }
                    } label: {
                        PickerChip(text: c.modelChoices(for: agent).first { $0.id == modelId }?.name ?? "Default model") {
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
            }
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
            if let pr = project {
                Text(pr.location)
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
        let wanted = c.agents(for: project).contains { $0.id == t.agent } ? t.agent : c.agent(for: project)
        if wanted != agent { keepPicks = true }
        agent = wanted
        // (Another machine's agent picks its own model.)
        modelId = project?.host.isEmpty == false ? "" : t.model
        approval = c.approval(for: wanted, in: project).isEmpty ? "" : t.approval
        text = t.text
        focused = true
    }

    func start() {
        starting = true
        focused = false
        Task {
            let starts = choices?.approval(for: agent, in: project) ?? ""
            let ok = await model.startSession(project: project, agent: agent, model: modelId, approval: starts.isEmpty ? "" : approval,
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
