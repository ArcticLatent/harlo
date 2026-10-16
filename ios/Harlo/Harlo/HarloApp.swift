import HarloKit
import SwiftUI
import UIKit
import UserNotifications

/// harlo on the phone: pairs with harlo on your computer (its QR code) and
/// shows its sessions, end-to-end encrypted (HarloKit has the protocol).
@main
struct HarloApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @Environment(\.scenePhase) private var phase
    /// Wi-Fi to mobile data and back: check the connection at once.
    @State private var network: NetworkWatch?
    /// The logo's animation, on opening (not in the screenshots' tour).
    @State private var splash = !Tour.enabled

    var body: some Scene {
        WindowGroup {
            Group {
                if Tour.enabled {
                    TourView()
                } else {
                    RootView()
                }
            }
            // (Themed reads the model, so it goes inside.)
            .modifier(Themed())
            // On opening: the logo, then the app.
            .overlay {
                if splash {
                    SplashView { splash = false }
                }
            }
            .environment(delegate.model)
            // Connected while in front; iOS cuts sockets in the background.
            .onChange(of: phase, initial: true) { _, now in
                guard !Tour.enabled else { return }
                let model = delegate.model
                if network == nil {
                    network = NetworkWatch { Task { await model.networkChanged() } }
                }
                switch now {
                case .active:
                    model.resume()
                    Notifications.settle(waiting: model.waiting.count)
                    if model.host != nil, !model.demo { Task { await Notifications.register() } }
                case .background: model.suspend()
                default: break
                }
            }
            // While in front, the badge follows what waits.
            .onChange(of: delegate.model.waiting.count) { _, n in
                if phase == .active, !Tour.enabled { UNUserNotificationCenter.current().setBadgeCount(n) { _ in } }
            }
            // Just paired: ask for notifications.
            .onChange(of: delegate.model.host?.key) { _, key in
                if key != nil, !Tour.enabled, !delegate.model.demo { Task { await Notifications.register() } }
            }
            // Live Activities of sessions that stopped working end.
            .onChange(of: delegate.model.sessions) { _, list in
                if phase == .active, !Tour.enabled { LiveActivities.tidy(list) }
            }
            // A Live Activity tapped: its session (harlo://session?key=…).
            .onOpenURL { url in
                guard url.scheme == "harlo", url.host == "session",
                      let key = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?.first(where: { $0.name == "key" })?.value
                else { return }
                let model = delegate.model
                if model.session?.liveKey == key || model.session?.key == key { return }
                model.open(model.sessions.first { $0.key == key } ?? SessionSummary(["key": .string(key)]))
            }
        }
    }
}

struct RootView: View {
    @Environment(HarloModel.self) private var model

    var body: some View {
        Group {
            if model.host == nil {
                NavigationStack { PairView() }
            } else {
                MainTabs()
            }
        }
        .alert("harlo", isPresented: Binding(get: { model.alert != nil }, set: { if !$0 { model.alert = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(model.alert ?? "")
        }
    }
}
