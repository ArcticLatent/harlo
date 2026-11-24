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
    @Environment(\.dynamicTypeSize) private var typeSize
    @State private var typed = ""
    @State private var picks: [Set<String>] = []
    @State private var custom: [String] = []

    /// At the largest text sizes its scrolling parts are shorter, so the
    /// card leaves the conversation room (its answers stay in view).
    var big: Bool { typeSize.isAccessibilitySize }

    var heading: String {
        if request.approval { return "Approve this?" }
        return request.title.isEmpty ? "The agent asks" : request.title
    }

    var body: some View {
        if bare {
            VStack(alignment: .leading, spacing: 10) {
                if !request.message.isEmpty {
                    Text(request.message)
                        .scaledFont(15)
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
            // Why it asks (Harlo's: a destructive command, one outside the
            // sandbox).
            if request.approval && !request.note.isEmpty {
                Text(request.note)
                    .font(.subheadline)
                    .foregroundStyle(Color(p.warning))
            }
            if !request.message.isEmpty {
                if key == nil {
                    ScrollView {
                        Text(request.message)
                            .font(Style.mono(12))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .textSelection(.enabled)
                    }
                    .frame(maxHeight: big ? 90 : 140)
                    .fixedSize(horizontal: false, vertical: true)
                } else {
                    Text(request.message)
                        .font(Style.mono(12))
                        .lineLimit(6)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            content
                // (While its answer is on its way: one answer per request.)
                .disabled(model.answering.contains(request.id))
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
                .answerField(p)
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
        } else if request.method == "ask" && request.questions.count == 1 && big {
            // (At the largest text sizes only the question's words scroll:
            // its choices and Answer stay in view.)
            question(0, wordsCap: 110)
            Button("Answer") {
                answer(UIRequest.askAnswer(request.questions, picked: picks.map { Array($0) }, typed: custom))
            }
            .buttonStyle(.borderedProminent)
            .foregroundStyle(Color(p.onPrimary))
        } else if request.method == "ask" && !request.questions.isEmpty {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    ForEach(request.questions.indices, id: \.self) { i in
                        question(i)
                    }
                }
            }
            .frame(maxHeight: big ? 190 : 320)
            .fixedSize(horizontal: false, vertical: true)
            .scrollIndicators(.visible)
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

    /// A question, its choices and a field for your own answer; `wordsCap`:
    /// the question's words scroll past that height.
    @ViewBuilder func question(_ i: Int, wordsCap: CGFloat? = nil) -> some View {
        let q = request.questions[i]
        VStack(alignment: .leading, spacing: 6) {
            if !q.header.isEmpty {
                Text(q.header.uppercased())
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(Color(p.muted))
            }
            if let wordsCap {
                CappedWords(text: q.question, cap: wordsCap)
            } else {
                Text(q.question)
                    .font(.subheadline)
            }
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
            .answerField(p)
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
        Task {
            let taken = if let key { await model.answer(request, value, in: key) } else { await model.answer(request, value) }
            // (Felt once harlo has it: an answer that didn't get there stays.)
            if taken { UINotificationFeedbackGenerator().notificationOccurred(.success) }
        }
    }
}
