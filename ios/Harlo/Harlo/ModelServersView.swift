import HarloKit
import SwiftUI

/// Settings → Model servers: the ones harlo set up, on its computer and
/// each machine it's connected to, each started or stopped from here.
struct ModelServersView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @State private var machines: [ModelServers]?
    /// Servers starting or stopping (by machine and address).
    @State private var working: Set<String> = []

    var body: some View {
        Form {
            Group {
                if let machines {
                    if machines.allSatisfy({ $0.servers.isEmpty && $0.error.isEmpty }) {
                        Section {
                            Text("harlo hasn't set up a model server yet. Set one up in harlo's Settings → Model servers on your computer.")
                                .foregroundStyle(Color(p.muted))
                        }
                    }
                    ForEach(machines.filter { !$0.servers.isEmpty || !$0.error.isEmpty }) { m in
                        Section {
                            ForEach(m.servers) { s in row(s, on: m.host) }
                            if !m.error.isEmpty {
                                Label(m.error, systemImage: "exclamationmark.triangle")
                                    .font(.system(size: 14))
                                    .foregroundStyle(Color(p.warning))
                            }
                        } header: {
                            Text(m.host.isEmpty ? "This computer" : m.host)
                        }
                    }
                } else {
                    Section {
                        HStack(spacing: 10) {
                            ProgressView()
                            Text("Asking harlo…").foregroundStyle(Color(p.muted))
                        }
                    }
                }
            }
            .listRowBackground(Color(p.elevated))
        }
        .screen()
        .navigationTitle("Model servers")
        .navigationBarTitleDisplayMode(.inline)
        .task { await load() }
        .refreshable { await load() }
    }

    func load() async {
        if let list = await model.modelServers() { machines = list } else if machines == nil { machines = [] }
    }

    func row(_ s: ModelServer, on host: String) -> some View {
        let id = host + "|" + s.url
        let busy = working.contains(id)
        return HStack(spacing: 12) {
            Circle()
                .fill(Color(s.running ? p.success : p.faint))
                .frame(width: 9, height: 9)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 3) {
                Text(s.name).font(.system(size: 16, weight: .medium))
                Text(([s.engine] + s.models).filter { !$0.isEmpty }.joined(separator: " · "))
                    .font(.system(size: 13))
                    .foregroundStyle(Color(p.muted))
                    .lineLimit(2)
                Text(busy ? (s.running ? "Stopping…" : "Starting… (a big model can take minutes)") : s.running ? "Running" : "Stopped")
                    .font(.system(size: 13))
                    .foregroundStyle(Color(s.running && !busy ? p.success : p.muted))
            }
            Spacer(minLength: 6)
            if busy {
                ProgressView()
            } else {
                Button(s.running ? "Stop" : "Start") {
                    UIImpactFeedbackGenerator(style: .light).impactOccurred()
                    set(s, on: host, running: !s.running)
                }
                .buttonStyle(.bordered)
                .tint(Color(s.running ? p.muted : p.primary))
            }
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .combine)
    }

    func set(_ s: ModelServer, on host: String, running: Bool) {
        let id = host + "|" + s.url
        working.insert(id)
        Task {
            let now = await model.setServer(host: host, url: s.url, running: running)
            if let now, var list = machines,
               let i = list.firstIndex(where: { $0.host == host }), let j = list[i].servers.firstIndex(where: { $0.url == s.url }) {
                list[i].servers[j].running = now
                machines = list
            }
            working.remove(id)
        }
    }
}
