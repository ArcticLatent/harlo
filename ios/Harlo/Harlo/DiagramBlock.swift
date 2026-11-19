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
    /// Their pictures, decoded once (not on every pass of the body).
    @State private var image: UIImage?
    @State private var downImage: UIImage?
    @State private var width = 0.0
    @State private var why = ""
    @State private var loading = true
    @State private var showCode = false
    @State private var zoomed = false

    /// Drawn down when that shows it larger in the card (a narrow phone).
    var shown: (picture: DiagramPicture, image: UIImage)? {
        if DiagramPicture.showsDown(across: picture, down: drawnDown, width: width - 20), let d = drawnDown, let i = downImage {
            return (d, i)
        }
        if let d = picture, let i = image { return (d, i) }
        return nil
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
            } else if let s = shown {
                let d = s.picture, image = s.image
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
        .task(id: "\(light)|\(source)") {
            loading = true
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
