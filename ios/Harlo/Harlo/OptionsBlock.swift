import HarloKit
import SwiftUI
import UIKit

/// An ```options block in a reply: its choices as cards, one above another,
/// the one the agent would pick marked, with its pros and cons; the code a
/// tap away. While the agent waits for your answer, each has a Choose
/// button that sends its name; the one you chose stays marked.
struct OptionsBlock: View {
    @Environment(\.palette) private var p
    @Environment(\.dynamicTypeSize) private var typeSize
    let choices: [Choice]
    let code: CodeBlock
    /// What you wrote next (nil while nothing has followed).
    var answer: String? = nil
    /// Choose buttons shown (the latest reply, the agent waiting).
    var choosing = false
    var choose: (String) -> Void = { _ in }
    @State private var showCode = false

    static func norm(_ t: String) -> String {
        t.trimmingCharacters(in: .whitespacesAndNewlines).trimmingCharacters(in: CharacterSet(charactersIn: ".!")).lowercased()
    }

    var chosen: String? {
        guard let a = answer.map(Self.norm), !a.isEmpty else { return nil }
        return choices.first { Self.norm($0.name) == a || Self.norm(Visuals.plain($0.name)) == a }?.name
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if showCode {
                code
            } else {
                ForEach(choices) { c in card(c) }
            }
            VisualToggle(title: showCode ? "Show the cards" : "Code",
                         icon: showCode ? "rectangle.split.3x1" : "chevron.left.forwardslash.chevron.right") {
                showCode.toggle()
            }
        }
    }

    func card(_ c: Choice) -> some View {
        let picked = chosen == c.name
        let marked = picked || (c.recommended && chosen == nil)
        return VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                // (Its Markdown drawn: `code` as code; the rest semibold.)
                Text(Style.attributed(Visuals.spans(c.name).map { var s = $0; s.bold = s.bold || !s.code; return s }, p, size: 17, typeSize: typeSize))
                    .foregroundStyle(Color(p.foreground))
                    .frame(maxWidth: .infinity, alignment: .leading)
                if picked {
                    Text("Chosen")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(Color(p.primary))
                } else if c.recommended && chosen == nil {
                    Text("Recommended")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(Color(p.primary))
                }
            }
            if !c.about.isEmpty {
                Text(Style.attributed(Visuals.spans(c.about), p, size: 15, typeSize: typeSize))
                    .foregroundStyle(Color(p.muted))
            }
            ForEach(Array(c.pros.enumerated()), id: \.offset) { _, t in point("+", t, p.success) }
            ForEach(Array(c.cons.enumerated()), id: \.offset) { _, t in point("−", t, p.error) }
            if choosing && chosen == nil {
                Button {
                    UIImpactFeedbackGenerator(style: .light).impactOccurred()
                    choose(c.name)
                } label: {
                    Text("Choose")
                        .font(.subheadline.weight(.semibold))
                        .padding(.horizontal, 16)
                        .padding(.vertical, 8)
                        .foregroundStyle(Color(c.recommended ? p.onPrimary : p.foreground))
                        .background { RoundedRectangle(cornerRadius: 8).fill(Color(c.recommended ? p.primary : p.raised)) }
                        .overlay { RoundedRectangle(cornerRadius: 8).strokeBorder(Color(c.recommended ? p.primary : p.hairline), lineWidth: 1) }
                }
                .buttonStyle(.plain)
                .padding(.top, 4)
                .accessibilityLabel("Choose \(Visuals.plain(c.name))")
                .accessibilityIdentifier("optionChoose")
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background { RoundedRectangle(cornerRadius: 10).fill(Color(p.raised)) }
        .overlay {
            RoundedRectangle(cornerRadius: 10)
                .strokeBorder(Color(marked ? p.primary : p.hairline), lineWidth: picked ? 2 : marked ? 1.5 : 1)
        }
        .opacity(chosen != nil && !picked ? 0.55 : 1)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(picked ? "\(Visuals.plain(c.name)), chosen" : c.recommended ? "\(Visuals.plain(c.name)), recommended" : Visuals.plain(c.name))
    }

    func point(_ mark: String, _ text: String, _ color: RGBA) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(mark)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(Color(color))
                .accessibilityLabel(mark == "+" ? "For:" : "Against:")
            Text(Style.attributed(Visuals.spans(text), p, size: 15, typeSize: typeSize))
                .foregroundStyle(Color(p.foreground))
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
