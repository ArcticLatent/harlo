import HarloKit
import SwiftUI

/// A wiki page: what kind it is, when it was checked against the code,
/// its summary, then its text (diagrams drawn, code in wells). A link to
/// another page opens it here; a link to code opens the file at its line.
struct WikiPageView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let wiki: Wiki
    let link: WikiLink
    @State private var page: WikiPageText?
    @State private var failed = false
    @State private var next: WikiLink?
    @State private var file: WikiFile?
    @State private var jump = ""

    var title: String { page?.title ?? wiki.pages.first { $0.path == link.path }?.title ?? link.path }

    var meta: String {
        guard let page else { return "" }
        let kind = page.kind.isEmpty ? "" : page.kind.prefix(1).uppercased() + page.kind.dropFirst()
        let checked = page.verified.isEmpty ? "" : "checked against the code " + Wiki.ago(page.verified)
        return [kind, checked].filter { !$0.isEmpty }.joined(separator: " · ")
    }

    var body: some View {
        ScrollViewReader { reader in
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    if let page, !page.error.isEmpty {
                        ContentUnavailableView(title, systemImage: "book.closed", description: Text(page.error))
                    } else if let page {
                        // (Its summary first, then what kind it is: the title
                        // is in the bar, so not again at the top of its text.)
                        if !page.summary.isEmpty {
                            Text(page.summary).font(.body).foregroundStyle(Color(p.foreground).opacity(0.82))
                                .accessibilityIdentifier("wikiPageLede")
                        }
                        if !meta.isEmpty {
                            Text(meta).font(.footnote).foregroundStyle(Color(p.muted)).accessibilityIdentifier("wikiPageMeta")
                                .padding(.bottom, 4)
                        }
                        ForEach(page.blocks.indices, id: \.self) { i in
                            WikiBlock(block: page.blocks[i], first: i == 0).id(page.blocks[i].anchor.isEmpty ? "b\(i)" : page.blocks[i].anchor)
                        }
                        if !page.sources.isEmpty {
                            WikiSources(list: page.sources) { s in
                                file = WikiFile(path: wiki.file(s.path), line: s.ranges.first?.lowerBound ?? 0)
                            }
                        }
                    } else if failed {
                        ContentUnavailableView(title, systemImage: "book.closed", description: Text("The page couldn't be read."))
                    } else {
                        ProgressView().frame(maxWidth: .infinity).padding(.top, 40)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .onChange(of: jump) { _, anchor in
                guard !anchor.isEmpty else { return }
                withAnimation(.easeOut(duration: 0.2)) { reader.scrollTo(anchor, anchor: .top) }
                jump = ""
            }
        }
        .screen()
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.inline)
        .navigationDestination(item: $next) { l in WikiPageView(wiki: wiki, link: l) }
        .sheet(item: $file) { f in FileView(path: f.path, line: f.line) }
        .environment(\.openURL, OpenURLAction { url in
            let target = url.absoluteString
            if let code = WikiPageText.code(of: target) {
                file = WikiFile(path: wiki.file(code.path), line: code.line)
                return .handled
            }
            if let to = page?.page(of: target) {
                if to.path == link.path { jump = to.section } else { next = WikiLink(path: to.path, section: to.section) }
                return .handled
            }
            return .systemAction
        })
        .task {
            model.wikiReadLast[wiki.root] = link.path
            page = await model.wikiPage(link.path)
            failed = page == nil
            if !link.section.isEmpty {
                // (Once it's laid out.)
                try? await Task.sleep(for: .milliseconds(150))
                jump = link.section
            }
        }
    }
}
