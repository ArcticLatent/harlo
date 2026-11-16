import HarloKit
import SwiftUI

/// Paired: the dashboard (your sessions), a new session, what waits on you
/// (Approvals) and Settings. An open session shows in the tab it was opened
/// from (the dashboard's, or the new session's); one opened elsewhere (an
/// approval, a Live Activity) shows on the dashboard.
struct MainTabs: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @State private var tab: AppTab
    /// The tab whose stack shows the open session (nil: none open).
    @State private var sessionTab: AppTab?

    init(tab: AppTab = .dashboard) {
        _tab = State(initialValue: tab)
    }

    var body: some View {
        TabView(selection: $tab) {
            Tab("Dashboard", systemImage: "house.fill", value: .dashboard) {
                NavigationStack {
                    DashboardView(tab: $tab)
                        .navigationDestination(isPresented: presenting(.dashboard)) { SessionView() }
                }
            }

            Tab("New Session", systemImage: "plus.square.fill", value: .newSession) {
                NavigationStack {
                    NewSessionView(tab: $tab)
                        .navigationDestination(isPresented: presenting(.newSession)) { SessionView() }
                }
            }

            Tab("Approvals", systemImage: "checkmark.shield.fill", value: .approvals) {
                NavigationStack {
                    ApprovalsView()
                }
            }
            .badge(model.waiting.count)

            Tab("Settings", systemImage: "gearshape.fill", value: .settings) {
                SettingsView(inTab: true)
            }
        }
        .toolbarBackground(Color(p.sidebar), for: .tabBar)
        .toolbarBackgroundVisibility(.visible, for: .tabBar)
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
