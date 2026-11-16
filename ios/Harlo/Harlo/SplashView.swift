import SwiftUI

/// On opening: the logo in the middle, on harlo's navy (the launch screen's
/// colour, so it follows on without a jump): the H's two agents slide
/// together, the wordmark fades in beside them, then it all fades away into
/// the app. With Reduce Motion, the logo just shows and fades.
struct SplashView: View {
    /// false: the logo as it ends (the screenshots).
    var animated = true
    var done: () -> Void = {}
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var joined = false
    @State private var named = false
    @State private var gone = false

    var body: some View {
        ZStack {
            Color("LaunchBackground").ignoresSafeArea()
            HarloLogo(height: 58, joined: joined, named: named)
                .scaleEffect(gone ? 1.06 : 1)
        }
        .opacity(gone ? 0 : 1)
        .allowsHitTesting(!gone)
        .task {
            guard animated else {
                joined = true
                named = true
                return
            }
            if reduceMotion {
                joined = true
                named = true
                try? await Task.sleep(for: .milliseconds(700))
            } else {
                try? await Task.sleep(for: .milliseconds(120))
                withAnimation(.spring(response: 0.55, dampingFraction: 0.78)) { joined = true }
                try? await Task.sleep(for: .milliseconds(320))
                withAnimation(.easeOut(duration: 0.45)) { named = true }
                try? await Task.sleep(for: .milliseconds(900))
            }
            withAnimation(.easeInOut(duration: 0.45)) { gone = true }
            try? await Task.sleep(for: .milliseconds(470))
            done()
        }
    }
}
