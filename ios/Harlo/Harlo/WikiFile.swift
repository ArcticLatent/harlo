import HarloKit
import SwiftUI

/// A code link's file, at its line.
struct WikiFile: Identifiable {
    var path: String
    var line: Int
    var id: String { "\(path):\(line)" }
}
