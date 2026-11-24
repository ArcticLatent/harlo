import HarloKit
import SwiftUI
import UIKit

/// Screenshots without a Mac: launched with `-tour` (CI does, in the iOS
/// simulator), the app shows each screen with sample content, in dark and
/// light, and saves a picture of each to Documents/tour (then `done`).
enum Tour {
    static let enabled = ProcessInfo.processInfo.arguments.contains("-tour")

    enum Step: String, CaseIterable {
        case splash, pair, demo, widget, dashboard, session, approval, newSession, approvals, models, changes, diff, file, branches, activity, settings
    }

    static var folder: URL {
        FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("tour")
    }

    /// The app's window as it looks now.
    @MainActor
    static func snapshot(_ name: String) {
        guard let window = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).flatMap(\.windows).first(where: \.isKeyWindow)
        else { return }
        let image = UIGraphicsImageRenderer(bounds: window.bounds).image { _ in
            window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
        }
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try? image.pngData()?.write(to: folder.appendingPathComponent(name + ".png"))
    }

    @MainActor
    static func setStyle(_ style: UIUserInterfaceStyle) {
        for scene in UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }) {
            for w in scene.windows { w.overrideUserInterfaceStyle = style }
        }
    }
}
