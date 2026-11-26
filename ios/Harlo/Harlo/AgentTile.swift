import HarloKit
import SwiftUI

/// A session as a tile: its model's provider's logo on the provider's
/// colour (the app's palette from the environment).
struct AgentTile: View {
    @Environment(\.palette) private var p
    /// The model's provider, a logo's id ("" not known).
    var maker = ""
    var size: CGFloat = 44

    var body: some View {
        ProviderTile(maker: maker, size: size, palette: p)
    }
}
