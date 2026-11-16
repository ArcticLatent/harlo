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
        if Replaying.enabled { return Replaying.model() }
        let m = HarloModel(store: KeychainStore(), deviceName: UIDevice.current.name)
        // The home screen widget follows the session list.
        m.widgetChanged = { WidgetCenter.shared.reloadTimelines(ofKind: "HarloSessions") }
        // The plan limits widget, harlo's plan limits.
        m.limitsWidgetChanged = { WidgetCenter.shared.reloadTimelines(ofKind: "HarloLimits") }
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
            // Tapped: its session (or the list, for a test; for a plan limit,
            // with the limits showing).
            if content.isLimit { UserDefaults.standard.set(true, forKey: DashboardView.showLimitsKey) }
            guard !content.session.isEmpty, !content.isLimit else { return }
            let summary = model.sessions.first { $0.key == content.session }
                ?? SessionSummary(["key": .string(content.session), "title": .string(content.title)])
            model.open(summary)
        }
    }
}
