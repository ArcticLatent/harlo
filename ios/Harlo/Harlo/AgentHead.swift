import HarloKit
import SwiftUI
import UIKit

/// The agent at the top of its reply: its tile, its name, the reply's
/// model when it isn't the session's now (written before a switch, or by a
/// fallback), and when.
struct AgentHead: View {
    @Environment(\.palette) private var p
    let session: OpenSession
    /// When the reply started (unix ms; 0: unknown).
    var time: Int64 = 0
    /// The model that wrote it ("" unknown).
    var replyModel: String = ""

    /// Its name, when it isn't the session's model.
    var otherModel: String {
        let name = replyModel.split(separator: "/").last.map(String.init) ?? ""
        let now = session.model
        guard !name.isEmpty, !now.isEmpty, replyModel != now, name != now, !now.hasSuffix("/" + name) else { return "" }
        return name
    }

    var body: some View {
        HStack(spacing: 8) {
            AgentTile(harness: session.harness, size: Transcript.tile)
            Text(Style.harnessLabel(session.harness))
                .scaledFont(16.5, .semibold)
            if !otherModel.isEmpty {
                Text(otherModel)
                    .scaledFont(13.5)
                    .foregroundStyle(Color(p.muted))
                    .lineLimit(1)
            }
            if time > 0 {
                Text(Style.clock(time))
                    .scaledFont(13.5)
                    .foregroundStyle(Color(p.muted))
            }
        }
        .padding(.top, 4)
    }
}
