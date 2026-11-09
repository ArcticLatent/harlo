import Foundation
import HarloKit

/// The app on a recording of harlo instead of a harlo (its UI tests and
/// screenshots): launched with `-replay <scene>`, it shows what a real harlo
/// sent up to that scene (ios/fixtures/recording.jsonl, in Debug builds'
/// bundle; scripts/ios-record.sh makes it). Nothing is paired or kept, and
/// it asks for no notifications.
enum Replaying {
    static let scene: String? = {
        let a = ProcessInfo.processInfo.arguments
        guard let i = a.firstIndex(of: "-replay"), i + 1 < a.count else { return nil }
        return a[i + 1]
    }()

    static var enabled: Bool { scene != nil }

    /// Its model, or one saying why there's none.
    @MainActor
    static func model() -> HarloModel {
        guard let scene, let url = Bundle.main.url(forResource: "recording", withExtension: "jsonl"),
              let text = try? String(contentsOf: url, encoding: .utf8)
        else {
            let m = HarloModel(store: MemoryStore(), deviceName: "iPhone")
            m.alert = "There's no recording in this build (Debug builds have ios/fixtures/recording.jsonl)."
            return m
        }
        do {
            return HarloModel.replaying(try Replay(recording: text, scene: scene))
        } catch {
            let m = HarloModel(store: MemoryStore(), deviceName: "iPhone")
            m.alert = "\(error)"
            return m
        }
    }
}
