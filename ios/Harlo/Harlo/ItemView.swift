import HarloKit
import SwiftUI
import UIKit

struct ItemView: View {
    let item: TranscriptItem
    let session: OpenSession

    var body: some View {
        switch item {
        case .row(let r):
            RowView(row: r)
        case .run(let group, let summary, let rows, let open, let active):
            RunView(group: group, summary: summary, rows: rows, open: open, active: active, session: session)
        }
    }
}
