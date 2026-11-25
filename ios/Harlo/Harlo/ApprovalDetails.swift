import HarloKit
import SwiftUI

/// What an approval would act on, as harlo on the computer words it: the
/// command or the paths, why it has to ask, and the change an edit would
/// make.
struct ApprovalDetails: View {
    @Environment(\.palette) private var p
    @Environment(\.dynamicTypeSize) private var typeSize
    let request: UIRequest
    /// On a card in a list: short, and nothing scrolls (its session shows
    /// all of it).
    var brief = false
    /// Only what it acts on (while a refusal's reason is typed, the
    /// keyboard takes the room).
    var folded = false

    /// The lines of a change a list's card shows.
    static let briefLines = 8

    /// How many lines of the change a list's card leaves out.
    var left: Int {
        max(0, request.diff.split(separator: "\n", omittingEmptySubsequences: false).count - Self.briefLines)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if !request.detail.isEmpty {
                Text(request.detail)
                    .font(Style.mono(13))
                    .lineLimit(brief ? 4 : 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .textSelection(.enabled)
                    .padding(10)
                    .background { RoundedRectangle(cornerRadius: 10).fill(Color(p.codeBackground)) }
            }
            if !request.why.isEmpty && !folded {
                Text(request.why)
                    .font(.subheadline)
                    .foregroundStyle(Color(p.muted))
                    .lineLimit(brief ? 3 : nil)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if !request.diff.isEmpty && !folded {
                if brief {
                    DiffLines(diff: request.diff, limit: Self.briefLines)
                    if left > 0 {
                        Text(left == 1 ? "1 more line in the session" : "\(left) more lines in the session")
                            .font(.footnote)
                            .foregroundStyle(Color(p.muted))
                    }
                } else {
                    ScrollView {
                        DiffLines(diff: request.diff)
                    }
                    .frame(maxHeight: typeSize.isAccessibilitySize ? 90 : 140)
                    .fixedSize(horizontal: false, vertical: true)
                    .scrollIndicators(.visible)
                    .accessibilityIdentifier("approvalDiff")
                }
            }
        }
    }
}
