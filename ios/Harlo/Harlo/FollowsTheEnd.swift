import HarloKit
import SwiftUI
import UIKit

/// Whether the transcript is at its end, and back to the end when it's
/// scrolled past it: rows folding, a card or the keyboard resizing can
/// leave it showing only empty space below the last row (iOS 18 and later,
/// which say where it's scrolled). Content growing under it while it's at
/// the end (rows coming in as it opens, a reply, a picture or a diagram
/// drawn) keeps it there: only scrolling up leaves the end.
struct FollowsTheEnd: ViewModifier {
    @Binding var atBottom: Bool
    var toEnd: () -> Void
    @State private var healed = Date.distantPast
    /// Your finger on it, or its fling: only that leaves the end (a picture
    /// or a panel coming in late moved the end out of sight too, and the
    /// session then stopped short of it).
    @State private var yours = false

    struct Where: Equatable {
        var nearEnd: Bool
        /// Scrolled all the way down (within the stack's bottom padding).
        var atEnd: Bool
        /// Nothing of the content in sight: past its end.
        var pastEnd: Bool
        /// The content's height, and the top of what's in sight.
        var height: CGFloat
        var top: CGFloat
    }

    func body(content: Content) -> some View {
        content.onScrollGeometryChange(for: Where.self) { g in
            let end = g.contentSize.height
            let lowest = end + g.contentInsets.bottom - g.containerSize.height
            return Where(nearEnd: g.visibleRect.maxY >= end - 120,
                         atEnd: g.contentOffset.y >= lowest - 20,
                         pastEnd: end > 0 && g.visibleRect.minY > end - 40,
                         height: end, top: g.visibleRect.minY)
        } action: { old, now in
            // At the end and moved from it by anything but you (grown
            // below, a panel taller, laid out late): to the new end,
            // however little it moved (a panel a line taller hid the
            // last reply's end).
            if atBottom && !yours && (!now.nearEnd || (old.atEnd && !now.atEnd)) {
                toEnd()
                return
            }
            atBottom = now.nearEnd
            // (Once a moment: the scroll there says where it is again.)
            if now.pastEnd && Date.now.timeIntervalSince(healed) > 0.4 {
                healed = Date.now
                atBottom = true
                toEnd()
            }
        }
        .onScrollPhaseChange { _, phase in
            // (Animating: a tap on the status bar, to the top; the
            // scrolls to the end aren't animated.)
            switch phase {
            case .tracking, .interacting, .decelerating, .animating: yours = true
            default: yours = false
            }
        }
    }
}
