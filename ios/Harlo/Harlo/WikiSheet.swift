import HarloKit
import SwiftUI

/// The session menu's Wiki: the wiki's pages (how fresh they are and
/// Update on top), or what a wiki is and Create when there's none yet.
struct WikiSheet: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    @Environment(\.dismiss) private var dismiss
    let session: OpenSession
    @State private var notice: String?
    /// What's open: one path for the whole sheet, of pages (WikiLink) and
    /// code cards (WikiHits.Code). (A path typed for pages alone ignored a
    /// code hit's tap.)
    @State private var path = NavigationPath()
    /// Read once, as the sheet opens (not each time a page closes).
    @State private var loaded = false

    var body: some View {
        // The wiki's screen is always the root, whatever state the wiki is
        // in: inside a branch at the root, its search field went when you
        // came back from a page.
        NavigationStack(path: $path) {
            WikiView(wiki: session.wiki ?? Wiki(exists: false, loading: true), notice: $notice, showSession: show) { path.append($0) }
                // A page row, a search hit or a graph box pushes a WikiLink.
                .navigationDestination(for: WikiLink.self) { link in
                    if let wiki = session.wiki { WikiPageView(wiki: wiki, link: link) }
                }
                // A search's code hit pushes its card.
                .navigationDestination(for: WikiHits.Code.self) { code in
                    WikiCodeCard(code: code)
                }
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } }
                }
                .alert(notice ?? "", isPresented: Binding(get: { notice != nil }, set: { if !$0 { notice = nil } })) {
                    Button("OK", role: .cancel) {}
                }
                .task {
                    guard !loaded else { return }
                    loaded = true
                    await model.loadWiki()
                }
        }
        .modifier(Themed())
    }

    /// Opens the session writing the wiki.
    func show(_ key: String) {
        dismiss()
        _ = model.openHelper(key)
    }
}
