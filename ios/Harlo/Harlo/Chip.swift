import HarloKit
import SwiftUI
import UIKit

/// A small chip button (the composer's model, thinking, mode, Plan).
struct Chip: View {
    @Environment(\.palette) private var p
    let icon: String
    let text: String
    var on = false

    var body: some View {
        HStack(spacing: 5) {
            Image(systemName: icon).scaledFont(11, .semibold)
            Text(text).scaledFont(13, .medium).lineLimit(1)
        }
        .padding(.horizontal, 10)
        .frame(minHeight: 30)
        .background { RoundedRectangle(cornerRadius: Corner.small).fill(Color(on ? p.accentSoft.over(p.raised) : p.raised.mix(p.foreground, 0.04))) }
        .overlay { RoundedRectangle(cornerRadius: Corner.small).strokeBorder(Color(on ? p.primary.mix(p.raised, 0.4) : p.hairline), lineWidth: 1) }
        .foregroundStyle(Color(on ? p.primary : p.muted))
        .contentShape(RoundedRectangle(cornerRadius: Corner.small))
    }
}
