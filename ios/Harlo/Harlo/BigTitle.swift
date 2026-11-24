import HarloKit
import SwiftUI
import UIKit

/// A screen's big title, and a line under it.
struct BigTitle: View {
    @Environment(\.palette) private var p
    let text: String
    var subtitle = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(text)
                .scaledFont(34, .bold)
                .tracking(-0.4)
            if !subtitle.isEmpty {
                Text(subtitle)
                    .scaledFont(16)
                    .foregroundStyle(Color(p.muted))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
