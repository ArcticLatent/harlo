import HarloKit
import SwiftUI

/// A sheet of the other sessions' requests (a session's toolbar opens it).
struct InboxView: View {
    let except: [String]

    var body: some View {
        NavigationStack {
            ApprovalsView(except: except, sheet: true)
        }
    }
}
