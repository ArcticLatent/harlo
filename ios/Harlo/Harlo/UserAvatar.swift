import HarloKit
import SwiftUI
import UIKit

/// You: the penguin picked in harlo on the computer (its left rail).
struct UserAvatar: View {
    @Environment(HarloModel.self) private var model
    var size: CGFloat = 32

    var body: some View {
        Image("Penguin-" + (model.theme ?? .standard).penguin)
            .resizable()
            .interpolation(.high)
            .scaledToFit()
            .frame(width: size, height: size)
            .clipShape(Circle())
            .accessibilityLabel("You")
    }
}
