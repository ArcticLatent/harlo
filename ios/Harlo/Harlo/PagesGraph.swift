import HarloKit
import SwiftUI

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
    /// Their pictures, decoded once (not on every pass of the body).
    @State private var image: UIImage?
    @State private var downImage: UIImage?
    @State private var width = 0.0
    @State private var why = ""
    @State private var loading = true

    var light: Bool { p.background.luminance > 0.5 }
    /// The graph to draw (an overview's links only when it was read last).
    var source: String { wiki.graph(readLast: model.wikiReadLast[wiki.root] ?? "") }

    /// Drawn down when that shows it larger on the screen.
    var shown: (picture: DiagramPicture, image: UIImage)? {
        if DiagramPicture.showsDown(across: picture, down: drawnDown, width: width), let d = drawnDown, let i = downImage {
            return (d, i)
        }
        if let d = picture, let i = image { return (d, i) }
        return nil
    }

    var body: some View {
        Group {
            if let s = shown {
                drawn(s.picture, s.image)
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
        .task(id: "\(light)|\(source)") {
            let source = source
            loading = true
            let colors = ["background": p.elevated.hex, "surface": p.raised.hex, "text": p.foreground.hex, "muted": p.muted.hex, "accent": p.primary.hex]
            let r = await model.diagram(source, light: light, colors: colors)
            picture = r.picture
            image = r.picture.flatMap { UIImage(data: $0.png) }
            why = r.why
            loading = false
            if r.picture != nil, let down = DiagramPicture.down(source) {
                drawnDown = await model.diagram(down, light: light, colors: colors).picture
            } else {
                drawnDown = nil
            }
            downImage = drawnDown.flatMap { UIImage(data: $0.png) }
        }
    }

    /// The picture: a box opens its page, anywhere else zooms.
    func drawn(_ d: DiagramPicture, _ image: UIImage) -> some View {
        let zoomed = ZoomedPicture(image: image, size: CGSize(width: d.width, height: d.height))
        let ratio = CGSize(width: max(d.width, 1), height: max(d.height, 1))
        return Image(uiImage: image)
            .resizable()
            .interpolation(.high)
            .aspectRatio(ratio, contentMode: .fit)
            .contentShape(Rectangle())
            .onTapGesture { zoom = zoomed }
            .accessibilityLabel("The wiki's pages and their links")
            .accessibilityAddTraits(.isButton)
            .accessibilityHint("Opens it to zoom")
            .accessibilityAction { zoom = zoomed }
            .overlay { boxes(d) }
            .frame(maxWidth: d.width)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 6)
    }

    /// A button over each page's box.
    func boxes(_ d: DiagramPicture) -> some View {
        GeometryReader { g in
            ForEach(d.boxes, id: \.id) { (b: DiagramPicture.Box) in
                if let page = wiki.graphPage(b.id) {
                    box(b, page, in: g.size)
                }
            }
        }
    }

    /// A page's box: a button over it (at least 30 points tall, to tap),
    /// ringed when it's the page read last.
    func box(_ b: DiagramPicture.Box, _ page: WikiPage, in size: CGSize) -> some View {
        let last: Bool = model.wikiReadLast[wiki.root] == page.path
        let title: String = page.title.isEmpty ? page.path : page.title
        let label: String = "Read " + title + (last ? ", read last" : "")
        let ring: Color = Color(p.primary).opacity(last ? 0.75 : 0)
        let w: Double = b.w * size.width
        let h: Double = b.h * size.height
        let x: Double = (b.x + b.w / 2) * size.width
        let y: Double = (b.y + b.h / 2) * size.height
        return Button { open(WikiLink(path: page.path)) } label: {
            RoundedRectangle(cornerRadius: 6)
                .strokeBorder(ring, lineWidth: 2)
                .padding(-3)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .frame(width: w, height: h)
        .frame(minHeight: 30)
        .position(x: x, y: y)
        .accessibilityLabel(label)
        .accessibilityIdentifier("wikiGraphBox")
    }
}
