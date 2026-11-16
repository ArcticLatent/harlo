import HarloKit
import SwiftUI

/// Words that scroll past `cap`: when they do, their last line fades out
/// and the scroll bar shows for a moment, so it's clear there's more.
struct CappedWords: View {
    let text: String
    let cap: CGFloat
    @State private var height: CGFloat = 0

    var cut: Bool { height > cap + 1 }

    var body: some View {
        ScrollView {
            Text(text)
                .font(.subheadline)
                .frame(maxWidth: .infinity, alignment: .leading)
                .fixedSize(horizontal: false, vertical: true)
                .background {
                    GeometryReader { g in
                        Color.clear
                            .onAppear { height = g.size.height }
                            .onChange(of: g.size.height) { _, h in height = h }
                    }
                }
                .padding(.bottom, cut ? 14 : 0)
        }
        .frame(maxHeight: cap)
        .fixedSize(horizontal: false, vertical: true)
        .scrollIndicators(.visible)
        .scrollIndicatorsFlash(onAppear: true)
        .mask {
            LinearGradient(stops: [.init(color: .black, location: 0),
                                   .init(color: .black, location: cut ? 0.78 : 1),
                                   .init(color: .black.opacity(cut ? 0.1 : 1), location: 1)],
                           startPoint: .top, endPoint: .bottom)
        }
        .accessibilityElement(children: .combine)
    }
}
