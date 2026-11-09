import HarloKit
import SwiftUI

/// An ```options block in a reply: its choices as cards, one above another,
/// the one the agent would pick marked, with its pros and cons; the code a
/// tap away.
struct OptionsBlock: View {
    @Environment(\.palette) private var p
    let choices: [Choice]
    let code: CodeBlock
    @State private var showCode = false

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
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(c.name)
                    .font(.body.weight(.semibold))
                    .foregroundStyle(Color(p.foreground))
                    .frame(maxWidth: .infinity, alignment: .leading)
                if c.recommended {
                    Text("Recommended")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(Color(p.primary))
                }
            }
            if !c.about.isEmpty {
                Text(c.about)
                    .font(.subheadline)
                    .foregroundStyle(Color(p.muted))
            }
            ForEach(Array(c.pros.enumerated()), id: \.offset) { _, t in point("+", t, p.success) }
            ForEach(Array(c.cons.enumerated()), id: \.offset) { _, t in point("−", t, p.error) }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 10).fill(Color(p.raised)))
        .overlay(RoundedRectangle(cornerRadius: 10)
            .strokeBorder(Color(c.recommended ? p.primary : p.hairline), lineWidth: c.recommended ? 1.5 : 1))
        .accessibilityElement(children: .combine)
        .accessibilityLabel(c.recommended ? "\(c.name), recommended" : c.name)
    }

    func point(_ mark: String, _ text: String, _ color: RGBA) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(mark)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(Color(color))
                .accessibilityLabel(mark == "+" ? "For:" : "Against:")
            Text(text)
                .font(.subheadline)
                .foregroundStyle(Color(p.foreground))
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

/// A ```mockup block in a reply: a picture of the HTML file the agent
/// wrote, taken by harlo, fitted to the width (tap to zoom); with one of how
/// it is now, Before and After switch between them. The code a tap away;
/// when there's no picture, the code with why.
struct MockupBlock: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let source: String
    let code: CodeBlock
    @State private var pictures: MockupPictures?
    @State private var why = ""
    @State private var loading = true
    @State private var showCode = false
    @State private var showBefore = false
    @State private var zoomed = false

    var picture: DiagramPicture? { showBefore ? pictures?.before : pictures?.after }
    var pictureWhy: String { (showBefore ? pictures?.beforeWhy : pictures?.afterWhy) ?? why }

    var body: some View {
        Group {
            if loading && pictures == nil {
                HStack(spacing: 8) {
                    ProgressView()
                    Text("Taking a picture of it…").font(.subheadline).foregroundStyle(Color(p.muted))
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .card(p, radius: 10, fill: p.codeBackground)
            } else if showCode || pictures == nil || (pictures?.after == nil && pictures?.before == nil) {
                VStack(alignment: .leading, spacing: 6) {
                    code
                    if pictures?.after != nil || pictures?.before != nil {
                        VisualToggle(title: "Show the picture", icon: "photo") { showCode = false }
                    } else if !pictureWhy.isEmpty {
                        Text(pictureWhy)
                            .font(.footnote)
                            .foregroundStyle(Color(p.muted))
                            .padding(.horizontal, 4)
                    }
                }
            } else {
                VStack(alignment: .leading, spacing: 8) {
                    if let m = pictures, !m.title.isEmpty {
                        Text(m.title)
                            .font(.footnote.weight(.medium))
                            .foregroundStyle(Color(p.muted))
                            .padding(.horizontal, 2)
                    }
                    if pictures?.hasBefore == true {
                        Picker("Show", selection: $showBefore) {
                            Text("Before").tag(true)
                            Text("After").tag(false)
                        }
                        .pickerStyle(.segmented)
                        .accessibilityIdentifier("mockupBeforeAfter")
                    }
                    if let d = picture, let image = UIImage(data: d.png) {
                        Button { zoomed = true } label: {
                            Image(uiImage: image)
                                .resizable()
                                .interpolation(.high)
                                .aspectRatio(CGSize(width: max(d.width, 1), height: max(d.height, 1)), contentMode: .fit)
                                .frame(maxWidth: d.width)
                                .clipShape(RoundedRectangle(cornerRadius: 6))
                                .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Color(p.hairline), lineWidth: 1))
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(showBefore ? "Mockup, before" : "Mockup")
                        .accessibilityHint("Opens it to zoom")
                        .fullScreenCover(isPresented: $zoomed) {
                            DiagramZoom(image: image, size: CGSize(width: d.width, height: d.height), title: "Mockup")
                        }
                    } else {
                        Text(pictureWhy.isEmpty ? "There's no picture of it." : pictureWhy)
                            .font(.footnote)
                            .foregroundStyle(Color(p.warning))
                            .padding(.vertical, 8)
                    }
                    VisualToggle(title: "Code", icon: "chevron.left.forwardslash.chevron.right") { showCode = true }
                }
            }
        }
        // Asked each time it comes on screen, so an edited file shows
        // (harlo keeps the pictures until the file changes).
        .task(id: source) {
            let r = await model.mockup(source)
            if r.pictures != nil || pictures == nil {
                pictures = r.pictures
                why = r.why
            }
            loading = false
        }
    }
}

/// The small link under a visual block that switches it to its code and back.
struct VisualToggle: View {
    @Environment(\.palette) private var p
    let title: String
    let icon: String
    let action: () -> Void

    var body: some View {
        Button {
            withAnimation(.easeOut(duration: 0.15)) { action() }
        } label: {
            Label(title, systemImage: icon)
                .font(.footnote.weight(.medium))
                .foregroundStyle(Color(p.primary))
                .padding(.horizontal, 4)
                // (A full-size target, whatever its text.)
                .frame(minHeight: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
