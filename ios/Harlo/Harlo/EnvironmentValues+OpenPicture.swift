import HarloKit
import SwiftUI

// The conversation's pictures: a photo you sent, one a tool returned or a
// model made, a picture file a reply names. Thumbnails under their row; a
// tap opens the viewer, which steps through the session's pictures.
extension EnvironmentValues {
    /// Opens one of the session's pictures in the viewer (the session view
    /// sets it).
    @Entry var openPicture: (@MainActor (String) -> Void)? = nil
}
