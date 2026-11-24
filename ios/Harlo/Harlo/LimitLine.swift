import HarloKit
import SwiftUI

/// Above the composer, from 90% of a plan limit the session uses: how much,
/// and when it resets; once it's used up, Stop and continue (or Continue)
/// at the reset, as harlo's own line does.
struct LimitLine: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let session: OpenSession

    var body: some View {
        let limit = session.limit
        if limit != nil || session.resumesAt > 0 {
            let out = limit?.out ?? false
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .scaledFont(12)
                    .foregroundStyle(Color(out ? p.error : p.warning))
                Text(words(limit))
                    .scaledFont(13.5)
                    .foregroundStyle(Color(out ? p.error : p.warning))
                    .lineLimit(2)
                Spacer(minLength: 4)
                if session.resumesAt > 0 {
                    button("Don't continue") { await model.cancelContinue() }
                } else if let l = limit, l.out, l.resetAt > now, session.busy || session.info.limitRefused {
                    button((session.busy ? "Stop and continue " : "Continue ") + resetTime(l.resetAt)) { await model.continueAtReset() }
                }
            }
            .padding(.horizontal, 18)
            .padding(.top, 8)
        }
    }

    var now: Int64 { Int64(Date.now.timeIntervalSince1970 * 1000) }

    func words(_ l: SessionLimit?) -> String {
        let gone = l.map { "You've used up your \($0.window.name). " } ?? ""
        if session.resumesAt > 0 {
            return (l?.out == true ? gone : "") + "This session continues \(resetTime(session.resumesAt))"
        }
        guard let l else { return "" }
        let when = l.window.resetsAt > 0 ? " · resets \(resetTime(l.window.resetsAt))" : ""
        return l.out ? "You've used up your \(l.window.name)\(when)"
                     : "You've used \(Int(l.window.usedFraction * 100))% of your \(l.window.name)\(when)"
    }

    func button(_ title: String, _ action: @escaping () async -> Void) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            Task { await action() }
        } label: {
            Text(title)
                .scaledFont(12.5, .medium)
                .foregroundStyle(Color(p.primary))
                .multilineTextAlignment(.trailing)
        }
        .buttonStyle(.plain)
    }
}
