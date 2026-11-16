import HarloKit
import SwiftUI
import UIKit

/// A file's or a diff's view options, behind "…": long lines wrapped (a
/// checkmark when they are) or kept whole to scroll across, and copying
/// the file when there's one to copy.
struct ViewMenu: View {
    @Binding var wrap: Bool
    var copy: (() -> Void)? = nil

    var body: some View {
        Menu {
            Toggle("Wrap Long Lines", isOn: $wrap)
            if let copy {
                Button(action: copy) {
                    Label("Copy File", systemImage: "doc.on.doc")
                }
            }
        } label: {
            Image(systemName: "ellipsis.circle")
        }
        .accessibilityLabel("View options")
    }
}
