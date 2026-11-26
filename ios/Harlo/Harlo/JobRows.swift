import HarloKit
import SwiftUI

/// What a session has going beside its conversation: commands and
/// subagents in the background, its goal, side questions. Each row says
/// where it stands and offers the one thing to do to it (Stop, Pause,
/// Resume, Dismiss).
struct JobRows: View {
    let rows: [JobRow]

    var body: some View {
        ForEach(rows, content: JobRowView.init)
    }
}
