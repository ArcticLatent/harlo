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
    /// The same flowchart drawn down, when it's drawn across.
    @State private var drawnDown: DiagramPicture?
    @State private var width = 0.0
    @State private var why = ""
    @State private var loading = true
    @State private var showCode = false
    @State private var zoomed = false

    /// Drawn down when that shows it larger in the card (a narrow phone).
    var shown: DiagramPicture? {
        DiagramPicture.showsDown(across: picture, down: drawnDown, width: width - 20) ? drawnDown : picture
    }

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
            } else if let d = shown, let image = UIImage(data: d.png) {
                VStack(alignment: .leading, spacing: 0) {
                    Button { zoomed = true } label: {
                        Image(uiImage: image)
                            .resizable()
                            .interpolation(.high)
                            .aspectRatio(CGSize(width: max(d.width, 1), height: max(d.height, 1)), contentMode: .fit)
                            .frame(maxWidth: d.width, alignment: .leading)
                            .padding(10)
                            // (Drawn down, it's narrow: centred in its card.)
                            .frame(maxWidth: .infinity, alignment: d == drawnDown ? .center : .leading)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Diagram")
                    .accessibilityHint("Opens it to zoom")
                    toggle("Source", "chevron.left.forwardslash.chevron.right")
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
        .onGeometryChange(for: Double.self) { $0.size.width } action: { width = $0 }
        .task(id: "\(light)|\(wiki.mermaid)") {
            let source = wiki.mermaid
            loading = true
            let r = await model.diagram(source, light: light, colors: colors)
            picture = r.picture
            why = r.why
            loading = false
            if r.picture != nil, let down = DiagramPicture.down(source) {
                drawnDown = await model.diagram(down, light: light, colors: colors).picture
            } else {
                drawnDown = nil
            }
        }
    }

    func toggle(_ title: String, _ icon: String) -> some View {
        Button {
            withAnimation(.easeOut(duration: 0.15)) { showCode.toggle() }
        } label: {
            Label(title, systemImage: icon)
                .font(.footnote.weight(.medium))
                .foregroundStyle(Color(p.primary.legible(on: p.raised, toward: p.foreground, ratio: 5)))
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
    let wiki: Wiki
    @Binding var zoom: ZoomedPicture?
    /// Opens a page, as its box is tapped.
    let open: (WikiLink) -> Void
    @State private var picture: DiagramPicture?
    @State private var drawnDown: DiagramPicture?
    @State private var width = 0.0
    @State private var why = ""
    @State private var loading = true

    var light: Bool { p.background.luminance > 0.5 }

    /// Drawn down when that shows it larger on the screen.
    var shown: DiagramPicture? {
        DiagramPicture.showsDown(across: picture, down: drawnDown, width: width) ? drawnDown : picture
    }

    var body: some View {
        Group {
            if let d = shown, let image = UIImage(data: d.png) {
                let zoomIn = { zoom = ZoomedPicture(image: image, size: CGSize(width: d.width, height: d.height)) }
                // A box opens its page; anywhere else zooms.
                Image(uiImage: image)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(CGSize(width: max(d.width, 1), height: max(d.height, 1)), contentMode: .fit)
                    .contentShape(Rectangle())
                    .onTapGesture(perform: zoomIn)
                    .accessibilityLabel("The wiki's pages and their links")
                    .accessibilityAddTraits(.isButton)
                    .accessibilityHint("Opens it to zoom")
                    .accessibilityAction(perform: zoomIn)
                    .overlay {
                        GeometryReader { g in
                            ForEach(d.boxes, id: \.id) { b in
                                if let page = wiki.graphPage(b.id) { box(b, page, in: g.size) }
                            }
                        }
                    }
                    .frame(maxWidth: d.width)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 6)
            } else if loading {
                HStack(spacing: 8) {
                    ProgressView()
                    Text("Drawing the graph…").font(.subheadline).foregroundStyle(Color(p.muted))
                }
            } else {
                Text(why.isEmpty ? "The graph couldn't be drawn." : why).font(.footnote).foregroundStyle(Color(p.muted))
            }
        }
        .frame(maxWidth: .infinity)
        .onGeometryChange(for: Double.self) { $0.size.width } action: { width = $0 }
        .task(id: "\(light)|\(wiki.mermaid)") {
            let source = wiki.mermaid
            loading = true
            let colors = ["background": p.elevated.hex, "surface": p.raised.hex, "text": p.foreground.hex, "muted": p.muted.hex, "accent": p.primary.hex]
            let r = await model.diagram(source, light: light, colors: colors)
            picture = r.picture
            why = r.why
            loading = false
            if r.picture != nil, let down = DiagramPicture.down(source) {
                drawnDown = await model.diagram(down, light: light, colors: colors).picture
            } else {
                drawnDown = nil
            }
        }
    }

    /// A page's box: a button over it (at least 30 points tall, to tap),
    /// ringed when it's the page read last.
    func box(_ b: DiagramPicture.Box, _ page: WikiPage, in size: CGSize) -> some View {
        let last = model.wikiReadLast[wiki.root] == page.path
        let title = page.title.isEmpty ? page.path : page.title
        return Button { open(WikiLink(path: page.path)) } label: {
            RoundedRectangle(cornerRadius: 6)
                .strokeBorder(Color(p.primary).opacity(last ? 0.75 : 0), lineWidth: 2)
                .padding(-3)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .frame(width: b.w * size.width, height: b.h * size.height)
        .frame(minHeight: 30)
        .position(x: (b.x + b.w / 2) * size.width, y: (b.y + b.h / 2) * size.height)
        .accessibilityLabel("Read " + title + (last ? ", read last" : ""))
        .accessibilityIdentifier("wikiGraphBox")
    }
}
