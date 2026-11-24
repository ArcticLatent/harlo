import HarloKit
import SwiftUI
import UIKit

/// A screen's head: harlo's mark and your computer's name (its connection
/// as a dot), and the bell: what needs you (a red dot), tap for Approvals.
struct AppHeader: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    var bell: (() -> Void)?
    /// The dashboard's plan limits shown (nil: no button).
    var limits: Binding<Bool>?

    var body: some View {
        HStack(spacing: 10) {
            Image("Logo")
                .resizable()
                .scaledToFit()
                .frame(width: 24, height: 24)
            Text(model.host?.name ?? "harlo")
                .scaledFont(19, .medium)
                .lineLimit(1)
                .accessibilityLabel("\(model.host?.name ?? "harlo"), \(model.state == .connected ? "connected" : "not connected")")
                .accessibilityIdentifier("appHeaderName")
            StatusDot(status: model.state == .connected ? "live" : "exited")
                .accessibilityHidden(true)
            Spacer(minLength: 8)
            // (Only when harlo says limits.)
            if let limits, !model.limits.isEmpty {
                let near = model.limits.machines.contains { $0.accounts.contains { $0.windows.contains(where: \.nearly) } }
                Button {
                    withAnimation(.easeOut(duration: 0.2)) { limits.wrappedValue.toggle() }
                } label: {
                    Image(systemName: "gauge.with.dots.needle.67percent")
                        .font(.system(size: 17, weight: .regular))
                        .frame(width: 40, height: 40)
                        .background { Circle().fill(Color(limits.wrappedValue ? p.accentSoft.over(p.raised) : p.raised)) }
                        .overlay(alignment: .topTrailing) {
                            if near {
                                Circle().fill(Color(p.warning)).frame(width: 9, height: 9).offset(x: -6, y: 6)
                            }
                        }
                        .foregroundStyle(Color(limits.wrappedValue ? p.primary : p.foreground))
                }
                .buttonStyle(.plain)
                .sensoryFeedback(.selection, trigger: limits.wrappedValue)
                .accessibilityLabel(limits.wrappedValue ? "Hide plan limits" : "Show plan limits")
            }
            if let bell {
                Button(action: bell) {
                    Image(systemName: "bell")
                        .font(.system(size: 18, weight: .regular))
                        .frame(width: 40, height: 40)
                        .background { Circle().fill(Color(p.raised)) }
                        .overlay(alignment: .topTrailing) {
                            if !model.waiting.isEmpty {
                                Circle().fill(Color(p.error)).frame(width: 9, height: 9).offset(x: -6, y: 6)
                            }
                        }
                        .foregroundStyle(Color(p.foreground))
                }
                .buttonStyle(.plain)
                .accessibilityLabel(model.waiting.isEmpty ? "Approvals" : "\(model.waiting.count) waiting for you")
            }
        }
        // (The buttons' height with or without them, so the name sits in
        // the same place on every tab.)
        .frame(minHeight: 40)
    }
}
