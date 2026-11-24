import SwiftUI

/// "Good morning, Robin / What are we building today?"
struct Greeting: View {
    @Environment(\.palette) private var p
    /// Your name, for the greeting (Settings).
    @AppStorage("yourName") private var name = ""

    var body: some View {
        let first = name.trimmingCharacters(in: .whitespaces)
        let named = Text(first).foregroundStyle(Color(p.primary))
        Text("\(Style.greeting())\(first.isEmpty ? "" : ", ")\(named)\nWhat are we building today?")
            .scaledFont(32, .medium)
            .tracking(-0.3)
            .lineSpacing(4)
            .fixedSize(horizontal: false, vertical: true)
    }
}
