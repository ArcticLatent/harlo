import HarloKit
import SwiftUI

/// The plan sheet's Wiki: the repository's wiki (OpenWiki's, in openwiki/)
/// to read, how fresh it is against the code, and a run that writes it,
/// started on the computer. Without one, what it is and Create.
struct WikiSection: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let wiki: Wiki
    @Binding var notice: String?
    /// Opens the session writing it.
    var showSession: (String) -> Void

    var body: some View {
        Section {
            if wiki.loading {
                HStack(spacing: 8) {
                    ProgressView()
                    Text("Reading the wiki…").font(.subheadline).foregroundStyle(Color(p.muted))
                }
            } else if !wiki.exists {
                VStack(alignment: .leading, spacing: 4) {
                    Text("No wiki yet").font(.body.weight(.medium))
                    Text("Your agent writes one from the code: how its parts work and why, each fact tied to the lines it came from. Agents search it instead of reading through the code, and you can read it here.")
                        .font(.footnote)
                        .foregroundStyle(Color(p.muted))
                }
                .padding(.vertical, 2)
                if wiki.running {
                    running
                } else if wiki.canWrite {
                    runButton("create", "Create wiki", "book")
                }
            } else {
                NavigationLink {
                    WikiView(wiki: wiki)
                } label: {
                    Label {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Read the wiki").font(.body.weight(.medium))
                            Text(wiki.pages.count == 1 ? "1 page" : "\(wiki.pages.count) pages")
                                .font(.footnote)
                                .foregroundStyle(Color(p.muted))
                        }
                    } icon: {
                        Image(systemName: "book").foregroundStyle(Color(p.primary))
                    }
                }
                .accessibilityIdentifier("wikiOpen")
                VStack(alignment: .leading, spacing: 6) {
                    Text(wiki.freshness())
                        .font(.subheadline)
                        .foregroundStyle(Color(wiki.run != nil && !wiki.running ? p.warning : p.muted))
                        .accessibilityIdentifier("wikiFreshness")
                    if let r = wiki.run, wiki.running, r.total > 0 {
                        ProgressView(value: Double(r.done), total: Double(max(r.total, 1)))
                            .tint(Color(p.primary))
                    }
                }
                .padding(.vertical, 2)
                if wiki.running {
                    running
                } else if wiki.canWrite {
                    runButton(wiki.run != nil ? "continue" : "update", wiki.run != nil ? "Continue the run" : "Update the wiki",
                              wiki.run != nil ? "play.circle" : "arrow.clockwise")
                }
            }
        } header: {
            Text("Wiki")
        } footer: {
            Text(!wiki.exists
                 ? "It's written in a session on your computer you can watch, and spends that agent's tokens. The pages land in openwiki/ for you to review in Changes."
                 : wiki.enabled ? "Agents search it in sessions started from now on." : "Agents don't use it: switched off in harlo's Wiki panel on the computer.")
        }
        // While a run writes it: how far along, now and then.
        .task(id: wiki.running) {
            while wiki.running && !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 3_000_000_000)
                await model.loadWiki()
            }
        }
    }

    @ViewBuilder var running: some View {
        if !wiki.session.isEmpty {
            Button {
                showSession(wiki.session)
            } label: {
                Label("Show the session writing it", systemImage: "arrow.up.forward.square")
            }
            .frame(minHeight: 44)
        }
    }

    func runButton(_ mode: String, _ title: String, _ icon: String) -> some View {
        Button {
            UISelectionFeedbackGenerator().selectionChanged()
            Task {
                if await model.runWiki(mode) {
                    notice = "A session on your computer is writing the wiki."
                }
            }
        } label: {
            Label(title, systemImage: icon)
                .font(.body.weight(wiki.wantsUpdate || !wiki.exists ? .semibold : .regular))
        }
        .frame(minHeight: 44)
        .accessibilityIdentifier("wikiRun")
    }
}

/// A page to open, at a section ("" its top).
struct WikiLink: Hashable {
    var path: String
    var section = ""
}

