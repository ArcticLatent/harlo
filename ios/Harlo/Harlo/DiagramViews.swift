import HarloKit
import SwiftUI

/// A Mermaid diagram in a reply: drawn by harlo in the app's colours,
/// fitted to the width (tap for the whole of it, to zoom), its code a tap
/// away. What can't be drawn shows its code.
struct DiagramBlock: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let source: String
    let code: CodeBlock
    @State private var picture: DiagramPicture?
    @State private var why = ""
    @State private var loading = true
    @State private var showCode = false
    @State private var zoomed = false

    var light: Bool { p.background.luminance > 0.5 }

    var colors: [String: String] {
        ["background": p.raised.hex, "surface": p.elevated.hex, "text": p.foreground.hex, "muted": p.muted.hex, "accent": p.primary.hex]
    }

    var body: some View {
        Group {
            if showCode || (!loading && picture == nil) {
                VStack(alignment: .leading, spacing: 6) {
                    code
                    if picture != nil {
                        toggle("Show the diagram", "point.3.connected.trianglepath.dotted")
                    } else if !why.isEmpty {
                        Text(why)
                            .scaledFont(12.5)
                            .foregroundStyle(Color(p.muted))
                            .padding(.horizontal, 4)
                    }
                }
            } else if let d = picture, let image = UIImage(data: d.png) {
                VStack(alignment: .leading, spacing: 0) {
                    Button { zoomed = true } label: {
                        Image(uiImage: image)
                            .resizable()
                            .interpolation(.high)
                            .aspectRatio(CGSize(width: max(d.width, 1), height: max(d.height, 1)), contentMode: .fit)
                            .frame(maxWidth: d.width, alignment: .leading)
                            .padding(10)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Diagram")
                    .accessibilityHint("Opens it to zoom")
                    toggle("Code", "chevron.left.forwardslash.chevron.right")
                        .padding(.horizontal, 6)
                        .padding(.bottom, 4)
                }
                // (A picture, not code: on a card like the options' and a
                // mockup's; its code goes in the code well.)
                .card(p, radius: 10)
                .fullScreenCover(isPresented: $zoomed) { DiagramZoom(image: image, size: CGSize(width: d.width, height: d.height)) }
            } else {
                HStack(spacing: 8) {
                    ProgressView()
                    Text("Drawing the diagram…").scaledFont(13.5).foregroundStyle(Color(p.muted))
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .card(p, radius: 10, fill: p.codeBackground)
            }
        }
        .task(id: "\(light)|\(source)") {
            loading = true
            let r = await model.diagram(source, light: light, colors: colors)
            picture = r.picture
            why = r.why
            loading = false
        }
    }

    func toggle(_ title: String, _ icon: String) -> some View {
        Button {
            withAnimation(.easeOut(duration: 0.15)) { showCode.toggle() }
        } label: {
            Label(title, systemImage: icon)
                .font(.footnote.weight(.medium))
                .foregroundStyle(Color(p.primary))
                .padding(.horizontal, 4)
                .frame(minHeight: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// A diagram on its own: pinch to zoom, drag to look around.
struct DiagramZoom: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.palette) private var p
    let image: UIImage
    let size: CGSize
    var title = "Diagram"
    @State private var scale: CGFloat = 1
    @State private var last: CGFloat = 1

    var body: some View {
        NavigationStack {
            GeometryReader { g in
                let fit = min(g.size.width / max(size.width, 1), g.size.height / max(size.height, 1), 1)
                ScrollView([.horizontal, .vertical], showsIndicators: false) {
                    Image(uiImage: image)
                        .resizable()
                        .interpolation(.high)
                        .frame(width: size.width * fit * scale, height: size.height * fit * scale)
                        .frame(minWidth: g.size.width, minHeight: g.size.height)
                }
                .gesture(MagnifyGesture()
                    .onChanged { v in scale = max(1, min(6, last * v.magnification)) }
                    .onEnded { _ in last = scale })
                .onTapGesture(count: 2) {
                    withAnimation(.easeOut(duration: 0.2)) {
                        scale = scale > 1 ? 1 : 2.5
                        last = scale
                    }
                }
            }
            .background(Color(p.codeBackground).ignoresSafeArea())
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
            }
        }
        .modifier(Themed())
    }
}

/// A drawn picture opened to zoom (Identifiable, for `fullScreenCover`).
struct ZoomedPicture: Identifiable {
    let id = UUID()
    let image: UIImage
    let size: CGSize
}

/// The wiki's pages and their links, drawn by harlo (as a reply's
/// diagram): fitted to the row, tap to zoom. (The zoom is presented by
/// the screen, outside the list: a cover on a row the list then drops,
/// as switching back to Pages does, left the next page push stuck.)
struct PagesGraph: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let source: String
    @Binding var zoom: ZoomedPicture?
    @State private var picture: DiagramPicture?
    @State private var why = ""
    @State private var loading = true

    var light: Bool { p.background.luminance > 0.5 }

    var body: some View {
        Group {
            if let d = picture, let image = UIImage(data: d.png) {
                Button { zoom = ZoomedPicture(image: image, size: CGSize(width: d.width, height: d.height)) } label: {
                    Image(uiImage: image)
                        .resizable()
                        .interpolation(.high)
                        .aspectRatio(CGSize(width: max(d.width, 1), height: max(d.height, 1)), contentMode: .fit)
                        .frame(maxWidth: d.width)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 6)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("The wiki's pages and their links")
                .accessibilityHint("Opens it to zoom")
            } else if loading {
                HStack(spacing: 8) {
                    ProgressView()
                    Text("Drawing the graph…").font(.subheadline).foregroundStyle(Color(p.muted))
                }
            } else {
                Text(why.isEmpty ? "The graph couldn't be drawn." : why).font(.footnote).foregroundStyle(Color(p.muted))
            }
        }
        .task(id: "\(light)|\(source)") {
            loading = true
            let colors = ["background": p.elevated.hex, "surface": p.raised.hex, "text": p.foreground.hex, "muted": p.muted.hex, "accent": p.primary.hex]
            let r = await model.diagram(source, light: light, colors: colors)
            picture = r.picture
            why = r.why
            loading = false
        }
    }
}
