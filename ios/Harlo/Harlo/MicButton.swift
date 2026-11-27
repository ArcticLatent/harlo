import SwiftUI

/// The mic: tap to dictate into `text`, tap again to stop (red while it
/// listens).
struct MicButton: View {
    @Environment(\.palette) private var p
    @Binding var text: String
    var size: CGFloat = 34
    @State private var dictation = Dictation()

    var failure: String? {
        if case .failed(let why) = dictation.state { return why }
        return nil
    }

    var body: some View {
        Button {
            dictation.toggle(into: $text)
        } label: {
            Image(systemName: dictation.listening ? "waveform" : "mic")
                .scaledFont(16, .semibold)
                .symbolEffect(.variableColor.iterative, isActive: dictation.state == .listening)
                .frame(width: size, height: size)
                .background { RoundedRectangle(cornerRadius: Corner.card).fill(Color(dictation.listening ? p.error : p.subtle)) }
                .overlay { RoundedRectangle(cornerRadius: Corner.card).strokeBorder(Color(dictation.listening ? p.error : p.border), lineWidth: 1) }
                .foregroundStyle(dictation.listening ? Color.white : Color(p.muted))
        }
        .buttonStyle(.plain)
        // A tap as it starts listening, and as it stops.
        .sensoryFeedback(trigger: dictation.state, Self.feedback)
        .accessibilityLabel(dictation.listening ? "Stop dictating" : "Dictate")
        .alert("Dictation", isPresented: Binding(get: { failure != nil }, set: { if !$0 { dictation.clearFailure() } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(failure ?? "")
        }
        // Leaving the screen stops it.
        .onDisappear { dictation.stop() }
    }

    /// A light tap as it starts listening, and as it stops.
    static func feedback(_ old: Dictation.State, _ new: Dictation.State) -> SensoryFeedback? {
        let starts = new == .listening
        let stops = old == .listening && new == .idle
        return starts || stops ? .impact(weight: .light) : nil
    }
}