/// The wiki's pages by folder (the quickstart first), a search of it, and
/// its pages' links as a graph.
struct WikiView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let wiki: Wiki
    @State private var query = ""
    @State private var hits: WikiHits?
    @State private var searching = false
    @State private var workspace = ""
    @State private var graph = false

    /// The pages by folder, in the wiki's order.
    var groups: [(name: String, pages: [WikiPage])] {
        var out: [(name: String, pages: [WikiPage])] = []
        for page in wiki.pages {
            if let i = out.firstIndex(where: { $0.name == page.group }) { out[i].pages.append(page) } else { out.append((page.group, [page])) }
        }
        return out
    }

    var body: some View {
        List {
            Group {
                if !query.isEmpty && (hits != nil || searching) {
                    results
                } else {
                    if !wiki.mermaid.isEmpty {
                        Picker("Show", selection: $graph) {
                            Text("Pages").tag(false)
                            Text("Graph").tag(true)
                        }
                        .pickerStyle(.segmented)
                        .accessibilityIdentifier("wikiPagesGraph")
                    }
                    if graph && !wiki.mermaid.isEmpty {
                        Section {
                            PagesGraph(source: wiki.mermaid)
                        } footer: {
                            Text((wiki.graphPages.count < wiki.pages.count ? "The quickstart and the pages it links to or from. " : "") + "An arrow is a link from one page to another. Tap it to zoom.")
                        }
                    } else {
                        ForEach(groups, id: \.name) { g in
                            Section {
                                ForEach(g.pages) { page in
                                    NavigationLink(value: WikiLink(path: page.path)) { row(page) }
                                        .accessibilityIdentifier("wikiPage")
                                }
                            } header: {
                                if !g.name.isEmpty { Text(g.name) }
                            }
                        }
                    }
                }
            }
            .listRowBackground(Color(p.elevated))
        }
        .screen()
        .navigationTitle("Wiki")
        .navigationBarTitleDisplayMode(.inline)
        .navigationDestination(for: WikiLink.self) { link in WikiPageView(wiki: wiki, link: link) }
        .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Search the wiki")
        .onSubmit(of: .search) { Task { await search() } }
        .onChange(of: query) { _, q in
            if q.isEmpty { hits = nil; workspace = "" }
        }
    }

    func row(_ page: WikiPage) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(page.title.isEmpty ? page.path : page.title).font(.body.weight(.medium))
            if !page.summary.isEmpty {
                Text(page.summary).font(.footnote).foregroundStyle(Color(p.muted)).lineLimit(2)
            }
        }
        .padding(.vertical, 2)
    }

    @ViewBuilder var results: some View {
        if searching {
            HStack(spacing: 8) {
                ProgressView()
                Text("Searching…").font(.subheadline).foregroundStyle(Color(p.muted))
            }
        } else if let hits {
            if !hits.workspaces.isEmpty {
                Section {
                    ForEach(hits.workspaces, id: \.id) { w in
                        Button(w.name) {
                            workspace = w.id
                            Task { await search() }
                        }
                        .frame(minHeight: 44)
                    }
                } header: {
                    Text("Search which workspace?")
                } footer: {
                    Text("This wiki is linked to others.")
                }
            } else if hits.hits.isEmpty {
                Text("Nothing in the wiki matches “\(query)”.").foregroundStyle(Color(p.muted))
            } else {
                Section {
                    ForEach(hits.hits) { hit in
                        NavigationLink(value: WikiLink(path: hit.page.hasPrefix("openwiki/") ? String(hit.page.dropFirst(9)) : hit.page, section: hit.section)) {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(hit.heading.isEmpty ? title(of: hit.page) : hit.heading).font(.body.weight(.medium))
                                if !hit.heading.isEmpty {
                                    Text(title(of: hit.page)).font(.caption).foregroundStyle(Color(p.muted))
                                }
                                if !hit.snippet.isEmpty {
                                    Text(hit.snippet).font(.footnote).foregroundStyle(Color(p.muted)).lineLimit(3)
                                }
                            }
                            .padding(.vertical, 2)
                        }
                        .accessibilityIdentifier("wikiHit")
                    }
                } footer: {
                    if hits.plain { Text("OpenWiki couldn't run, so these are the pages that have its words.") }
                }
            }
        }
    }

    func title(of page: String) -> String {
        let path = page.hasPrefix("openwiki/") ? String(page.dropFirst(9)) : page
        return wiki.pages.first { $0.path == path }?.title ?? path
    }

    func search() async {
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !q.isEmpty else { return }
        searching = true
        hits = await model.searchWiki(q, workspace: workspace) ?? WikiHits(["hits": []])
        searching = false
    }
}

