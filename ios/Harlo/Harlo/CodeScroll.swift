import HarloKit
import SwiftUI
import UIKit

/// Code lines' container: as they are when wrapped, else in a sideways
/// scroll.
struct CodeScroll<Content: View>: View {
    let wrap: Bool
    @ViewBuilder let content: Content

    var body: some View {
        if wrap {
            content
        } else {
            ScrollView(.horizontal) { content }.scrollIndicators(.hidden)
        }
    }
}
