import HarloKit
import SwiftUI

/// A transcript row: a reply opens with the agent's tile, name and time
/// (after your message, or harlo's to the agent); replies full width
/// under it.
struct TranscriptRow: View {
    let item: TranscriptItem
    /// The first of the agent's rows after a sent message.
    let opensReply: Bool
    let session: OpenSession
    var openSheet: (SessionSheet) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if opensReply {
                AgentHead(session: session, time: SessionContent.time(of: item), replyModel: SessionContent.model(of: item))
                    .padding(.top, 6)
            }
            ItemView(item: item, session: session)
                .contextMenu { ItemMenu(item: item, session: session, openSheet: openSheet) }
        }
    }
}
