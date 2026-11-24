import HarloKit
import SwiftUI
import UIKit

/// The photos going with the next message, each removable.
struct PhotoStrip: View {
    @Environment(\.palette) private var p
    @Binding var photos: [Photo]
    /// On their way to harlo: dimmed, each with a spinner, none removable.
    var sending = false

    var body: some View {
        ScrollView(.horizontal) {
            HStack(spacing: 8) {
                ForEach(photos) { photo in
                    ZStack(alignment: .topTrailing) {
                        if let image = UIImage(data: photo.data) {
                            Image(uiImage: image)
                                .resizable()
                                .scaledToFill()
                                .frame(width: 56, height: 56)
                                .clipShape(RoundedRectangle(cornerRadius: 8))
                                .overlay { RoundedRectangle(cornerRadius: 8).strokeBorder(Color(p.border), lineWidth: 1) }
                                .opacity(sending ? 0.45 : 1)
                                .overlay { if sending { ProgressView().controlSize(.small) } }
                        }
                        if !sending {
                        Button {
                            photos.removeAll { $0.id == photo.id }
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .scaledFont(18)
                                .symbolRenderingMode(.palette)
                                .foregroundStyle(Color.white, Color.black.opacity(0.6))
                        }
                        .offset(x: 6, y: -6)
                        .accessibilityLabel("Remove photo")
                        }
                    }
                }
            }
            .padding(.top, 6)
            .padding(.horizontal, 2)
        }
        .scrollIndicators(.hidden)
    }
}
