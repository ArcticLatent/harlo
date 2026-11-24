import HarloKit
import SwiftUI
import UIKit

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
