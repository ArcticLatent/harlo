import HarloKit
import SwiftUI

/// A row beside the conversation, in full: a side question with its whole
/// answer, a goal with why it paused.
struct JobDetail: View {
    @Environment(\.palette) private var p
    @Environment(\.dismiss) private var dismiss
    let row: JobRow

    /// What it has to say, with its emphasis, code and links (a side
    /// question's answer is Markdown); as written when it can't be read
    /// as that.
    var words: AttributedString {
        let inline = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        return (try? AttributedString(markdown: row.detail, options: inline)) ?? AttributedString(row.detail)
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    Text(row.text)
                        .scaledFont(15, .medium)
                    Text(words)
                        .scaledFont(15)
                    if !row.note.isEmpty {
                        Text(row.note)
                            .scaledFont(13)
                            .foregroundStyle(Color(p.muted))
                    }
                }
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(16)
            }
            .screen()
            .navigationTitle(row.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done", action: close)
                }
            }
        }
        .presentationDetents([.medium, .large])
    }

    func close() {
        dismiss()
    }
}
