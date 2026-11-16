import HarloKit
import SwiftUI
import UIKit
import WidgetKit

/// Shows each step's screen in turn and takes its picture.
struct TourView: View {
    @Environment(HarloModel.self) private var model
    @State private var step: Tour.Step = .pair
    @State private var unpaired = HarloModel(store: MemoryStore(), deviceName: "iPhone")
    @State private var demo: HarloModel = {
        let m = HarloModel(store: MemoryStore(), deviceName: "iPhone")
        m.startDemo()
        return m
    }()

    var body: some View {
        // (The task lives on the container; each screen is new.)
        ZStack { screen.id(step) }
            .task {
                for (n, style) in [(0, UIUserInterfaceStyle.dark), (1, UIUserInterfaceStyle.light)] {
                    Tour.setStyle(style)
                    for (i, s) in Tour.Step.allCases.enumerated() {
                        model.close()
                        if [.session, .approval, .models, .activity, .file, .branches].contains(s) { model.openSample(asking: s == .approval) }
                        step = s
                        try? await Task.sleep(for: .milliseconds(1500))
                        Tour.snapshot(String(format: "%02d-%@-%@", i + 1, s.rawValue, n == 0 ? "dark" : "light"))
                    }
                }
                try? Data().write(to: Tour.folder.appendingPathComponent("done"))
            }
    }

    @ViewBuilder var screen: some View {
        switch step {
        case .splash:
            SplashView(animated: false)
        case .pair:
            NavigationStack { PairView() }
                .environment(unpaired)
        case .demo:
            MainTabs(tab: .dashboard)
                .environment(demo)
        case .widget:
            WidgetTour()
        case .dashboard, .session, .approval:
            // (An open session shows in the dashboard's tab.)
            MainTabs(tab: .dashboard)
        case .newSession:
            MainTabs(tab: .newSession)
        case .approvals:
            MainTabs(tab: .approvals)
        case .models:
            if let s = model.session { ModelPicker(session: s) }
        case .changes:
            ChangesView()
        case .diff:
            NavigationStack { DiffView(file: Samples.standard.changes.files[0]) }
        case .file:
            FileView(path: "crates/harlo-core/src/sidebar.rs", line: 3)
        case .branches:
            BranchesView(busy: false) { _ in }
        case .activity:
            if let s = model.session { ActivityView(session: s) }
        case .settings:
            MainTabs(tab: .settings)
        }
    }
}
