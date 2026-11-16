import HarloKit
import SwiftUI
import UIKit

/// A long press on a message: copy it; yours also edit & resend, or ask
/// again with another model; the agent's work undoes its turn.
struct ItemMenu: View {
    @Environment(HarloModel.self) private var model
    /// Undo a turn (Settings → Undo a turn).
    @AppStorage("undoTurns") private var undoOn = true
    let item: TranscriptItem
    let session: OpenSession
    var openSheet: (SessionSheet) -> Void

    var body: some View {
        if case .row(let r) = item {
            if r.kind == "user" {
                Button {
                    UIPasteboard.general.string = r.text
                } label: {
                    Label("Copy", systemImage: "doc.on.doc")
                }

                if session.canBranch {
                    Button {
                        Task { await model.editInComposer(r.key) }
                    } label: {
                        Label("Edit & resend", systemImage: "pencil")
                    }
                    Button {
                        openSheet(.retry(r.key))
                    } label: {
                        Label("Retry with another model", systemImage: "arrow.triangle.2.circlepath")
                    }
                }
            } else if r.kind == "markdown" || r.kind == "code" {
                Button {
                    UIPasteboard.general.string = r.kind == "code" && !r.text.isEmpty ? r.text : RichText.plainText(RichCache.blocks(r.html))
                } label: {
                    Label("Copy", systemImage: "doc.on.doc")
                }
                undoItem(after: r.key)
            } else if r.kind == "tool" || r.kind == "notice" {
                Button {
                    UIPasteboard.general.string = r.kind == "tool" ? "\(r.title) \(r.subtitle)\n\n\(r.text)" : r.text
                } label: {
                    Label("Copy", systemImage: "doc.on.doc")
                }
                undoItem(after: r.key)
            }
        } else if case .run(_, _, let rows, _, _) = item, let first = rows.first {
            undoItem(after: first.key)
        }
    }

    /// The agent's work in a turn (a reply, a run): undo that turn, from
    /// your message that started it.
    @ViewBuilder func undoItem(after key: String) -> some View {
        if undoOn, !session.busy, let start = SessionContent.turnStart(session.rows, key), session.undoable[start] != nil {
            Button {
                openSheet(.undo(start))
            } label: {
                Label(SessionContent.lastUser(session.rows) == start ? "Undo this turn" : "Undo from here", systemImage: "arrow.uturn.backward")
            }
        }
    }
}
