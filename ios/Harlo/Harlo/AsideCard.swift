import HarloKit
import SwiftUI

/// A side question (`/btw …`) and its answer as it comes, above the
/// composer: not part of the conversation, and not kept.
struct AsideCard: View {
    @Environment(\.palette) private var p
    @Environment(\.dynamicTypeSize) private var typeSize
    let aside: Aside
    @Binding var folded: Bool
    let dismiss: () -> Void

    var answer: AttributedString {
        (try? AttributedString(markdown: aside.text, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(aside.text)
    }

    /// Folded only once it's answered (an error stays open).
    var isFolded: Bool { folded && aside.done && aside.error.isEmpty }

    var body: some View {
        // (At the largest text sizes, Dismiss goes under the question.)
        let head = typeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 2))
            : AnyLayout(HStackLayout(alignment: .firstTextBaseline, spacing: 8))
        VStack(alignment: .leading, spacing: 8) {
            head {
                Button {
                    withAnimation(.easeOut(duration: 0.2)) { folded.toggle() }
                } label: {
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        Label(aside.question, systemImage: "text.bubble")
                            .font(.subheadline.weight(.semibold))
                            .lineLimit(isFolded ? 1 : 2)
                            .multilineTextAlignment(.leading)
                        if aside.done && aside.error.isEmpty {
                            Image(systemName: isFolded ? "chevron.down" : "chevron.up")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(Color(p.muted))
                        }
                    }
                    .frame(maxWidth: .infinity, minHeight: 32, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(!aside.done || !aside.error.isEmpty)
                .accessibilityHint(isFolded ? "Shows the answer" : "Folds the answer")
                if !aside.done {
                    Text("answering…")
                        .font(.caption)
                        .foregroundStyle(Color(p.muted))
                }
                Button("Dismiss", action: dismiss)
                    .font(.subheadline)
                    .foregroundStyle(Color(p.primary))
                    .buttonStyle(.plain)
                    .frame(minWidth: 44, minHeight: 32)
                    .contentShape(Rectangle())
            }
            if !aside.error.isEmpty {
                Text(aside.error)
                    .font(.subheadline)
                    .foregroundStyle(Color(p.error))
            } else if !aside.text.isEmpty && !isFolded {
                ScrollView {
                    Text(answer)
                        .scaledFont(15)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .textSelection(.enabled)
                }
                // (At the largest text sizes, less of it at once: the
                // conversation above keeps its room.)
                .frame(maxHeight: typeSize.isAccessibilitySize ? 150 : 220)
                .fixedSize(horizontal: false, vertical: true)
            }
            if aside.done && aside.error.isEmpty && !isFolded && !typeSize.isAccessibilitySize {
                Text("A side question: not part of the conversation, and not kept.")
                    .font(.caption)
                    .foregroundStyle(Color(p.muted))
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, isFolded ? 6 : 14)
        .card(p, radius: 18)
        .padding(.horizontal, 12)
        .padding(.top, 8)
    }
}
