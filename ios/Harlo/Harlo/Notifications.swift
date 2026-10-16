import HarloKit
import SwiftUI
import UIKit
import UserNotifications
import WidgetKit

/// The app's model, and its notifications: the push token goes to harlo;
/// Allow / Deny on an approval answer it (in the background, the app
/// closed); tapping one opens its session.
@MainActor
final class AppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    lazy var model: HarloModel = {
        if Tour.enabled { return HarloModel.sample(busier: true) }
        let m = HarloModel(store: KeychainStore(), deviceName: UIDevice.current.name)
        // The home screen widget follows the session list.
        m.widgetChanged = { WidgetCenter.shared.reloadTimelines(ofKind: "HarloSessions") }
        return m
    }()

    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        let center = UNUserNotificationCenter.current()
        center.delegate = self
        let allow = UNNotificationAction(identifier: "allow", title: "Allow", options: [.authenticationRequired])
        let deny = UNNotificationAction(identifier: "deny", title: "Deny", options: [.destructive, .authenticationRequired])
        center.setNotificationCategories([
            UNNotificationCategory(identifier: "approval", actions: [allow, deny], intentIdentifiers: [], options: []),
            UNNotificationCategory(identifier: "question", actions: [], intentIdentifiers: [], options: []),
        ])
        return true
    }

    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        Task { await model.setPushToken(deviceToken, sandbox: Notifications.sandbox) }
    }

    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
        Notifications.problem = error.localizedDescription
    }

    // In front the app shows it all already; a test still shows.
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        Notifications.content(notification.request.content.userInfo)?.kind == "test" ? [.banner, .sound] : []
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        let info = response.notification.request.content.userInfo
        let action = response.actionIdentifier
        guard let content = Notifications.content(info) else { return }
        await handle(content, action: action)
    }

    private func handle(_ content: PushContent, action: String) async {
        switch action {
        case "allow", "deny":
            await model.answer(fromNotification: content, allow: action == "allow")
        default:
            // Tapped: its session (or the list, for a test).
            guard !content.session.isEmpty else { return }
            let summary = model.sessions.first { $0.key == content.session }
                ?? SessionSummary(["key": .string(content.session), "title": .string(content.title)])
            model.open(summary)
        }
    }
}

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
