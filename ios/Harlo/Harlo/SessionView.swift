import HarloKit
import SwiftUI
import UIKit

struct SessionView: View {
    @Environment(HarloModel.self) private var model

    var body: some View {
        if let s = model.session {
            // (Another session opened from "Needs you" starts afresh.)
            SessionContent(session: s)
                .id(ObjectIdentifier(s))
        } else {
            ProgressView()
        }
    }
}
