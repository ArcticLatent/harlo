import HarloKit
import SwiftUI

/// The viewer's grid: every picture of the session, each over its name, the
/// one open ringed; a tap opens one.
struct PictureGrid: View {
    let paths: [String]
    let current: String
    let name: (String) -> String
    /// The viewer's top bar height, which the grid starts below.
    let top: CGFloat
    let open: (String) -> Void

    var body: some View {
        ScrollViewReader { reader in
            ScrollView {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 104), spacing: 12)], spacing: 16) {
                    ForEach(paths, id: \.self) { path in
                        PictureGridCell(path: path, name: name(path), open: path == current) { open(path) }
                            .id(path)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.top, top + 8)
                .padding(.bottom, 24)
            }
            .onAppear { reader.scrollTo(current, anchor: .center) }
        }
        .background(Color.black.ignoresSafeArea())
    }
}
