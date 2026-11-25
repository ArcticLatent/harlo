import HarloKit
import SwiftUI
import UIKit

/// An approval's answers, as the Approvals cards show them: Deny (red, an
/// outline), Approve (filled with the accent), any others plain; the go-ahead
/// last. A refusal can say what the agent should do instead: it's told,
/// and carries on.
struct ApprovalButtons: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.dynamicTypeSize) private var typeSize
    let request: UIRequest
    /// The session it's in (nil: the open one).
    var key: String? = nil
    /// Saying why it's refused (the card around makes room for the
    /// keyboard meanwhile).
    @Binding var explaining: Bool
    @State private var reason = ""
    @FocusState private var typing: Bool

    var options: [String] {
        request.options.sorted { order($0) < order($1) }
    }

    /// Harlo on the computer passes a refusal's reason on to the agent.
    var takesReason: Bool { request.approval && model.takesReasons }

    /// Harlo's own approval (once, always, deny) where the text isn't at
    /// its largest: the two answers for now over the two that say more.
    var paired: Bool {
        takesReason && !typeSize.isAccessibilitySize && once != nil && always != nil && deny != nil && request.options.count == 3
    }

    var deny: String? { request.options.first { UIRequest.role(of: $0) == .deny } }
    /// The go-ahead that is kept ("Allow always").
    var always: String? { request.options.first { UIRequest.role(of: $0) == .allow && $0.lowercased().contains("always") } }
    /// The go-ahead for this once.
    var once: String? { request.options.first { UIRequest.role(of: $0) == .allow && $0 != always } }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if explaining {
                explain
            } else {
                if paired, let deny, let once, let always {
                    HStack(spacing: 10) {
                        button(deny, .deny) { answer(UIRequest.choose(deny)) }
                        button(once, .allow) { answer(UIRequest.choose(once)) }
                    }
                    HStack(spacing: 10) {
                        button("Deny and say why", .deny, quiet: true) { explaining = true }
                        button(always, .allow, quiet: true) { answer(UIRequest.choose(always)) }
                    }
                } else {
                    buttons
                    if takesReason {
                        button("Deny and say why", .deny, quiet: true) { explaining = true }
                    }
                }
                if !request.always.isEmpty {
                    Text(request.always)
                        .font(.footnote)
                        .foregroundStyle(Color(p.muted))
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        // (While its answer is on its way: one answer per request.)
        .disabled(model.answering.contains(request.id))
        .opacity(model.answering.contains(request.id) ? 0.55 : 1)
    }

    /// The refusal with its reason: what the agent should do instead.
    @ViewBuilder var explain: some View {
        TextField("What should it do instead?", text: $reason, axis: .vertical)
            .lineLimit(1...4)
            .answerField(p)
            .focused($typing)
            .onAppear { typing = true }
            .accessibilityIdentifier("denyReason")
        HStack(spacing: 10) {
            button("Back", .plain) {
                typing = false
                explaining = false
            }
            button("Deny", .deny) { answer(request.refusal(saying: reason)) }
        }
    }

    @ViewBuilder var buttons: some View {
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

    /// `quiet`: an answer that says more than yes or no (smaller, an
    /// outline, its role only in its colour).
    func button(_ text: String, _ role: UIRequest.OptionRole, quiet: Bool = false, action: @escaping () -> Void) -> some View {
        let filled = role == .allow && !quiet
        let ink = filled ? p.onPrimary : role == .deny ? p.error : role == .allow ? p.primary : p.foreground
        return Button(action: action) {
            HStack(spacing: 8) {
                if !quiet {
                    switch role {
                    case .allow: Image(systemName: "checkmark").scaledFont(15, .bold)
                    case .deny: Image(systemName: "xmark").scaledFont(15, .semibold)
                    case .plain: EmptyView()
                    }
                }
                Text(text)
                    .scaledFont(quiet ? 15 : 17, quiet ? .medium : .semibold)
                    .lineLimit(1)
                    .minimumScaleFactor(quiet ? 0.8 : 1)
            }
            .frame(maxWidth: .infinity)
            .frame(minHeight: quiet ? 44 : 48)
            .foregroundStyle(Color(ink))
            .background { RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color(filled ? p.primary : p.raised)) }
            .overlay { RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Color(filled ? p.primary : p.hairline), lineWidth: 1) }
            .contentShape(RoundedRectangle(cornerRadius: 12))
        }
        .buttonStyle(.plain)
    }

    func answer(_ value: JSON) {
        Task {
            let taken = if let key { await model.answer(request, value, in: key) } else { await model.answer(request, value) }
            // (Felt once harlo has it, not on the tap: an answer that didn't
            // get there stays to be sent again.)
            if taken { UINotificationFeedbackGenerator().notificationOccurred(.success) }
        }
    }
}
