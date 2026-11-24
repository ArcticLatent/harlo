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
