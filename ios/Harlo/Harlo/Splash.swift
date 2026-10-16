import SwiftUI

/// harlo's logo: the H (two agents, mauve and blue, interlocking) and the
/// "Harlo" wordmark beside it, as in Harlo-Logo.svg. Set the height.
/// `joined` false: the agents apart (the splash brings them together);
/// `named` false: the wordmark hidden.
struct HarloLogo: View {
    var height: CGFloat = 56
    var wordColor = Color(red: 0xCA / 255, green: 0xD3 / 255, blue: 0xF5 / 255)
    var joined = true
    var named = true

    var body: some View {
        // (The logo's units: the H is 124 × 128, then a 59 gap, then the 401-wide wordmark.)
        let k = height / 128
        HStack(spacing: 59 * k) {
            ZStack {
                Image("SplashMauve").resizable().scaledToFit()
                    .offset(x: joined ? 0 : -44 * k)
                Image("SplashBlue").resizable().scaledToFit()
                    .offset(x: joined ? 0 : 44 * k)
            }
            .frame(width: 124 * k, height: height)
            .opacity(joined ? 1 : 0)
            Image("Wordmark")
                .resizable()
                .renderingMode(.template)
                .scaledToFit()
                .foregroundStyle(wordColor)
                .frame(width: 401 * k, height: height)
                .opacity(named ? 1 : 0)
                .offset(x: named ? 0 : -14 * k)
        }
        .accessibilityElement()
        .accessibilityLabel("Harlo")
    }
}

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
