import SwiftUI

/// The app's corner radii: the desktop's, so cards, fields and buttons
/// have corners rather than round ends.
enum Corner {
    /// Buttons, chips, tags, code wells and thumbnails.
    static let small: CGFloat = 6
    /// Cards and fields.
    static let card: CGFloat = 8
    /// The message box and the cards that hold other cards.
    static let large: CGFloat = 10
}
