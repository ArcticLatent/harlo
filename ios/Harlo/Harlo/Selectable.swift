import HarloKit
import SwiftUI

/// Text selection, on or off (code wrapped at its breaks isn't selectable:
/// what it copied would carry them).
struct Selectable: ViewModifier {
    let on: Bool

    func body(content: Content) -> some View {
        if on { content.textSelection(.enabled) } else { content.textSelection(.disabled) }
    }
}
