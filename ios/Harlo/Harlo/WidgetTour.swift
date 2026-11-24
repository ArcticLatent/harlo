import HarloKit
import SwiftUI
import WidgetKit

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
                    // (The plan limits widget beside it.)
                    LimitsWidgetView(snapshot: .sample(), family: .systemSmall)
                        .padding(16)
                        .frame(width: 170, height: 170, alignment: .topLeading)
                        .background { SessionsTourBackground() }
                        .clipShape(RoundedRectangle(cornerRadius: 24, style: .continuous))
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
            .background { SessionsTourBackground() }
            .clipShape(RoundedRectangle(cornerRadius: 24, style: .continuous))
    }
}
