import HarloKit
import UserNotifications
import WidgetKit

/// Opens harlo's notifications on the phone: what they say comes sealed for
/// this phone (Apple carries a placeholder), opened with the keys the app
/// keeps in the Keychain (its group, shared with this extension).
final class NotificationService: UNNotificationServiceExtension {
    override func didReceive(_ request: UNNotificationRequest, withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void) {
        guard let content = request.content.mutableCopy() as? UNMutableNotificationContent,
              let sealed = request.content.userInfo["h"] as? String,
              let opened = try? PushContent.open(sealed, store: KeychainStore(accessGroup: Self.keychainGroup))
        else {
            contentHandler(request.content)
            return
        }
        // The home screen widget: this session needs you, or finished.
        let store = KeychainStore(accessGroup: Self.keychainGroup)
        if var snap = WidgetSnapshot.load(store), !snap.host.isEmpty {
            snap.apply(opened)
            snap.save(store)
            WidgetCenter.shared.reloadTimelines(ofKind: "HarloSessions")
        }
        let lines = opened.lines
        content.title = lines.title
        content.subtitle = lines.subtitle
        content.body = lines.body
        content.threadIdentifier = opened.session
        content.categoryIdentifier = opened.kind == "ask" ? (opened.canApprove ? "approval" : "question") : ""
        // For the app: Allow / Deny and opening the session.
        if let data = try? opened.json.data(), let text = String(data: data, encoding: .utf8) {
            content.userInfo["harlo"] = text
        }
        contentHandler(content)
    }

    /// The app's Keychain group (its team, from the build).
    static var keychainGroup: String? {
        (Bundle.main.object(forInfoDictionaryKey: "HarloTeam") as? String).flatMap { $0.isEmpty ? nil : "\($0).com.arcticlatent.harlo" }
    }
}
