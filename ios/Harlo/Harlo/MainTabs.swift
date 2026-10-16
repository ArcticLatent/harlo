import HarloKit
import SwiftUI

/// The app's four tabs.
enum AppTab: Hashable {
    case dashboard, newSession, approvals, settings
}

/// Paired: the dashboard (your sessions), a new session, what waits on you
/// (Approvals) and Settings. An open session shows in the tab it was opened
/// from (the dashboard's, or the new session's); one opened elsewhere (an
/// approval, a Live Activity) shows on the dashboard.
struct MainTabs: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @State var tab: AppTab = .dashboard
    /// The tab whose stack shows the open session (nil: none open).
    @State private var sessionTab: AppTab?

    var body: some View {
        TabView(selection: $tab) {
            NavigationStack {
                DashboardView(tab: $tab)
                    .navigationDestination(isPresented: presenting(.dashboard)) { SessionView() }
            }
            .tabItem { Label("Dashboard", systemImage: "house.fill") }
            .tag(AppTab.dashboard)

            NavigationStack {
                NewSessionView(tab: $tab)
                    .navigationDestination(isPresented: presenting(.newSession)) { SessionView() }
            }
            .tabItem { Label("New Session", systemImage: "plus.square.fill") }
            .tag(AppTab.newSession)

            NavigationStack {
                ApprovalsView()
            }
            .tabItem { Label("Approvals", systemImage: "checkmark.shield.fill") }
            .badge(model.waiting.count)
            .tag(AppTab.approvals)

            SettingsView(inTab: true)
                .tabItem { Label("Settings", systemImage: "gearshape.fill") }
                .tag(AppTab.settings)
        }
        .toolbarBackground(Color(p.sidebar), for: .tabBar)
        .toolbarBackground(.visible, for: .tabBar)
        .onChange(of: model.session == nil, initial: true) { _, closed in
            if closed {
                sessionTab = nil
            } else if sessionTab == nil {
                let here: AppTab = tab == .newSession ? .newSession : .dashboard
                sessionTab = here
                if tab != here { tab = here }
            }
        }
    }

    /// Whether `t`'s stack shows the open session; going back closes it.
    func presenting(_ t: AppTab) -> Binding<Bool> {
        Binding(get: { model.session != nil && sessionTab == t }, set: { if !$0 { model.close() } })
    }
}
