import HarloKit
import SwiftUI
import UIKit

/// An approval's answers, as the Approvals cards show them: Deny (red, an
/// outline), Approve (filled with the accent), any others plain; the go-ahead
/// last.
struct ApprovalButtons: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let request: UIRequest
    /// The session it's in (nil: the open one).
    var key: String? = nil

    var options: [String] {
        request.options.sorted { order($0) < order($1) }
    }

    var body: some View {
        if request.approval && !request.options.isEmpty {
            if options.count <= 2 {
                HStack(spacing: 10) {
                    ForEach(options, id: \.self) { o in button(o, UIRequest.role(of: o)) { answer(UIRequest.choose(o)) } }
                }
            } else {
                VStack(spacing: 8) {
                    ForEach(options, id: \.self) { o in button(o, UIRequest.role(of: o)) { answer(UIRequest.choose(o)) } }
                }
            }
        } else {
            HStack(spacing: 10) {
                button("Deny", .deny) { answer(request.approval(false)) }
                button("Approve", .allow) { answer(request.approval(true)) }
            }
        }
    }

    func order(_ option: String) -> Int {
        switch UIRequest.role(of: option) {
        case .deny: return 0
        case .plain: return 1
        case .allow: return 2
        }
    }

    func button(_ text: String, _ role: UIRequest.OptionRole, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 8) {
                switch role {
                case .allow: Image(systemName: "checkmark").scaledFont(15, .bold)
                case .deny: Image(systemName: "xmark").scaledFont(15, .semibold)
                case .plain: EmptyView()
                }
                Text(text).scaledFont(17, .semibold).lineLimit(1)
            }
            .frame(maxWidth: .infinity)
            .frame(minHeight: 48)
            .foregroundStyle(Color(role == .allow ? p.onPrimary : role == .deny ? p.error : p.foreground))
            .background { RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color(role == .allow ? p.primary : p.raised)) }
            .overlay { RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Color(role == .allow ? p.primary : p.hairline), lineWidth: 1) }
            .contentShape(RoundedRectangle(cornerRadius: 12))
        }
        .buttonStyle(.plain)
    }

    func answer(_ value: JSON) {
        UINotificationFeedbackGenerator().notificationOccurred(.success)
        Task {
            if let key { await model.answer(request, value, in: key) } else { await model.answer(request, value) }
        }
    }
}
