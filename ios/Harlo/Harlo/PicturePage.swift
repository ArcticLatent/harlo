import HarloKit
import SwiftUI
import UIKit

/// One picture of the viewer: fitted to the screen from harlo, zoomable.
struct PicturePage: View {
    @Environment(HarloModel.self) private var model
    let path: String
    @Binding var pulled: CGFloat
    let loaded: (UIImage) -> Void
    let toggleBars: () -> Void
    let close: () -> Void
    @State private var image: UIImage?
    @State private var thumb: UIImage?
    @State private var why = ""

    var body: some View {
        Group {
            if let image {
                ZoomableImage(image: image, pulled: $pulled, tapped: toggleBars, close: close)
                    .accessibilityElement()
                    .accessibilityIdentifier("pictureViewer")
                    .accessibilityLabel(PictureNames.spoken(model.session?.pictureName(path) ?? PictureNames.name(path)))
                    .accessibilityAddTraits(.isImage)
                    .accessibilityAction(named: "Show or hide the controls", toggleBars)
            } else if !why.isEmpty {
                VStack(spacing: 10) {
                    Image(systemName: "photo").font(.system(size: 34, weight: .light))
                    Text(why.contains("no longer") ? "No longer on disk" : why).font(.subheadline).multilineTextAlignment(.center)
                }
                .foregroundStyle(.white.opacity(0.75))
                .padding(32)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .contentShape(Rectangle())
                .onTapGesture(perform: toggleBars)
                .accessibilityAction(named: "Show or hide the controls", toggleBars)
            } else {
                // The thumbnail while the screen's copy comes.
                ZStack {
                    if let thumb {
                        Image(uiImage: thumb).resizable().scaledToFit()
                    }
                    ProgressView().tint(.white)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .task(id: path) {
            thumb = await model.picture(path, thumb: true).picture.flatMap { UIImage(data: $0.data) }
            let r = await model.picture(path, thumb: false)
            if let p = r.picture, let ui = UIImage(data: p.data) {
                image = ui
                loaded(ui)
            } else {
                why = r.why.isEmpty ? "harlo couldn't read that picture." : r.why
            }
        }
    }
}
