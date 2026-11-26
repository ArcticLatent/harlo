import HarloKit
import SwiftUI

/// One thing a session has going beside its conversation: what it is,
/// where it stands, and the one thing to do to it. A row with more to say
/// than fits (a side question's answer, why a goal paused) opens in full.
struct JobRowView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let row: JobRow
    /// Its action is on its way to harlo.
    @State private var acting = false
    /// Shown in full.
    @State private var reading = false

    init(_ row: JobRow) {
        self.row = row
    }

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: row.symbol.isEmpty ? "circle.dashed" : row.symbol)
                .scaledFont(12)
                .foregroundStyle(Color(row.working ? p.primary : p.muted))
                .frame(width: 20)
                .accessibilityHidden(true)
            if row.detail.isEmpty {
                JobRowWords(row: row)
            } else {
                Button(action: read) {
                    JobRowWords(row: row)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityHint("Shows it in full")
            }
            if !row.action.isEmpty {
                Button(action: act) {
                    Text(row.action)
                        .scaledFont(13, .medium)
                        .foregroundStyle(Color(row.action == "Stop" ? p.error : p.primary))
                        .frame(minWidth: 44, minHeight: 32)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(acting)
                .opacity(acting ? 0.55 : 1)
                .accessibilityLabel("\(row.action): \(row.text)")
            }
        }
        .sheet(isPresented: $reading) {
            JobDetail(row: row)
        }
    }

    func read() {
        reading = true
    }

    func act() {
        acting = true
        Task {
            await model.act(on: row)
            acting = false
        }
    }
}
