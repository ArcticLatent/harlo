import HarloKit
import SwiftUI
import UserNotifications

struct SettingsView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.dismiss) private var dismiss
    @State private var scanning = false
    @State private var forgetting = false
    /// Whether iOS lets the app notify (nil: not read yet).
    @State private var allowed: UNAuthorizationStatus?
    @State private var liveActivities = LiveActivities.enabled
    /// A tab (no Done button), or a sheet.
    var inTab = false
    /// Your name, for the New Session greeting.
    @AppStorage("yourName") private var name = ""
    @AppStorage("undoTurns") private var undoOn = true
    @AppStorage("reviewComments") private var commentsOn = true
    @AppStorage("dictationLanguage") private var dictationLanguage = ""

    /// The six penguins; the one you have ringed. Picking one needs harlo
    /// (it keeps it), or the demo.
    var penguins: some View {
        let current = (model.theme ?? .standard).penguin
        let can = model.demo || model.state == .connected
        return HStack(spacing: 0) {
            ForEach(Theme.avatars, id: \.self) { name in
                Button {
                    Task { await model.setAvatar(name) }
                } label: {
                    Image("Penguin-" + name)
                        .resizable()
                        .interpolation(.high)
                        .scaledToFit()
                        .frame(width: 40, height: 40)
                        .clipShape(Circle())
                        .padding(3)
                        .overlay(Circle().stroke(Color(p.primary), lineWidth: name == current ? 2 : 0))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Penguin, \(name)")
                .accessibilityAddTraits(name == current ? .isSelected : [])
                .frame(maxWidth: .infinity)
            }
        }
        .disabled(!can)
        .opacity(can ? 1 : 0.5)
        .padding(.vertical, 2)
    }

    var stateText: String {
        switch model.state {
        case .connected: return "Connected"
        case .connecting: return "Connecting…"
        case .waiting(let why): return why.isEmpty ? "Not connected" : why
        case .unpaired: return "Not paired"
        }
    }

    var body: some View {
        NavigationStack {
            Form {
                Group {
                // A tab: the app's header and a big title, as the other tabs.
                if inTab {
                    Section {
                        VStack(alignment: .leading, spacing: 22) {
                            AppHeader()
                            BigTitle(text: "Settings")
                        }
                        .listRowBackground(Color.clear)
                        .listRowInsets(EdgeInsets(top: 8, leading: 4, bottom: 0, trailing: 4))
                    }
                }
                Section {
                    HStack(spacing: 14) {
                        UserAvatar(size: 56)
                        VStack(alignment: .leading, spacing: 6) {
                            TextField("Your name", text: $name)
                                .font(.system(size: 17, weight: .medium))
                                .textContentType(.givenName)
                        }
                    }
                    .padding(.vertical, 4)
                    penguins
                } header: {
                    Text("You")
                } footer: {
                    Text("Your name is only on this phone, for the greeting on New Session. Your penguin is harlo's too: picking one here changes it on your computer.")
                }
                Section {
                    Picker("Language", selection: $dictationLanguage) {
                        ForEach(Dictation.languages, id: \.code) { l in
                            Text(l.name).tag(l.code)
                        }
                    }
                } header: {
                    Text("Dictation")
                } footer: {
                    Text("Tap the mic beside a message and speak: the words go in as you say them. Recognized on the iPhone itself when it has the language (else by Apple's speech recognition); nothing is kept.")
                }
                Section {
                    Toggle("Undo a turn", isOn: $undoOn)
                    Toggle("Comments on changes", isOn: $commentsOn)
                } header: {
                    Text("Reviewing")
                } footer: {
                    Text("Undo: long-press the agent's reply (or tap ↶ next to View diff) to put the folder's files back as they were before it (harlo on your computer keeps the snapshots; its Settings → Git turns them off). Comments: tap a line of a file's changes to comment on it, then send them all to the agent.")
                }
                if model.demo {
                    Section {
                        Button("Leave the demo") { model.endDemo() }
                    } header: {
                        Text("Demo")
                    } footer: {
                        Text("Sample sessions; nothing is connected or saved. Leaving it goes back to pairing with harlo on your computer.")
                    }
                } else if let host = model.host {
                    Section {
                        LabeledContent("Computer", value: host.name)
                        LabeledContent("Status", value: stateText)
                        LabeledContent("harlo's key", value: fingerprint(host.key))
                            .font(Style.mono(13))
                        LabeledContent("Colours", value: (model.theme ?? .standard).family)
                    } header: {
                        Text("harlo")
                    }
                    Section {
                        ForEach(host.endpoints, id: \.self) { url in
                            HStack {
                                Text(Style.endpointLabel(url))
                                Spacer()
                                if url == host.lastEndpoint {
                                    Image(systemName: "checkmark").foregroundStyle(Color(p.primary))
                                }
                            }
                        }
                        Button("Add an address…") { scanning = true }
                    } header: {
                        Text("Addresses")
                    } footer: {
                        Text("harlo is tried at each, the last one that worked first. To reach it away from home, scan its QR code again with Tailscale or the relay picked in Pair a device.")
                    }
                }
                if model.host != nil && !model.demo {
                    notifications
                }
                Section {
                    LabeledContent("This phone's key", value: model.fingerprint)
                        .font(Style.mono(13))
                } footer: {
                    Text("harlo lists this phone in Settings → Devices; removing it there unpairs it.")
                }
                if !model.demo {
                    Section {
                        Button("Forget harlo", role: .destructive) { forgetting = true }
                    }
                }
                Section {
                    LabeledContent("Version", value: Style.version)
                    Link("Help and support", destination: URL(string: "https://harlo.run/support")!)
                    Link("Privacy policy", destination: URL(string: "https://harlo.run/privacy")!)
                }
                }
                .listRowBackground(Color(p.elevated))
            }
            .screen()
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar(inTab ? .hidden : .visible, for: .navigationBar)
            .toolbar {
                if !inTab {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { dismiss() }
                    }
                }
            }
            .confirmationDialog("Forget harlo on this phone?", isPresented: $forgetting, titleVisibility: .visible) {
                Button("Forget", role: .destructive) {
                    model.forget()
                    if !inTab { dismiss() }
                }
            } message: {
                Text("You'll need to scan its QR code again. harlo keeps this phone in its Devices until you remove it there.")
            }
            .sheet(isPresented: $scanning) {
                ScanSheet { code in
                    scanning = false
                    Task { await model.pair(code) }
                }
            }
        }
        .modifier(Themed())
        .task { allowed = await UNUserNotificationCenter.current().notificationSettings().authorizationStatus }
    }

    /// What harlo tells this phone while the app is closed.
    @ViewBuilder var notifications: some View {
        Section {
            if allowed == .denied {
                Button("Allow in iOS Settings…") {
                    if let url = URL(string: UIApplication.openNotificationSettingsURLString) { UIApplication.shared.open(url) }
                }
            } else if let push = model.push {
                Toggle("Questions and approvals", isOn: Binding(get: { push.asks }, set: { on in Task { await model.setPushWants(asks: on, done: push.done) } }))
                Toggle("Finished runs", isOn: Binding(get: { push.done }, set: { on in Task { await model.setPushWants(asks: push.asks, done: on) } }))
            } else if let problem = Notifications.problem {
                Text(problem).foregroundStyle(Color(p.warning))
            } else {
                Button("Turn on notifications") {
                    Task {
                        await Notifications.register()
                        allowed = await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
                    }
                }
            }
            Toggle("Live Activities", isOn: $liveActivities)
                .onChange(of: liveActivities) { _, on in
                    LiveActivities.enabled = on
                    if !on { LiveActivities.endAll() }
                }
        } header: {
            Text("Notifications")
        } footer: {
            Text(allowed == .denied
                 ? "Notifications are off for harlo in iOS Settings."
                 : "While the app is closed. Approvals have Allow and Deny on the notification. A session you send a message to shows on the lock screen and in the Dynamic Island while it works (Live Activities). They come from harlo on your computer through its relay; what they say is encrypted for this phone.")
        }
    }
}
