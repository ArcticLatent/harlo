import HarloKit
import SwiftUI

/// A wiki page: what kind it is, who wrote and reviewed it, what it owes
/// the code (with Check this page and Mark reviewed), its summary and text
/// (diagrams drawn, code in wells), its footnotes and the pages linking to
/// it. A link to another page opens it here; a footnote or a link to a
/// file opens the code.
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
    @State private var notice: String?
    @State private var busy = false

    var title: String { page?.title ?? wiki.pages.first { $0.path == link.path }?.title ?? link.path }

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
                        let meta = page.meta()
                        if !meta.isEmpty {
                            Text(meta).font(.footnote).foregroundStyle(Color(p.muted)).accessibilityIdentifier("wikiPageMeta")
                        }
                        owes(page)
                        ForEach(page.blocks.indices, id: \.self) { i in
                            WikiBlock(block: page.blocks[i], first: i == 0).id(page.blocks[i].anchor.isEmpty ? "b\(i)" : page.blocks[i].anchor)
                        }
                        if !page.footnotes.isEmpty {
                            WikiFootnotes(list: page.footnotes) { open($0) }
                        }
                        if !page.backlinks.isEmpty {
                            backlinks(page.backlinks)
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
        .alert(notice ?? "", isPresented: Binding(get: { notice != nil }, set: { if !$0 { notice = nil } })) {
            Button("OK", role: .cancel) {}
        }
        .environment(\.openURL, OpenURLAction { url in
            switch page?.target(of: url.absoluteString) {
            case .page(let path, let section)?:
                if path == link.path { jump = section } else { next = WikiLink(path: path, section: section) }
                return .handled
            case .file(let path)?:
                file = WikiFile(path: wiki.file(path), line: 0)
                return .handled
            case .footnote(let f)?:
                open(f)
                return .handled
            case nil:
                return .systemAction
            }
        })
        .task {
            model.wikiReadLast[wiki.root] = link.path
            await load()
            if !link.section.isEmpty {
                // (Once it's laid out.)
                try? await Task.sleep(for: .milliseconds(150))
                jump = link.section
            }
        }
    }

    func load() async {
        page = await model.wikiPage(link.path)
        failed = page == nil
    }

    /// A footnote's code, at its lines now (a file that's gone: nothing).
    func open(_ f: WikiPageText.Footnote) {
        guard f.state != "gone" else { return }
        file = WikiFile(path: wiki.file(f.path), line: f.start)
    }

    /// What it owes the code, and what to do about it: a session that
    /// checks it against its code, or your word that you have.
    @ViewBuilder func owes(_ page: WikiPageText) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(page.owes, id: \.self) { line in
                Text(line).font(.footnote).foregroundStyle(Color(p.warning))
            }
            HStack(spacing: 16) {
                Button {
                    UISelectionFeedbackGenerator().selectionChanged()
                    Task {
                        busy = true
                        if await model.runWiki("page:" + link.path) {
                            notice = "A session on your computer is checking this page against its code."
                        }
                        busy = false
                    }
                } label: {
                    Text("Check this page").font(.subheadline.weight(page.owes.isEmpty ? .regular : .semibold))
                }
                .frame(minHeight: 44)
                .accessibilityIdentifier("wikiCheckPage")
                .accessibilityHint("Starts a session on your computer that reads it against its code and fixes it")
                Button {
                    UISelectionFeedbackGenerator().selectionChanged()
                    Task {
                        busy = true
                        if await model.reviewWiki(link.path) { await load() }
                        busy = false
                    }
                } label: {
                    Text("Mark reviewed").font(.subheadline)
                }
                .frame(minHeight: 44)
                .accessibilityIdentifier("wikiMarkReviewed")
                .accessibilityHint("You've read it against its code: its changed footnotes count as checked")
            }
            .disabled(busy || wiki.running)
        }
        .padding(.bottom, 4)
    }

    /// The pages that link here.
    func backlinks(_ list: [WikiPage]) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Linked from")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(Color(p.muted))
                .padding(.bottom, 2)
                .accessibilityAddTraits(.isHeader)
            ForEach(list) { other in
                Button(other.title.isEmpty ? other.path : other.title) { next = WikiLink(path: other.path) }
                    .font(.footnote)
                    .frame(minHeight: 44)
                    .accessibilityIdentifier("wikiBacklink")
            }
        }
        .padding(.top, 8)
    }
}
