import HarloKit
import SwiftUI

/// Why harlo isn't connected, with a retry.
struct ConnectionBanner: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p

    var body: some View {
        HStack(spacing: 10) {
            switch model.state {
            case .connecting, .connected:
                ProgressView()
                Text("Connecting to \(model.host?.name ?? "harlo")…")
                    .foregroundStyle(Color(p.muted))
            case .waiting(let why):
                Image(systemName: "wifi.exclamationmark")
                    .foregroundStyle(Color(p.warning))
                VStack(alignment: .leading, spacing: 2) {
                    Text("Can't reach \(model.host?.name ?? "harlo")")
                    Text(why.isEmpty ? "Trying again…" : why + " Trying again…")
                        .font(.caption)
                        .foregroundStyle(Color(p.muted))
                }
                Spacer()
                Button("Retry") { Task { await model.reconnect() } }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
            case .unpaired:
                EmptyView()
            }
        }
        .font(.callout)
    }
}
