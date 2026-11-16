import HarloKit
import SwiftUI
import UIKit

/// Parsed rich text, kept by its HTML (rows re-render as others stream),
/// the paths in it linked (tapping one opens the file).
@MainActor
enum RichCache {
    private static var cache: [String: [RichBlock]] = [:]

    static func blocks(_ html: String) -> [RichBlock] {
        if let b = cache[html] { return b }
        if cache.count > 2000 { cache.removeAll() }
        let b = FileLinks.link(RichText.parse(html))
        cache[html] = b
        return b
    }
}
