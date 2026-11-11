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
                        Text(why.contains("no longer") ? "No longer on disk" : "Can't show")
                            .font(.caption2.weight(.medium))
                            .multilineTextAlignment(.center)
                            .lineLimit(2)
                            .minimumScaleFactor(0.8)
                    }
                    .foregroundStyle(Color(p.muted))
                    .padding(.horizontal, 6)
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
        .accessibilityLabel("Picture: " + PictureNames.spoken(model.session?.pictureName(path) ?? PictureNames.name(path)))
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
    @Environment(HarloModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let paths: [String]
    @State var current: String
    @State private var bars = true
    /// Every picture at once, as the desktop's grid (more than one).
    @State private var grid = false
    /// How far it's been pulled down (to close), 0…1.
    @State private var pulled: CGFloat = 0
    @State private var shown: [String: UIImage] = [:]

    var index: Int { paths.firstIndex(of: current) ?? 0 }
    /// Its name as people say it ("You sent", not harlo's file name).
    func name(_ path: String) -> String { model.session?.pictureName(path) ?? PictureNames.name(path) }

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
            if grid {
                PictureGrid(paths: paths, current: current, name: name) { path in
                    current = path
                    withAnimation(.easeOut(duration: 0.15)) { grid = false }
                }
                .transition(.opacity)
            }
            if (bars || grid) && pulled == 0 {
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
            if grid {
                Button {
                    withAnimation(.easeOut(duration: 0.15)) { grid = false }
                } label: {
                    HStack(spacing: 4) {
                        Image(systemName: "chevron.left").font(.body.weight(.semibold))
                        Text("Back")
                    }
                    .font(.body)
                    .frame(minHeight: 44)
                    .contentShape(Rectangle())
                }
                .accessibilityIdentifier("pictureGridBack")
                .accessibilityLabel("Back to the picture")
            }
            VStack(alignment: .leading, spacing: 2) {
                if grid {
                    Text("All pictures")
                        .font(.subheadline.weight(.semibold))
                    Text("\(paths.count) in this session")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(0.7))
                } else {
                    Text(name(current))
                        .accessibilityLabel(PictureNames.spoken(name(current)))
                        .font(.subheadline.weight(.semibold))
                        .lineLimit(1)
                        .truncationMode(.middle)
                    if paths.count > 1 {
                        Text("\(index + 1) of \(paths.count)")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(0.7))
                    }
                }
            }
            Spacer(minLength: 8)
            if paths.count > 1 {
                Button {
                    withAnimation(.easeOut(duration: 0.15)) { grid.toggle() }
                } label: {
                    Image(systemName: grid ? "square.grid.2x2.fill" : "square.grid.2x2")
                        .font(.body.weight(.semibold))
                        .frame(minWidth: 44, minHeight: 44)
                }
                .accessibilityIdentifier("pictureGrid")
                .accessibilityLabel(grid ? "Back to the picture" : "All pictures")
            }
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
            if !grid, let image = shown[current] {
                let name = name(current)
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

/// The viewer's grid: every picture of the session, each over its name, the
/// one open ringed; a tap opens one.
struct PictureGrid: View {
    let paths: [String]
    let current: String
    let name: (String) -> String
    let open: (String) -> Void

    var body: some View {
        ScrollViewReader { reader in
            ScrollView {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 104), spacing: 12)], spacing: 16) {
                    ForEach(paths, id: \.self) { path in
                        PictureGridCell(path: path, name: name(path), open: path == current) { open(path) }
                            .id(path)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.top, 64)
                .padding(.bottom, 24)
            }
            .onAppear { reader.scrollTo(current, anchor: .center) }
        }
        .background(Color.black.ignoresSafeArea())
    }
}

