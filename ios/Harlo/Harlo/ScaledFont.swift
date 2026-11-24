import HarloKit
import SwiftUI

/// The system font at `size` points at the default text size, growing and
/// shrinking with the phone's text size (Dynamic Type) as the nearest text
/// style does.
struct ScaledFont: ViewModifier {
    @ScaledMetric private var size: CGFloat
    private let weight: Font.Weight
    private let design: Font.Design

    init(size: CGFloat, weight: Font.Weight, design: Font.Design) {
        _size = ScaledMetric(wrappedValue: size, relativeTo: Self.style(for: size))
        self.weight = weight
        self.design = design
    }

    static func style(for size: CGFloat) -> Font.TextStyle {
        switch Style.textStyle(for: size) {
        case .title1: return .title
        case .title2: return .title2
        case .title3: return .title3
        case .body: return .body
        case .callout: return .callout
        case .subheadline: return .subheadline
        case .footnote: return .footnote
        case .caption1: return .caption
        default: return .caption2
        }
    }

    func body(content: Content) -> some View {
        content.font(.system(size: size, weight: weight, design: design))
    }
}
