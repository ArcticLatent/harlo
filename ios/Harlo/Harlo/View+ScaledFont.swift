import HarloKit
import SwiftUI

extension View {
    /// The system font at `size` (at the default text size), following the
    /// phone's text size.
    func scaledFont(_ size: CGFloat, _ weight: Font.Weight = .regular, design: Font.Design = .default) -> some View {
        modifier(ScaledFont(size: size, weight: weight, design: design))
    }
}
