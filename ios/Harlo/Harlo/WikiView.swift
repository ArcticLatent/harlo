import HarloKit
import SwiftUI

/// The wiki's pages by folder (the quickstart first), a search of it, and
/// its pages' links as a graph.
struct WikiView: View {
    @Environment(HarloModel.self) private var model
    @Environment(\.palette) private var p
    let wiki: Wiki
    @Binding var notice: String?
    /// Opens the session writing it.
    var showSession: (String) -> Void
    /// Opens a page (the graph's boxes).
    var open: (WikiLink) -> Void
    @State private var query = ""
    @State private var hits: WikiHits?
    @State private var searching = false
    @State private var workspace = ""
    @State private var graph = false
    @State private var zoom: ZoomedPicture?

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
                if !wiki.inProject {
                    Text("This session isn't in one of harlo's projects, so it has no wiki.")
                        .foregroundStyle(Color(p.muted))
                } else if !wiki.exists || wiki.loading {
                    // (Reading it, or what a wiki is and Create.)
                    WikiSection(wiki: wiki, notice: $notice, showSession: showSession)
                } else if !query.isEmpty && (hits != nil || searching) {
                    results
                } else {
                    WikiSection(wiki: wiki, notice: $notice, showSession: showSession)
                    if !wiki.mermaid.isEmpty {
                        Picker("Show", selection: $graph) {
                            Text("Pages").tag(false)
                            Text("Graph").tag(true)
                        }
                        .pickerStyle(.segmented)
                        .accessibilityIdentifier("wikiPagesGraph")
                        // On the screen, above the list: not in a card.
                        .listRowBackground(Color.clear)
                        .listRowInsets(EdgeInsets(top: 2, leading: 0, bottom: 2, trailing: 0))
                        .listRowSeparator(.hidden)
                    }
                    if graph && !wiki.mermaid.isEmpty {
                        Section {
                            PagesGraph(wiki: wiki, zoom: $zoom, open: open)
                        } footer: {
                            Text((wiki.graphPages.count < wiki.pages.count ? "The quickstart and the pages it links to or from. " : "") + "An arrow is a link from one page to another. Tap a page to read it, or elsewhere to zoom.")
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
        .fullScreenCover(item: $zoom) { z in DiagramZoom(image: z.image, size: z.size, title: "Wiki") }
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
                                    Text(marked(hit.snippet)).font(.footnote).foregroundStyle(Color(p.muted)).lineLimit(3)
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

    /// A hit's text with the query's words in the text colour and medium
    /// weight (the rest stays muted), so it says why it matched.
    func marked(_ text: String) -> AttributedString {
        var out = AttributedString(text)
        let words = Wiki.markWords(query)
        guard !words.isEmpty else { return out }
        // (The whole word a match is in: "amended" for "amend".)
        var start: String.Index?
        for i in Array(text.indices) + [text.endIndex] {
            let inWord = i < text.endIndex && (text[i].isLetter || text[i].isNumber || text[i] == "_")
            if inWord, start == nil { start = i }
            if !inWord, let s = start {
                let word = text[s..<i].lowercased()
                if words.contains(where: { word.contains($0) }),
                   let lo = AttributedString.Index(s, within: out), let hi = AttributedString.Index(i, within: out) {
                    out[lo..<hi].foregroundColor = Color(p.foreground)
                    out[lo..<hi].font = .footnote.weight(.semibold)
                }
                start = nil
            }
        }
        return out
    }

    func search() async {
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !q.isEmpty else { return }
        searching = true
        hits = await model.searchWiki(q, workspace: workspace) ?? WikiHits(["hits": []])
        searching = false
    }
}
