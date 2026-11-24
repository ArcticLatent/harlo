import SwiftUI

/// Lays out a line of code after its indentation, `columns` wide in the
/// first subview's width (one hidden character of the code's font). Deep
/// nesting is narrowed (past 8 columns, half a column each) and never takes
/// more than a third of the row, so wrapped code keeps most of a phone's
/// width and the nesting still reads.
struct HangingIndent: Layout {
    let columns: Int

    /// The indentation's width in `width`, for one character `char` wide.
    func indent(_ char: CGFloat, in width: CGFloat?) -> CGFloat {
        let n = columns <= 8 ? CGFloat(columns) : 8 + CGFloat(columns - 8) / 2
        let natural = n * char
        guard let width, width.isFinite else { return natural }
        return min(natural, width / 3)
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        guard subviews.count == 2 else { return .zero }
        let char = subviews[0].sizeThatFits(.unspecified).width
        let lead = indent(char, in: proposal.width)
        let code = subviews[1].sizeThatFits(ProposedViewSize(width: proposal.width.map { max($0 - lead, char) }, height: nil))
        return CGSize(width: proposal.width ?? lead + code.width, height: code.height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard subviews.count == 2 else { return }
        let char = subviews[0].sizeThatFits(.unspecified).width
        let lead = indent(char, in: bounds.width)
        subviews[0].place(at: bounds.origin, proposal: .unspecified)
        subviews[1].place(at: CGPoint(x: bounds.minX + lead, y: bounds.minY), proposal: ProposedViewSize(width: max(bounds.width - lead, char), height: nil))
    }
}
