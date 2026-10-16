import HarloKit
import SwiftUI
import UIKit
import WidgetKit

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
                        try? await Task.sleep(nanoseconds: 1_500_000_000)
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

/// The home screen widget in its three sizes, on a plain wallpaper, as
/// WidgetKit draws it (its views are shared with the widget).
struct WidgetTour: View {
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        let snap = WidgetSnapshot.sample()
        ZStack {
            LinearGradient(colors: scheme == .dark ? [Color(red: 0.13, green: 0.14, blue: 0.24), Color(red: 0.24, green: 0.2, blue: 0.4)]
                                                   : [Color(red: 0.78, green: 0.82, blue: 0.95), Color(red: 0.93, green: 0.85, blue: 0.95)],
                           startPoint: .top, endPoint: .bottom)
                .ignoresSafeArea()
            VStack(spacing: 22) {
                HStack(spacing: 22) {
                    box(snap, .systemSmall, width: 170, height: 170)
                    Spacer(minLength: 0)
                }
                box(snap, .systemMedium, width: 364, height: 170)
                box(snap, .systemLarge, width: 364, height: 382)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 20)
            .padding(.top, 20)
        }
    }

    func box(_ snap: WidgetSnapshot, _ family: WidgetFamily, width: CGFloat, height: CGFloat) -> some View {
        SessionsWidgetView(snapshot: snap, family: family)
            .padding(16)
            .frame(width: width, height: height, alignment: .topLeading)
            .background(SessionsTourBackground())
            .clipShape(RoundedRectangle(cornerRadius: 24, style: .continuous))
    }
}

/// The widget's background (as the widget's own), for the tour.
struct SessionsTourBackground: View {
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        let p = Theme.standard.palette(dark: scheme == .dark)
        let c = p.background.mix(p.foreground, scheme == .dark ? 0.03 : 0)
        Color(.sRGB, red: c.r, green: c.g, blue: c.b, opacity: 1)
    }
}
