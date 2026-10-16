import HarloKit
import SwiftUI
import UIKit

/// A question or approval the agent waits on: above the composer, or in
/// "Needs you" (`key`: the session it's in, answered from there).
struct RequestCard: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let request: UIRequest
    var key: String? = nil
    /// Opens its session (a question too long to answer in a list).
    var open: (() -> Void)? = nil
    /// On an Approvals card: only the answer (the card shows the rest).
    var bare = false
    @State private var typed = ""
    @State private var picks: [Set<String>] = []
    @State private var custom: [String] = []

    var heading: String {
        if request.approval { return "Approve this?" }
        return request.title.isEmpty ? "The agent asks" : request.title
    }

    var body: some View {
        if bare {
            VStack(alignment: .leading, spacing: 10) {
                if !request.message.isEmpty {
                    Text(request.message)
                        .font(.system(size: 15))
                        .lineLimit(6)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                content
            }
            .onAppear(perform: reset)
        } else if key == nil {
            card
                .padding(14)
                .card(p, radius: 18)
                .padding(.horizontal, 12)
                .padding(.top, 8)
        } else {
            card
        }
    }

    var card: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label(heading, systemImage: request.approval ? "hand.raised" : "questionmark.bubble")
                .font(key == nil ? .headline : .subheadline.weight(.semibold))
                .foregroundStyle(Color(request.approval ? p.warning : p.primary))
            if !request.message.isEmpty {
                if key == nil {
                    ScrollView {
                        Text(request.message)
                            .font(Style.mono(12))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .textSelection(.enabled)
                    }
                    .frame(maxHeight: 140)
                    .fixedSize(horizontal: false, vertical: true)
                } else {
                    Text(request.message)
                        .font(Style.mono(12))
                        .lineLimit(6)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            content
        }
        .onAppear(perform: reset)
    }

    func reset() {
        typed = request.prefill
        picks = Array(repeating: [], count: request.questions.count)
        custom = Array(repeating: "", count: request.questions.count)
    }

    @ViewBuilder var content: some View {
        if request.approval || request.method == "confirm" {
            ApprovalButtons(request: request, key: key)
        } else if request.method == "select" {
            VStack(spacing: 6) {
                ForEach(request.options, id: \.self) { o in
                    Button {
                        answer(UIRequest.choose(o))
                    } label: {
                        Text(o).frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .buttonStyle(.bordered)
                }
            }
        } else if request.method == "input" || request.method == "editor" {
            TextField(request.placeholder, text: $typed, axis: .vertical)
                .lineLimit(1...5)
                .textFieldStyle(.roundedBorder)
            Button("Send") { answer(UIRequest.typed(typed)) }
                .buttonStyle(.borderedProminent)
                .foregroundStyle(Color(p.onPrimary))
        } else if request.method == "ask" && !request.questions.isEmpty, let open {
            Button {
                open()
            } label: {
                Text(request.questions.count == 1 ? "Answer the question" : "Answer \(request.questions.count) questions")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
        } else if request.method == "ask" && !request.questions.isEmpty {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    ForEach(request.questions.indices, id: \.self) { i in
                        question(i)
                    }
                }
            }
            .frame(maxHeight: 320)
            .fixedSize(horizontal: false, vertical: true)
            Button("Answer") {
                answer(UIRequest.askAnswer(request.questions, picked: picks.map { Array($0) }, typed: custom))
            }
            .buttonStyle(.borderedProminent)
            .foregroundStyle(Color(p.onPrimary))
        } else {
            Button("Dismiss") { answer(UIRequest.dismissed) }
                .buttonStyle(.bordered)
        }
    }

    @ViewBuilder func question(_ i: Int) -> some View {
        let q = request.questions[i]
        VStack(alignment: .leading, spacing: 6) {
            if !q.header.isEmpty {
                Text(q.header.uppercased())
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(Color(p.muted))
            }
            Text(q.question)
                .font(.subheadline)
            ForEach(q.options, id: \.self) { o in
                let on = i < picks.count && picks[i].contains(o)
                Button {
                    toggle(i, o, multi: q.multi)
                } label: {
                    HStack(spacing: 8) {
                        Image(systemName: q.multi ? (on ? "checkmark.square.fill" : "square") : (on ? "largecircle.fill.circle" : "circle"))
                            .foregroundStyle(Color(on ? p.primary : p.muted))
                        Text(o)
                            .multilineTextAlignment(.leading)
                        Spacer(minLength: 0)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            TextField("Or type your answer", text: Binding(
                get: { i < custom.count ? custom[i] : "" },
                set: { if i < custom.count { custom[i] = $0 } }
            ))
            .textFieldStyle(.roundedBorder)
        }
    }

    func toggle(_ i: Int, _ o: String, multi: Bool) {
        guard i < picks.count else { return }
        if picks[i].contains(o) {
            picks[i].remove(o)
        } else {
            if !multi { picks[i].removeAll() }
            picks[i].insert(o)
        }
    }

    func answer(_ value: JSON) {
        UINotificationFeedbackGenerator().notificationOccurred(.success)
        Task {
            if let key { await model.answer(request, value, in: key) } else { await model.answer(request, value) }
        }
    }
}

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
                case .allow: Image(systemName: "checkmark").font(.system(size: 15, weight: .bold))
                case .deny: Image(systemName: "xmark").font(.system(size: 15, weight: .semibold))
                case .plain: EmptyView()
                }
                Text(text).font(.system(size: 17, weight: .semibold)).lineLimit(1)
            }
            .frame(maxWidth: .infinity)
            .frame(height: 48)
            .foregroundStyle(Color(role == .allow ? p.onPrimary : role == .deny ? p.error : p.foreground))
            .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color(role == .allow ? p.primary : p.raised)))
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Color(role == .allow ? p.primary : p.hairline), lineWidth: 1))
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
