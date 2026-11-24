import HarloKit
import SwiftUI

/// A declaration's card from the computer's code index: where it is, how
/// it's declared, what it calls and what calls it.
struct WikiCodeCard: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let code: WikiHits.Code
    @State private var card: String?
    @State private var failed = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Text(code.place).font(.footnote).foregroundStyle(Color(p.muted))
                if let card {
                    Text(card)
                        .font(.footnote.monospaced())
                        .textSelection(.enabled)
                        .accessibilityIdentifier("wikiCodeCard")
                } else if failed {
                    Text("The code index couldn't be asked.").foregroundStyle(Color(p.muted))
                } else {
                    HStack(spacing: 8) {
                        ProgressView()
                        Text("Looking it up…").font(.subheadline).foregroundStyle(Color(p.muted))
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding()
        }
        .navigationTitle(code.name)
        .navigationBarTitleDisplayMode(.inline)
        .task {
            guard card == nil else { return }
            card = await model.lookupCode(code)
            failed = card == nil
        }
    }
}