/// A code link's file, at its line.
struct WikiFile: Identifiable {
    var path: String
    var line: Int
    var id: String { "\(path):\(line)" }
}

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

    /// Where in the code the page comes from: each file opens at the
    /// first lines its claims cite.
    func sources(_ list: [WikiPageText.Source]) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Sources")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(Color(p.muted))
                .padding(.bottom, 2)
                .accessibilityAddTraits(.isHeader)
            ForEach(list, id: \.path) { s in
                Button {
                    file = WikiFile(path: wiki.file(s.path), line: s.ranges.first?.lowerBound ?? 0)
                } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(s.path).font(.footnote.monospaced()).foregroundStyle(Color(p.primary)).multilineTextAlignment(.leading)
                        let lines = s.ranges.map { $0.count == 1 ? "\($0.lowerBound)" : "\($0.lowerBound)–\($0.upperBound)" }
                        let about = [lines.isEmpty ? "" : "lines " + lines.joined(separator: ", "), s.claims == 0 ? "" : "\(s.claims) \(s.claims == 1 ? "claim" : "claims")"].filter { !$0.isEmpty }
                        if !about.isEmpty {
                            Text(about.joined(separator: " · ")).font(.caption).foregroundStyle(Color(p.muted)).multilineTextAlignment(.leading)
                        }
                    }
                    .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("wikiSource")
                .accessibilityHint("Opens the file")
            }
        }
        .padding(.top, 8)
    }

    var body: some View {
        ScrollViewReader { reader in
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    if let page, !page.error.isEmpty {
                        ContentUnavailableView(title, systemImage: "book.closed", description: Text(page.error))
                    } else if let page {
                        if !meta.isEmpty {
                            Text(meta).font(.footnote).foregroundStyle(Color(p.muted)).accessibilityIdentifier("wikiPageMeta")
                        }
                        if !page.summary.isEmpty {
                            Text(page.summary).font(.subheadline).foregroundStyle(Color(p.muted))
                        }
                        ForEach(page.blocks.indices, id: \.self) { i in
                            block(page.blocks[i]).id(page.blocks[i].anchor.isEmpty ? "b\(i)" : page.blocks[i].anchor)
                        }
                        if !page.sources.isEmpty { sources(page.sources) }
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
            page = await model.wikiPage(link.path)
            failed = page == nil
            if !link.section.isEmpty {
                // (Once it's laid out.)
                try? await Task.sleep(nanoseconds: 150_000_000)
                jump = link.section
            }
        }
    }

    @ViewBuilder
    func block(_ b: WikiPageText.Block) -> some View {
        switch b.kind {
        case "mermaid" where model.drawsDiagrams:
            DiagramBlock(source: b.code, code: CodeBlock(spans: [Span(b.code, code: true)], language: "mermaid", raw: b.code))
        case "mermaid", "code":
            CodeBlock(spans: codeSpans(b), language: b.lang, raw: b.code)
        default:
            RichTextView(blocks: RichText.parse(b.html))
        }
    }

    func codeSpans(_ b: WikiPageText.Block) -> [Span] {
        if case .code(let spans)? = RichText.parse(b.html).first { return spans }
        return [Span(b.code, code: true)]
    }
}
