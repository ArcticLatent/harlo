import HarloKit
import SwiftUI
import UIKit

/// A rounded icon square (a tool call's kind: a file, an edit, a command).
struct IconSquare: View {
    @Environment(\.palette) private var p
    let symbol: String
    var size: CGFloat = 36

    var body: some View {
        RoundedRectangle(cornerRadius: 9, style: .continuous)
            .fill(Color(p.foreground).opacity(0.06))
            .frame(width: size, height: size)
            .overlay { Image(systemName: symbol).font(.system(size: size * 0.42)).foregroundStyle(Color(p.muted)) }
    }
}
