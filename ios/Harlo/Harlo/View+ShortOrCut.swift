import HarloKit
import SwiftUI
import UIKit

extension View {
    /// A line at the end of a row: short, its own size; long (a question's
    /// answers, an error), cut at the end to a third of the phone's width.
    /// (Kept whole, a long one was wider than its card and widened the
    /// whole session past the screen's sides.)
    @ViewBuilder func shortOrCut(_ text: String) -> some View {
        if text.count <= 20 {
            fixedSize()
        } else {
            truncationMode(.tail).frame(maxWidth: 130, alignment: .trailing)
        }
    }
}
