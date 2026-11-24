import HarloKit
import SwiftUI

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
                ScrollView([.horizontal, .vertical]) {
                    Image(uiImage: image)
                        .resizable()
                        .interpolation(.high)
                        .frame(width: size.width * fit * scale, height: size.height * fit * scale)
                        .frame(minWidth: g.size.width, minHeight: g.size.height)
                }
                .scrollIndicators(.hidden)
                .gesture(MagnifyGesture()
                    .onChanged { v in scale = max(1, min(6, last * v.magnification)) }
                    .onEnded { _ in last = scale })
                .onTapGesture(count: 2) {
                    withAnimation(.easeOut(duration: 0.2)) {
                        scale = scale > 1 ? 1 : 2.5
                        last = scale
                    }
                }
                .accessibilityZoomAction { action in
                    withAnimation(.easeOut(duration: 0.2)) {
                        scale = action.direction == .zoomIn ? min(6, scale * 1.5) : max(1, scale / 1.5)
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