/// One picture of the grid: its thumbnail, square, over its name.
struct PictureGridCell: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.dynamicTypeSize) private var typeSize
    let path: String
    let name: String
    let open: Bool
    let tap: () -> Void
    @State private var image: UIImage?
    @State private var why = ""

    var body: some View {
        Button(action: tap) {
            VStack(alignment: .leading, spacing: 6) {
                Color.white.opacity(0.08)
                    .aspectRatio(1, contentMode: .fit)
                    .overlay {
                        if let image {
                            Image(uiImage: image).resizable().interpolation(.high).scaledToFill()
                        } else if !why.isEmpty {
                            VStack(spacing: 4) {
                                Image(systemName: "photo").font(.system(size: 20, weight: .light))
                                Text(why.contains("no longer") ? "No longer on disk" : "Can't show")
                                    .font(.caption2.weight(.medium))
                                    .multilineTextAlignment(.center)
                            }
                            .foregroundStyle(.white.opacity(0.7))
                            .padding(6)
                        } else {
                            ProgressView().tint(.white)
                        }
                    }
                    .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(.white, lineWidth: open ? 2 : 0))
                Text(name)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(open ? 1 : 0.8))
                    .lineLimit(typeSize >= .xLarge ? 2 : 1)
                    .truncationMode(.tail)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("pictureGridCell")
        .accessibilityLabel("Picture: " + PictureNames.spoken(name))
        .accessibilityAddTraits(open ? .isSelected : [])
        .task(id: path) {
            let r = await model.picture(path, thumb: true)
            image = r.picture.flatMap { UIImage(data: $0.data) }
            why = image == nil ? (r.why.isEmpty ? "harlo couldn't read that picture." : r.why) : ""
        }
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
                    .accessibilityLabel(PictureNames.spoken(model.session?.pictureName(path) ?? PictureNames.name(path)))
                    .accessibilityAddTraits(.isImage)
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

    /// Fits the picture inside itself at 1×, centred, whenever it's laid
    /// out (it starts at no size, and turning the phone changes it); zoomed
    /// smaller than the screen, kept centred.
    final class FittingScroll: UIScrollView {
        weak var picture: UIImageView?

        override func layoutSubviews() {
            super.layoutSubviews()
            guard let picture, let image = picture.image else { return }
            if picture.transform.isIdentity {
                let size = PictureFit.size(image.size, in: bounds.size, scale: traitCollection.displayScale)
                if picture.frame.size != size {
                    picture.frame = CGRect(origin: .zero, size: size)
                    contentSize = size
                }
            }
            center()
        }

        func center() {
            let dx = max(0, (bounds.width - contentSize.width) / 2), dy = max(0, (bounds.height - contentSize.height) / 2)
            let inset = UIEdgeInsets(top: dy, left: dx, bottom: dy, right: dx)
            guard contentInset != inset else { return }
            contentInset = inset
            if zoomScale <= minimumZoomScale && !isTracking { contentOffset = CGPoint(x: -dx, y: -dy) }
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

        func scrollViewDidZoom(_ scrollView: UIScrollView) { (scrollView as? FittingScroll)?.center() }

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

/// How big a picture is drawn: as big as fits, but at most four screen pixels
/// to each of its own. A phone's screenshot fills the width, a 300-pixel one
/// nearly does, an icon stays an icon (past that its text goes soft; zooming
/// still goes further).
enum PictureFit {
    static func size(_ pixels: CGSize, in box: CGSize, scale: CGFloat) -> CGSize {
        guard pixels.width > 0, pixels.height > 0, box.width > 0, box.height > 0 else { return .zero }
        let most = min(box.width / pixels.width, box.height / pixels.height, 4 / max(scale, 1))
        return CGSize(width: (pixels.width * most).rounded(), height: (pixels.height * most).rounded())
    }
}

/// A picture a reply shows on its own line, in its place: as wide as the
/// reply (at most four screen pixels to each of its own, as in the
/// viewer), a tap opens the viewer. Its
/// words when it isn't one of the session's pictures (a web address, a
/// file that's gone).
struct ReplyPicture: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.openPicture) private var openPicture
    @Environment(\.displayScale) private var displayScale
    @Environment(\.dynamicTypeSize) private var typeSize
    let src: String
    let alt: String
    @State private var image: UIImage?
    @State private var why = ""

    var path: String? {
        guard model.showsPictures, let s = model.session else { return nil }
        return PictureNames.match(src, in: s.pictures)
    }

    var body: some View {
        if let path {
            Button {
                openPicture?(path)
            } label: {
                Group {
                    if let image {
                        let px = CGSize(width: image.size.width * image.scale, height: image.size.height * image.scale)
                        Image(uiImage: image)
                            .resizable()
                            .interpolation(.high)
                            .aspectRatio(px, contentMode: .fit)
                            .frame(maxWidth: px.width * 4 / max(displayScale, 1))
                    } else if !why.isEmpty {
                        Label(alt.isEmpty ? why : "\(alt): \(why)", systemImage: "photo")
                            .font(.subheadline)
                            .foregroundStyle(Color(p.muted))
                            .padding(12)
                    } else {
                        Rectangle()
                            .fill(Color(p.codeBackground))
                            .aspectRatio(16 / 10, contentMode: .fit)
                            .overlay { ProgressView() }
                    }
                }
                .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color(p.hairline), lineWidth: 1))
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityIdentifier("replyPicture")
            .accessibilityLabel("Picture: " + (alt.isEmpty ? PictureNames.spoken(PictureNames.name(path)) : alt))
            .accessibilityHint(why.isEmpty ? "Opens it" : why)
            .task(id: path) {
                // (The thumbnail first: it's quick, and gives the shape.)
                if let t = await model.picture(path, thumb: true).picture { image = UIImage(data: t.data) }
                let r = await model.picture(path, thumb: false)
                if let full = r.picture.flatMap({ UIImage(data: $0.data) }) {
                    image = full
                } else if image == nil {
                    why = r.why.isEmpty ? "harlo couldn't read that picture." : r.why
                }
            }
        } else {
            Text(Style.attributed([Span(alt, italic: true)], p, typeSize: typeSize))
                .textSelection(.enabled)
        }
    }
}
