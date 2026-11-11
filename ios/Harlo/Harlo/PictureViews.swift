import HarloKit
import SwiftUI
import UIKit

// The conversation's pictures: a photo you sent, one a tool returned or a
// model made, a picture file a reply names. Thumbnails under their row; a
// tap opens the viewer, which steps through the session's pictures.

private struct OpenPictureKey: EnvironmentKey {
    static let defaultValue: ((String) -> Void)? = nil
}

extension EnvironmentValues {
    /// Opens one of the session's pictures in the viewer (the session view
    /// sets it).
    var openPicture: ((String) -> Void)? {
        get { self[OpenPictureKey.self] }
        set { self[OpenPictureKey.self] = newValue }
    }
}

/// The picture the viewer opens at (Identifiable, for `fullScreenCover`).
struct PictureOpening: Identifiable {
    let path: String
    var id: String { path }
}

/// A row's pictures, as thumbnails in a row that wraps.
struct PictureStrip: View {
    @Environment(HarloModel.self) private var model
    let paths: [String]
    var side: CGFloat = 84

    var body: some View {
        if model.showsPictures && !paths.isEmpty {
            Flow(spacing: 8) {
                ForEach(paths, id: \.self) { path in
                    PictureTile(path: path, side: side)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

/// One thumbnail: square, the picture filling it; a tap opens it. Says so
/// when harlo can't send it (the file's gone, say).
struct PictureTile: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.openPicture) private var openPicture
    let path: String
    var side: CGFloat = 84
    @State private var image: UIImage?
    @State private var why = ""

    var body: some View {
        Button {
            openPicture?(path)
        } label: {
            ZStack {
                Rectangle().fill(Color(p.codeBackground))
                if let image {
                    Image(uiImage: image)
                        .resizable()
                        .interpolation(.high)
                        .scaledToFill()
                        .frame(width: side, height: side)
                        .clipped()
                } else if !why.isEmpty {
                    VStack(spacing: 4) {
                        Image(systemName: "photo")
                            .font(.system(size: 18, weight: .medium))
                        Text(why.contains("no longer") ? "Gone" : "Can't show")
                            .font(.caption2.weight(.medium))
                    }
                    .foregroundStyle(Color(p.muted))
                } else {
                    ProgressView()
                }
            }
            .frame(width: side, height: side)
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color(p.hairline), lineWidth: 1))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("pictureTile")
        .accessibilityLabel("Picture, " + PictureNames.name(path))
        .accessibilityHint(why.isEmpty ? "Opens it" : why)
        .task(id: path) {
            let r = await model.picture(path, thumb: true)
            image = r.picture.flatMap { UIImage(data: $0.data) }
            why = image == nil ? (r.why.isEmpty ? "harlo couldn't read that picture." : r.why) : ""
        }
    }
}

/// The session's pictures full screen: swipe across for the next, pinch or
/// double-tap to zoom, swipe down (or Done) to close, Share to send or save
/// one. A tap hides the bars.
struct PictureViewer: View {
    @Environment(\.dismiss) private var dismiss
    let paths: [String]
    @State var current: String
    @State private var bars = true
    /// How far it's been pulled down (to close), 0…1.
    @State private var pulled: CGFloat = 0
    @State private var shown: [String: UIImage] = [:]

    var index: Int { paths.firstIndex(of: current) ?? 0 }

    var body: some View {
        ZStack {
            Color.black.opacity(1 - Double(pulled) * 0.7).ignoresSafeArea()
            TabView(selection: $current) {
                ForEach(paths, id: \.self) { path in
                    PicturePage(path: path, pulled: $pulled, loaded: { shown[path] = $0 }, toggleBars: {
                        withAnimation(.easeOut(duration: 0.15)) { bars.toggle() }
                    }, close: { dismiss() })
                    .tag(path)
                }
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
            .ignoresSafeArea()
            if bars && pulled == 0 {
                VStack(spacing: 0) {
                    top
                    Spacer()
                    bottom
                }
                .transition(.opacity)
            }
        }
        .statusBarHidden(!bars)
        .preferredColorScheme(.dark)
    }

    var top: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(PictureNames.name(current))
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(1)
                    .truncationMode(.middle)
                if paths.count > 1 {
                    Text("\(index + 1) of \(paths.count)")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(0.7))
                }
            }
            Spacer(minLength: 8)
            Button("Done") { dismiss() }
                .font(.body.weight(.semibold))
                .frame(minHeight: 44)
                .accessibilityIdentifier("pictureDone")
        }
        .foregroundStyle(.white)
        .padding(.horizontal, 16)
        .padding(.bottom, 8)
        .background(LinearGradient(colors: [.black.opacity(0.7), .clear], startPoint: .top, endPoint: .bottom).ignoresSafeArea())
    }

    var bottom: some View {
        HStack {
            if let image = shown[current] {
                let name = PictureNames.name(current)
                ShareLink(item: Image(uiImage: image), preview: SharePreview(name, image: Image(uiImage: image))) {
                    Label("Share", systemImage: "square.and.arrow.up")
                        .font(.body.weight(.medium))
                        .frame(minHeight: 44)
                }
                .accessibilityIdentifier("pictureShare")
            }
            Spacer()
        }
        .foregroundStyle(.white)
        .padding(.horizontal, 16)
        .padding(.top, 8)
        .background(LinearGradient(colors: [.clear, .black.opacity(0.7)], startPoint: .top, endPoint: .bottom).ignoresSafeArea())
    }
}

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
                    .accessibilityLabel(PictureNames.name(path))
                    .accessibilityAddTraits(.isImage)
            } else if !why.isEmpty {
                VStack(spacing: 10) {
                    Image(systemName: "photo").font(.system(size: 34, weight: .light))
                    Text(why).font(.subheadline).multilineTextAlignment(.center)
                }
                .foregroundStyle(.white.opacity(0.75))
                .padding(32)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .contentShape(Rectangle())
                .onTapGesture(perform: toggleBars)
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

/// A picture in a scroll view that zooms: pinch, double-tap (in at the
/// tap, out again); pulled down while it isn't zoomed, it closes.
struct ZoomableImage: UIViewRepresentable {
    let image: UIImage
    @Binding var pulled: CGFloat
    let tapped: () -> Void
    let close: () -> Void

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    /// Fits the picture to itself at 1× whenever it's laid out (it starts
    /// at no size, and turning the phone changes it).
    final class FittingScroll: UIScrollView {
        weak var picture: UIView?

        override func layoutSubviews() {
            super.layoutSubviews()
            guard let picture, picture.transform.isIdentity, picture.frame.size != bounds.size else { return }
            picture.frame = CGRect(origin: .zero, size: bounds.size)
            contentSize = bounds.size
        }
    }

    func makeUIView(context: Context) -> UIScrollView {
        let scroll = FittingScroll()
        scroll.delegate = context.coordinator
        scroll.minimumZoomScale = 1
        scroll.maximumZoomScale = 6
        scroll.showsVerticalScrollIndicator = false
        scroll.showsHorizontalScrollIndicator = false
        scroll.contentInsetAdjustmentBehavior = .never
        scroll.backgroundColor = .clear
        let view = UIImageView(image: image)
        view.contentMode = .scaleAspectFit
        view.isUserInteractionEnabled = true
        scroll.addSubview(view)
        scroll.picture = view
        context.coordinator.imageView = view
        context.coordinator.scroll = scroll

        let double = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.doubleTapped(_:)))
        double.numberOfTapsRequired = 2
        scroll.addGestureRecognizer(double)
        let single = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.singleTapped))
        single.require(toFail: double)
        scroll.addGestureRecognizer(single)
        let pull = UIPanGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.pulling(_:)))
        pull.delegate = context.coordinator
        scroll.addGestureRecognizer(pull)
        return scroll
    }

    func updateUIView(_ scroll: UIScrollView, context: Context) {
        context.coordinator.parent = self
        if context.coordinator.imageView?.image !== image {
            context.coordinator.imageView?.image = image
            scroll.setZoomScale(1, animated: false)
        }
    }

    final class Coordinator: NSObject, UIScrollViewDelegate, UIGestureRecognizerDelegate {
        var parent: ZoomableImage
        weak var imageView: UIImageView?
        weak var scroll: UIScrollView?

        init(_ parent: ZoomableImage) { self.parent = parent }

        func viewForZooming(in scrollView: UIScrollView) -> UIView? { imageView }

        @objc func singleTapped() { parent.tapped() }

        @objc func doubleTapped(_ g: UITapGestureRecognizer) {
            guard let scroll, let imageView else { return }
            if scroll.zoomScale > 1 {
                scroll.setZoomScale(1, animated: true)
            } else {
                let at = g.location(in: imageView)
                let scale: CGFloat = 2.5
                let size = CGSize(width: scroll.bounds.width / scale, height: scroll.bounds.height / scale)
                scroll.zoom(to: CGRect(x: at.x - size.width / 2, y: at.y - size.height / 2, width: size.width, height: size.height), animated: true)
            }
        }

        /// Only a downward pull, and only at 1× (zoomed, a drag moves it).
        func gestureRecognizerShouldBegin(_ g: UIGestureRecognizer) -> Bool {
            guard let pan = g as? UIPanGestureRecognizer, let scroll else { return true }
            let v = pan.velocity(in: scroll)
            return scroll.zoomScale <= 1.01 && v.y > 0 && v.y > abs(v.x) * 1.5
        }

        func gestureRecognizer(_ g: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
            g is UIPanGestureRecognizer && other == scroll?.panGestureRecognizer
        }

        @objc func pulling(_ g: UIPanGestureRecognizer) {
            guard let scroll else { return }
            let dy = max(0, g.translation(in: scroll.superview).y)
            switch g.state {
            case .changed:
                scroll.transform = CGAffineTransform(translationX: 0, y: dy)
                parent.pulled = min(1, dy / 300)
            case .ended, .cancelled:
                if g.state == .ended && (dy > 120 || g.velocity(in: scroll).y > 900) {
                    parent.close()
                } else {
                    UIView.animate(withDuration: 0.2) { scroll.transform = .identity }
                    parent.pulled = 0
                }
            default:
                break
            }
        }
    }
}
