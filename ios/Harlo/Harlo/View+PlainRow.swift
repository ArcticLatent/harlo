import HarloKit
import SwiftUI

extension View {
    /// A list row with no background, separator or default insets (the
    /// dashboard's and Approvals' cards).
    func plainRow(top: CGFloat = 4, bottom: CGFloat = 4) -> some View {
        listRowInsets(EdgeInsets(top: top, leading: 16, bottom: bottom, trailing: 16))
            .listRowBackground(Color.clear)
            .listRowSeparator(.hidden)
    }
}
