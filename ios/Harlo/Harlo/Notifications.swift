import HarloKit
import SwiftUI
import UIKit
import UserNotifications

enum Notifications {
    /// An app run from Xcode gets development tokens; TestFlight's, production.
    static var sandbox: Bool {
        #if DEBUG
        return true
        #else
        return false
        #endif
    }

    /// Why the phone couldn't register for notifications (Settings shows it).
    @MainActor static var problem: String?

    /// What a notification says: as the extension opened it, or opened here
    /// (if the extension couldn't).
    static func content(_ info: [AnyHashable: Any]) -> PushContent? {
        if let text = info["harlo"] as? String, let v = try? JSON.parse(text) { return PushContent(v) }
        if let sealed = info["h"] as? String { return try? PushContent.open(sealed, store: KeychainStore()) }
        return nil
    }

    /// Asks once to show notifications (iOS asks the user the first time),
    /// then registers for them: harlo gets the token.
    @MainActor static func register() async {
        let center = UNUserNotificationCenter.current()
        var status = await center.notificationSettings().authorizationStatus
        if status == .notDetermined {
            _ = try? await center.requestAuthorization(options: [.alert, .badge, .sound])
            status = await center.notificationSettings().authorizationStatus
        }
        if status == .authorized || status == .provisional {
            UIApplication.shared.registerForRemoteNotifications()
        }
    }

    /// In front: the badge says what waits, and the notifications already
    /// seen go.
    @MainActor static func settle(waiting: Int) {
        let center = UNUserNotificationCenter.current()
        center.removeAllDeliveredNotifications()
        center.setBadgeCount(waiting) { _ in }
    }
}
