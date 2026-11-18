pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// The wiki of the active session's repository: Open Knowledge Format pages
// in its .okf/ folder, by folder (a filter by type), each read here (links
// between pages open in the panel, with Back; a footnote opens the code it
// cites, marked when that code changed since), a search, the pages' links
// drawn as a graph, and what the wiki owes the code now. Create and Update
// start a session that writes it with the project's agent; harlo checks
// first, with no model, and an update with nothing owed starts none. The
// result shows in Changes like any edit.
Rectangle {
    id: root
    /// The active session's key, or `project:<id>` on the new-session page.
    property string sessionKey: ""
    signal closeRequested()
    /// A file of the repository to open, at a line (0: its top).
    signal openFileRequested(string path, int line)
    color: Theme.canvas
    clip: true

    property var st: ({ root: "", project: -1, enabled: false, exists: false, pages: [] })
    readonly property var pages: st.pages || []
    /// "pages" or "graph" (when no page is open).
    property string mode: "pages"
    /// The page being read ("architecture/jobs.md"), "" for the list.
    property string openPage: ""
    /// The page read last (kept once it's closed): the graph rings it.
    property string lastRead: ""
    onOpenPageChanged: if (openPage !== "") lastRead = openPage
    property var pageView: null
    property string loadingPage: ""
    /// The section to show once it's loaded.
    property string wantSection: ""
    /// Pages read before this one, for Back: [{ page, y }].
    property var history: []
    /// A search: its words, what came back.
    property string query: ""
    property var results: null
    property bool searching: false
    property string workspace: ""
    property string runError: ""

    /// The repository whose wiki is shown: another one starts afresh (a
    /// file tab or another session of the same one keeps the page open).
    property string shownRoot: ""
    /// Who writes it (Settings → Wiki, else the project's agent and model).
    property var writer: ({})
    /// What the wiki owes the code now (`work`: counts and the list).
    readonly property var work: st.work || ({ count: 0, current: true })
    /// Who writes a kind of run ("create", "update"): "Written by Harlo
    /// with GPT-5 mini (Settings → Wiki)", or "".
    function writerOf(op) {
        const w = (writer || {})[op] || {}
        if (!w.agent) return ""
        const agent = w.agent === "claude" ? "Claude Code" : w.agent === "harlo" ? "Harlo" : w.agent
        return "Written by " + agent + (w.model ? " with " + Theme.modelLabel(w.model) : "") + " (Settings → Wiki)"
    }
    readonly property string writerText: writerOf("create")
    readonly property string updaterText: writerOf("update")
    function load() {
        st = JSON.parse(Workspace.wikiJson(sessionKey))
        writer = JSON.parse(Workspace.wikiWriterJson(sessionKey))
        if (st.root !== "" && st.root !== shownRoot) {
            shownRoot = st.root
            closePage(); results = null; query = ""; searchField.text = ""; mode = "pages"; typeFilter = ""
        }
        // (What it owes changed: the open page reads again, where it was.)
        const seen = JSON.stringify(st.work || {}) + JSON.stringify(pages.find(p => p.path === openPage) || {})
        if (seen !== shownWork) {
            const was = shownWork
            shownWork = seen
            if (was !== "" && openPage !== "" && loadingPage === "") {
                keepY = reader.contentY
                loadingPage = openPage
                wantSection = ""
                Workspace.requestWikiPage(sessionKey, openPage)
            }
        }
    }
    /// What the wiki owed when the open page was last read (with that page's row).
    property string shownWork: ""
    /// Where the reader was, for a page read again (-1 a page opened afresh).
    property real keepY: -1
    onSessionKeyChanged: if (visible) load()
    onVisibleChanged: if (visible) load()
    Component.onCompleted: if (visible) load()
    // What it owes, kept current while it shows (read in the background).
    Timer {
        interval: 4000
        repeat: true
        running: root.visible && root.st.project >= 0
        onTriggered: root.load()
    }
    Connections {
        target: Workspace
        function onWikiReady() { if (root.visible) root.load() }
        // (A session starting or ending a turn: a wiki run, say.)
        function onBusyCountChanged() { if (root.visible) root.load() }
        function onWikiPageReady(page, json) {
            if (page !== root.loadingPage) return
            root.loadingPage = ""
            root.pageView = JSON.parse(json)
            root.openPage = root.pageView.path
            const y = root.keepY
            root.keepY = -1
            reader.contentY = 0
            // (Read again: back where it was, once its blocks are laid out.)
            if (y > 0) Qt.callLater(() => { reader.contentY = Math.min(y, Math.max(0, reader.contentHeight - reader.height)) })
            if (root.wantSection !== "") Qt.callLater(root.showSection, root.wantSection)
        }
        function onWikiSearchReady(query, json) {
            if (query !== root.query) return
            root.searching = false
            root.results = JSON.parse(json)
        }
    }

    // ---- reading ----

    /// Opens a page (as a search ref or a link names it: ".okf/x.md",
    /// "x.md"), at a section when given.
    function openWiki(page, section, keep) {
        const p = String(page).replace(/^\.okf\//, "").replace(/^\.?\//, "")
        if (keep && root.openPage !== "") root.history = root.history.concat([{ page: root.openPage, y: reader.contentY }])
        if (p === root.openPage && root.pageView) { root.showSection(section || ""); return }
        root.wantSection = section || ""
        root.loadingPage = p
        Workspace.requestWikiPage(root.sessionKey, p)
    }
    function closePage() { root.openPage = ""; root.pageView = null; root.history = []; root.loadingPage = "" }
    function back() {
        if (root.history.length === 0) { closePage(); return }
        const last = root.history[root.history.length - 1]
        root.history = root.history.slice(0, -1)
        root.wantSection = ""
        root.loadingPage = last.page
        Workspace.requestWikiPage(root.sessionKey, last.page)
    }
    /// Scrolls the reader to a section's heading.
    function showSection(anchor) {
        if (anchor === "") return
        for (let i = 0; i < blocks.count; i++) {
            const b = blocks.itemAt(i)
            if (b && b.anchor === anchor) {
                reader.contentY = Math.max(0, Math.min(b.y, reader.contentHeight - reader.height))
                return
            }
        }
    }
    /// A link in a page: another page (in the panel), code (in the editor),
    /// a section of this one, or the web.
    function follow(href) {
        // A footnote: the code it cites, at its lines now.
        if (href.startsWith("okf-fn:")) {
            const f = ((root.pageView || {}).footnotes || []).find(x => x.label === decodeURIComponent(href.substring(7)))
            if (f && f.state !== "gone") root.openFileRequested(root.st.root + "/" + f.path, f.start || 0)
            return
        }
        if (href.startsWith("repo://")) {
            const m = /^repo:\/\/([^#]+)(?:#L(\d+))?/.exec(href)
            if (m) root.openFileRequested(root.st.root + "/" + decodeURIComponent(m[1]), m[2] ? Number(m[2]) : 0)
            return
        }
        if (href.startsWith("#")) { root.showSection(href.substring(1)); return }
        if (/^[a-z][a-z0-9+.-]*:/i.test(href)) { Qt.openUrlExternally(href); return }
        const [path, section] = href.split("#")
        // From the page's folder in .okf/ (`../src/run.rs`), or the wiki's
        // top (`/x.md`): a page when it stays in .okf/, else the
        // repository's file (`../../README.md`).
        const at = [".okf"].concat(path.startsWith("/") ? [] : root.openPage.split("/").slice(0, -1))
        for (const c of path.split("/")) {
            if (c === "..") at.pop()
            else if (c !== "." && c !== "") at.push(c)
        }
        if (at[0] === ".okf" && path.endsWith(".md")) root.openWiki(at.slice(1).join("/"), section || "", true)
        else root.openFileRequested(root.st.root + "/" + at.join("/"), 0)
    }

    // ---- searching ----

    /// The search field focused, when the wiki has pages to search (once
    /// the panel and its pages show, when they don't yet).
    property bool searchWanted: false
    function focusSearch() {
        searchWanted = !searchField.visible
        if (searchField.visible) searchField.forceActiveFocus()
    }

    function search(text) {
        root.query = text.trim()
        if (root.query === "") { root.results = null; root.searching = false; return }
        root.searching = true
        Workspace.requestWikiSearch(root.sessionKey, root.query, root.workspace)
    }

    /// A hit's snippet as styled text, the query's words in the text
    /// colour and medium weight (the rest stays muted), as session search
    /// marks its matches.
    /// The query's words worth marking in a hit: not filler words ("how",
    /// "the", "to"), unless the query is nothing else.
    readonly property var fillerWords: ["the", "a", "an", "and", "or", "of", "to", "in", "is", "it", "for", "on", "with", "as", "at", "by",
                                        "how", "what", "where", "when", "why", "does", "do", "are", "be", "from", "this", "that"]
    function markWords(query) {
        const words = query.toLowerCase().split(/[^0-9A-Za-z\u00C0-\uFFFF]+/).filter(w => w.length > 1)
        const kept = words.filter(w => fillerWords.indexOf(w) < 0)
        return kept.length > 0 ? kept : words
    }

    /// Highlighted code (a `<pre>`, spans never crossing a line) as one
    /// block per line that wraps rather than being cut, each wrapped part
    /// indented two characters past its line's own indent so a line still
    /// reads as one.
    function wrappedCode(html, charWidth) {
        const m = /^<pre[^>]*>([\s\S]*)<\/pre>$/.exec(html)
        if (!m) return html
        // (A span the newline sits in closes on the line it belongs to.)
        const lines = m[1].replace(/\n((?:<\/span>)+)/g, "$1\n").replace(/\n$/, "").split("\n")
        return lines.map(line => {
            const lead = /^[ \t]*/.exec(line.replace(/<[^>]*>/g, ""))[0].replace(/\t/g, "    ").length
            const hang = Math.round((lead + 2) * charWidth)
            return "<p style=\"margin:0; margin-left:" + hang + "px; text-indent:-" + hang + "px; white-space:pre-wrap;\">" + (line === "" ? " " : line) + "</p>"
        }).join("")
    }

    function marked(text, query) {
        const esc = s => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
        const words = markWords(query)
        if (words.length === 0) return esc(text)
        // (The whole word a match is in: "amended" for "amend".)
        const re = new RegExp("([0-9A-Za-z_\u00C0-\uFFFF]*(?:" + words.map(w => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|") + ")[0-9A-Za-z_\u00C0-\uFFFF]*)", "gi")
        return text.split(re).map((part, i) => i % 2 === 1
            ? "<b><font color='" + Theme.rgb(Theme.text) + "'>" + esc(part) + "</font></b>"
            : esc(part)).join("")
    }

    // ---- what's shown ----

    /// A folder's name as a heading ("architecture" → "Architecture").
    function groupOf(path) {
        const i = path.lastIndexOf("/")
        if (i < 0) return ""
        const f = path.substring(0, i).split("/").pop().replace(/[-_]/g, " ")
        return f.charAt(0).toUpperCase() + f.slice(1)
    }
    /// The pages' types, for the filter (only when there's more than one).
    readonly property var types: {
        const seen = []
        for (const p of pages) if (p.type && seen.indexOf(p.type) < 0) seen.push(p.type)
        return seen.sort()
    }
    /// The type shown ("" all).
    property string typeFilter: ""
    readonly property var pageRows: pages.filter(p => typeFilter === "" || p.type === typeFilter).map(p => Object.assign({ group: groupOf(p.path) }, p))
    function titleOf(path) {
        const p = pages.find(x => x.path === path.replace(/^\.okf\//, ""))
        return p ? p.title : path.replace(/^\.okf\//, "")
    }
    /// Who an OKF actor is, in words: "you" (this computer's person),
    /// "Claude Code with Opus 5.5", "Harlo with GPT-5 mini".
    function actorText(by) {
        const a = String(by || "")
        if (a.startsWith("human:")) return a === ((root.pageView || {}).you || "") ? "you" : a.substring(6)
        const agent = a.split("/")[0], model = a.slice(agent.length + 1)
        const name = agent === "claude" ? "Claude Code" : agent === "harlo" ? "Harlo" : agent
        // (The model as its agent lists it, where it does: "Opus 5.5".)
        const p = model && Workspace.providersRevision >= 0 ? JSON.parse(Workspace.providersJson()).find(x => x.id === agent) : null
        const i = p ? (p.modelIds || []).findIndex(id => id === model || id.endsWith("/" + model)) : -1
        return name + (model ? " with " + Theme.modelLabel(i >= 0 && p.models[i] ? p.models[i] : model) : "")
    }
    /// "3 days ago", from an ISO time.
    function ago(iso) {
        const t = Date.parse(iso)
        if (isNaN(t)) return ""
        const s = Math.max(0, (Date.now() - t) / 1000)
        if (s < 90) return "just now"
        if (s < 3600) return Math.round(s / 60) + " minutes ago"
        if (s < 86400) return Math.round(s / 3600) === 1 ? "an hour ago" : Math.round(s / 3600) + " hours ago"
        if (s < 86400 * 45) return Math.round(s / 86400) === 1 ? "yesterday" : Math.round(s / 86400) + " days ago"
        return new Date(t).toLocaleDateString(Qt.locale(), Locale.ShortFormat)
    }
    function start(mode) {
        root.runError = ""
        // (Creating it turns it on for the project's agents.)
        if (mode === "create" && root.st.project >= 0) Workspace.setSetting("wiki_" + root.st.project, "1")
        root.runError = Workspace.startWikiRun(root.sessionKey, mode)
        root.load()
    }
    /// The pages as a Mermaid flowchart: all of them, or for a big wiki
    /// the open page (else the overview) and its neighbours.
    readonly property var graphPages: {
        if (pages.length < 2) return []
        if (pages.length <= 40) return pages
        const at = root.openPage !== "" ? root.openPage : root.lastRead !== "" && pages.some(p => p.path === root.lastRead) ? root.lastRead : (pages[0] ? pages[0].path : "")
        const near = new Set([at])
        for (const p of pages) {
            if (p.path === at) p.links.forEach(l => near.add(l))
            else if (p.links.indexOf(at) >= 0) near.add(p.path)
        }
        return pages.filter(p => near.has(p.path))
    }
    readonly property string graphSource: {
        const shown = graphPages
        if (shown.length < 2) return ""
        const at = new Map(shown.map((p, i) => [p.path, i]))
        let out = "flowchart LR\n"
        shown.forEach((p, i) => { out += "  p" + i + "[\"" + p.title.replace(/"/g, "#quot;").replace(/[\[\]{}<>]/g, " ") + "\"]\n" })
        shown.forEach((p, i) => p.links.forEach(l => { if (at.has(l)) out += "  p" + i + " --> p" + at.get(l) + "\n" }))
        return out
    }
    // Drawn both across and down: the panel shows whichever it fits
    // larger (a chain of pages reads down a narrow panel, many pages off
    // one read across).
    readonly property string graphSourceDown: graphSource.replace(/^flowchart LR/, "flowchart TB")
    // (Drawn only while the panel shows: hidden, its picture is let go.)
    function drawGraph(source) {
        return mode === "graph" && root.visible && source !== "" && Diagrams.revision >= 0
            ? JSON.parse(Diagrams.diagram(source, Theme.isLight, JSON.stringify({ background: String(Theme.canvas), surface: String(Theme.raised), text: String(Theme.foreground),
                                                                                  muted: String(Theme.muted), accent: String(Theme.primary), font: Theme.uiFont })))
            : null
    }
    readonly property var graphAcross: drawGraph(graphSource)
    readonly property var graphDown: drawGraph(graphSourceDown)
    readonly property real graphRoom: Math.max(1, graphBox.width - 32)
    readonly property bool graphDownward: !!graphAcross && !!graphDown && graphAcross.state === "ready" && graphDown.state === "ready"
        && Math.min(1, graphRoom / Math.max(1, graphDown.width)) > 1.15 * Math.min(1, graphRoom / Math.max(1, graphAcross.width))
    readonly property var graphPic: {
        const a = graphAcross, b = graphDown
        if (!a) return null
        if (!b || b.state === "failed") return a
        if (a.state === "failed") return b
        if (a.state !== "ready" || b.state !== "ready") return { state: "drawing" }
        return graphDownward ? b : a
    }
    readonly property string graphShown: graphDownward ? graphSourceDown : graphSource
    /// The graph's zoom over its fit (Ctrl+wheel, − and +).
    property real graphZoom: 1
    onGraphSourceChanged: graphZoom = 1
    /// The whole graph, full size (pan and zoom).
    function openGraphFull() {
        if (!graphPic || graphPic.state !== "ready") return
        viewer.show("Wiki", [{ label: "Graph", url: graphPic.url, width: graphPic.width, height: graphPic.height }], 0, "Graph",
                    () => Diagrams.openInBrowser(graphShown, Theme.isLight), 2)
    }
    /// Wider than the panel: scrolled so the page read last (else the
    /// quickstart, else the first page) is in view rather than whatever the
    /// left edge holds.
    /// (Once for each graph at each panel width: zooming doesn't jump back.)
    property string graphCentredFor: ""
    function centreGraph() {
        const key = graphShown + "|" + Math.round(graphBox.width) + "|" + lastRead
        if (!graphImage.ready || graphCentredFor === key || graphImage.boxes.length === 0) return
        if (graphBox.contentWidth <= graphBox.width + 1) return
        graphCentredFor = key
        let i = graphPages.findIndex(p => p.path === lastRead)
        if (i < 0) i = graphPages.findIndex(p => p.path === "overview.md")
        if (i < 0) i = 0
        const b = graphImage.boxes.find(x => String(x.id) === "p" + i)
        if (!b) return
        // (From the sizes it's laid out to, which may not have reached the
        // picture yet.)
        const w = graphImage.Layout.preferredWidth
        const left = 18 + (Math.max(graphBox.width - 32, w) - w) / 2
        const centre = left + (b.x + b.w / 2) * w
        graphBox.contentX = Math.max(0, Math.min(graphBox.contentWidth - graphBox.width, centre - graphBox.width / 2))
    }
    readonly property bool reading: openPage !== "" || loadingPage !== ""
    readonly property bool showingResults: results !== null || searching

    PictureViewer { id: viewer }
    FontMetrics { id: codeChar; font.family: Theme.mono; font.pixelSize: Theme.mpx(12) }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        PanelHeader {
            title: "Wiki"
            subtitle: root.st.root.replace(/\/+$/, "").split("/").pop() + (root.st.host ? " on " + root.st.host : "")
            onCloseRequested: root.closeRequested()
            PanelButton { objectName: "wikiRefresh"; iconName: "refresh"; tip: "Refresh"; onClicked: { Workspace.refreshWiki(); root.shownWork = "?"; root.load() } }
        }

        Label {
            Layout.margins: 18
            Layout.fillWidth: true
            visible: root.st.project < 0
            wrapMode: Text.Wrap
            text: root.sessionKey === "" ? "No session selected." : "The wiki is for sessions in a project."
            color: Theme.textMuted
            font.pixelSize: Theme.px(12.5)
        }
        Label {
            Layout.margins: 18
            Layout.fillWidth: true
            objectName: "wikiReading"
            visible: root.st.project >= 0 && (!!root.st.loading || !!root.st.error)
            wrapMode: Text.Wrap
            text: root.st.error ? "The wiki couldn't be read: " + root.st.error : "Reading the wiki…"
            color: Theme.textMuted
            font.pixelSize: Theme.px(12.5)
        }

        // ---- none yet ----
        ColumnLayout {
            objectName: "wikiEmpty"
            Layout.fillWidth: true
            Layout.margins: 18
            visible: root.st.project >= 0 && (!root.st.exists || root.pages.length === 0) && !root.st.loading && !root.st.error
            spacing: 10
            Label {
                Layout.fillWidth: true
                text: "No wiki yet"
                color: Theme.text
                font.pixelSize: Theme.px(14)
                font.weight: Font.DemiBold
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: "A wiki of this project, written by your agent from the code: how its parts fit and why, each claim footnoted with the code it came from. harlo tells you when that code changes or new code comes that no page mentions, and agents read the pages instead of searching through the code."
                color: Theme.textMuted
                font.pixelSize: Theme.px(12.5)
                lineHeight: 1.15
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: "It's written in a session you can watch and spends that agent's tokens: more for a bigger repository. The pages land in .okf/ (Open Knowledge Format) and show in Changes for you to review and commit."
                color: Theme.textFaint
                font.pixelSize: Theme.px(12)
                lineHeight: 1.15
            }
            Label {
                objectName: "wikiWriter"
                Layout.fillWidth: true
                visible: root.writerText !== ""
                wrapMode: Text.Wrap
                text: root.writerText + "."
                color: Theme.textFaint
                font.pixelSize: Theme.px(12)
            }
            FlatButton {
                objectName: "wikiCreate"
                Layout.topMargin: 4
                variant: "primary"
                enabled: !root.st.running
                text: root.st.running ? "Writing the wiki…" : "Create wiki"
                font.pixelSize: Theme.px(12.5)
                onClicked: root.start("create")
            }
        }

        // ---- search ----
        AppTextField {
            id: searchField
            objectName: "wikiSearch"
            Layout.fillWidth: true
            Layout.leftMargin: 14
            Layout.rightMargin: 14
            Layout.topMargin: 12
            visible: root.st.exists && root.pages.length > 0
            onVisibleChanged: if (visible && root.searchWanted) { root.searchWanted = false; forceActiveFocus() }
            placeholderText: "Search the wiki"
            leftPadding: 30
            rightPadding: text !== "" ? 30 : 10
            onAccepted: root.search(text)
            Keys.onEscapePressed: event => {
                if (text !== "") { text = ""; root.search(""); event.accepted = true }
                else event.accepted = false
            }
            Icon { name: "search"; color: Theme.textFaint; x: 10; anchors.verticalCenter: parent.verticalCenter; implicitWidth: 14; implicitHeight: 14 }
            PanelButton {
                visible: searchField.text !== ""
                anchors.right: parent.right
                anchors.rightMargin: 3
                anchors.verticalCenter: parent.verticalCenter
                iconName: "close"
                tip: "Clear the search"
                onClicked: { searchField.text = ""; root.search("") }
            }
        }

        // Pages, or their graph (when no page or search is open).
        SegmentedControl {
            Layout.leftMargin: 14
            Layout.topMargin: 10
            visible: root.st.exists && root.pages.length > 1 && !root.reading && !root.showingResults
            labels: ["Pages", "Graph"]
            // (At the size of the app's tabs, not a mockup's small switch.)
            fontSize: Theme.px(13)
            segmentPadding: 4
            current: root.mode === "graph" ? 1 : 0
            namePrefix: "wikiView"
            onPicked: i => root.mode = i === 1 ? "graph" : "pages"
        }

        // ---- search results ----
        ListView {
            id: hitList
            objectName: "wikiHits"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.topMargin: 8
            visible: root.showingResults && !root.reading && root.st.exists
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            model: root.results ? root.results.hits : []
            header: ColumnLayout {
                width: hitList.width
                spacing: 6
                Label {
                    Layout.fillWidth: true
                    Layout.leftMargin: 18
                    Layout.rightMargin: 14
                    Layout.bottomMargin: 4
                    visible: text !== ""
                    wrapMode: Text.Wrap
                    text: root.searching ? "Searching…"
                        : !root.results ? ""
                        : root.results.hits.length === 0 ? "Nothing in the wiki matches “" + root.query + "”."
                        : ""
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(12)
                }
                Flow {
                    Layout.fillWidth: true
                    Layout.leftMargin: 18
                    Layout.rightMargin: 14
                    visible: !!root.results && root.results.workspaces.length > 0
                    spacing: 6
                    Repeater {
                        model: root.results ? root.results.workspaces : []
                        FlatButton {
                            required property var modelData
                            text: modelData.name
                            font.pixelSize: Theme.px(12)
                            onClicked: { root.workspace = modelData.id; root.search(root.query) }
                        }
                    }
                }
            }
            delegate: ItemDelegate {
                id: hit
                objectName: "wikiHit"
                required property var modelData
                width: ListView.view.width
                leftInset: 8
                rightInset: 8
                leftPadding: 18
                rightPadding: 14
                topPadding: 7
                bottomPadding: 7
                hoverEnabled: true
                background: Rectangle { radius: 8; color: hit.hovered || hit.activeFocus ? Theme.rowHover : "transparent" }
                contentItem: ColumnLayout {
                    spacing: 2
                    PlainLabel {
                        Layout.fillWidth: true
                        text: hit.modelData.heading || root.titleOf(hit.modelData.page)
                        color: Theme.text
                        font.pixelSize: Theme.px(13)
                        font.weight: Font.Medium
                        elide: Text.ElideRight
                    }
                    PlainLabel {
                        Layout.fillWidth: true
                        text: root.titleOf(hit.modelData.page) + (hit.modelData.wiki ? " · " + hit.modelData.wiki : "")
                        visible: !!hit.modelData.heading
                        color: Theme.textFaint
                        font.pixelSize: Theme.px(11.5)
                        elide: Text.ElideRight
                    }
                    Label {
                        Layout.fillWidth: true
                        visible: hit.modelData.snippet !== ""
                        objectName: "wikiHitSnippet"
                        text: root.marked(hit.modelData.snippet, root.query)
                        textFormat: Text.StyledText
                        color: Theme.textMuted
                        font.pixelSize: Theme.px(12)
                        wrapMode: Text.Wrap
                        maximumLineCount: 3
                        elide: Text.ElideRight
                    }
                }
                onClicked: root.openWiki(modelData.page, modelData.section, false)
            }
        }

        // ---- a page ----
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.reading && root.st.exists
            spacing: 0
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: 10
                Layout.rightMargin: 14
                Layout.topMargin: 8
                spacing: 6
                FlatButton {
                    objectName: "wikiBack"
                    outlined: false
                    font.pixelSize: Theme.px(12)
                    text: root.history.length > 0 ? "← " + root.titleOf(root.history[root.history.length - 1].page) : root.showingResults ? "← Results" : "← Pages"
                    Layout.maximumWidth: Math.max(80, root.width - 40)
                    onClicked: root.back()
                }
                Item { Layout.fillWidth: true }
            }
            // An edge under Back once the page has scrolled beneath it.
            Rectangle {
                objectName: "wikiBackEdge"
                Layout.fillWidth: true
                Layout.topMargin: 4
                Layout.preferredHeight: 1
                color: reader.contentY > 1 ? Theme.hairline : "transparent"
            }
            Flickable {
                id: reader
                objectName: "wikiReader"
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                contentHeight: readerCol.implicitHeight + 24
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { }
                ColumnLayout {
                    id: readerCol
                    x: 18
                    width: reader.width - 32
                    spacing: 0
                    Label {
                        Layout.fillWidth: true
                        Layout.topMargin: 6
                        visible: root.loadingPage !== "" && !root.pageView
                        text: "Opening…"
                        color: Theme.textMuted
                        font.pixelSize: Theme.px(12.5)
                    }
                    Label {
                        Layout.fillWidth: true
                        visible: !!root.pageView && !!root.pageView.error
                        text: root.pageView ? root.pageView.error || "" : ""
                        color: Theme.textMuted
                        wrapMode: Text.Wrap
                        font.pixelSize: Theme.px(12.5)
                    }
                    // The page's title, its summary as the lede, then what
                    // it is: its kind and when it was checked against the code.
                    PlainLabel {
                        objectName: "wikiPageTitle"
                        Layout.fillWidth: true
                        Layout.topMargin: 6
                        visible: text !== ""
                        text: root.pageView && !root.pageView.error ? root.pageView.title || "" : ""
                        color: Theme.text
                        font.pixelSize: Theme.px(21)
                        font.weight: Font.DemiBold
                        wrapMode: Text.Wrap
                    }
                    PlainLabel {
                        objectName: "wikiPageLede"
                        Layout.fillWidth: true
                        Layout.topMargin: 6
                        visible: text !== ""
                        text: root.pageView && !root.pageView.error ? root.pageView.description || "" : ""
                        color: Theme.textMuted
                        font.pixelSize: Theme.px(14)
                        lineHeight: 1.15
                        wrapMode: Text.Wrap
                    }
                    // What it is, who wrote it and who read it against the code.
                    Label {
                        objectName: "wikiPageMeta"
                        Layout.fillWidth: true
                        Layout.topMargin: 8
                        Layout.bottomMargin: 10
                        visible: text !== ""
                        readonly property var pv: root.pageView && !root.pageView.error ? root.pageView : null
                        readonly property var lastCheck: pv && pv.verified.length > 0 ? pv.verified[pv.verified.length - 1] : null
                        text: !pv ? "" : [pv.type,
                                          pv.status === "draft" ? "draft" : pv.status === "deprecated" ? "deprecated" : "",
                                          pv.generated ? "written by " + root.actorText(pv.generated.by) + " " + root.ago(pv.generated.at) : "",
                                          lastCheck ? "reviewed by " + root.actorText(lastCheck.by) + " " + root.ago(lastCheck.at) : "",
                                          !pv.edited ? "" : pv.generated || lastCheck ? "edited since" : "not checked yet"].filter(x => x !== "").join(" · ")
                        color: Theme.textFaint
                        font.pixelSize: Theme.px(11.5)
                        wrapMode: Text.Wrap
                    }
                    // What it owes the code, and what to do about it.
                    ColumnLayout {
                        objectName: "wikiPageOwes"
                        Layout.fillWidth: true
                        Layout.bottomMargin: 12
                        visible: !!root.pageView && !root.pageView.error
                        spacing: 6
                        readonly property var items: root.pageView && root.pageView.items ? root.pageView.items : []
                        Repeater {
                            model: parent.items
                            Label {
                                required property string modelData
                                Layout.fillWidth: true
                                text: modelData
                                color: Theme.warning
                                font.pixelSize: Theme.px(12)
                                wrapMode: Text.Wrap
                            }
                        }
                        Flow {
                            Layout.fillWidth: true
                            spacing: 8
                            FlatButton {
                                objectName: "wikiCheckPage"
                                text: "Check this page"
                                variant: (root.pageView && root.pageView.items || []).length > 0 ? "primary" : "outline"
                                enabled: !root.st.running
                                font.pixelSize: Theme.px(12)
                                ToolTip.visible: hovered
                                ToolTip.text: "Start a session that reads this page against its code and fixes it" + (root.updaterText ? ". " + root.updaterText + "." : "")
                                ToolTip.delay: 600
                                onClicked: root.start("page:" + root.openPage)
                            }
                            FlatButton {
                                objectName: "wikiMarkReviewed"
                                text: "Mark reviewed"
                                outlined: false
                                font.pixelSize: Theme.px(12)
                                ToolTip.visible: hovered
                                ToolTip.text: "You've read it against its code: its changed footnotes count as checked"
                                ToolTip.delay: 600
                                onClicked: Workspace.markWikiReviewed(root.sessionKey, root.openPage)
                            }
                        }
                    }
                    Repeater {
                        id: blocks
                        model: root.pageView && !root.pageView.error ? root.pageView.blocks : []
                        Item {
                            id: block
                            required property var modelData
                            required property int index
                            readonly property string anchor: modelData.anchor || ""
                            readonly property string kind: modelData.kind
                            // (A heading opens its block: the room above it,
                            // as Prose gives it, is the block's, since a
                            // document's first line gets none.)
                            readonly property int level: kind === "prose" && /^<h[1-6]/.test(modelData.html || "") ? Number(modelData.html.charAt(2)) : 0
                            // (The page's own title is drawn above it.)
                            readonly property string html: kind !== "prose" ? "" : index === 0 ? (modelData.html || "").replace(/^<h1[^>]*>[\s\S]*?<\/h1>\s*/, "") : modelData.html || ""
                            Layout.fillWidth: true
                            visible: kind !== "prose" || html.trim() !== ""
                            Layout.topMargin: kind !== "prose" ? 8 : level === 0 || index === 0 && level === 1 ? 0 : [0, 24, 22, 18, 16, 14, 14][level]
                            Layout.bottomMargin: kind === "prose" ? 0 : 8
                            implicitHeight: kind === "prose" ? prose.implicitHeight : kind === "code" ? codeWell.height : diagram.height
                            TextEdit {
                                id: prose
                                objectName: "wikiProse"
                                visible: block.kind === "prose"
                                width: parent.width
                                textFormat: TextEdit.RichText
                                text: visible ? Theme.htmlStyle + Theme.proseHtml(block.html, -1, true) : ""
                                font.family: Theme.uiFont
                                font.pixelSize: Theme.px(13.5)
                                color: Theme.foreground
                                selectionColor: Theme.selection
                                selectedTextColor: Theme.foreground
                                readOnly: true
                                selectByMouse: true
                                wrapMode: TextEdit.Wrap
                                onLinkActivated: link => root.follow(link)
                                HoverHandler { cursorShape: prose.hoveredLink ? Qt.PointingHandCursor : Qt.IBeamCursor }
                            }
                            Rectangle {
                                id: codeWell
                                objectName: "wikiCode"
                                visible: block.kind === "code"
                                width: parent.width
                                height: visible ? code.implicitHeight + 16 : 0
                                radius: 6
                                color: Theme.codeBg
                                TextEdit {
                                    id: code
                                    x: 10
                                    y: 8
                                    width: parent.width - 20
                                    textFormat: TextEdit.RichText
                                    // (Long lines wrap in a narrow panel rather than being cut.)
                                    text: codeWell.visible ? root.wrappedCode(block.modelData.html || "", codeChar.advanceWidth("m")) : ""
                                    font.family: Theme.mono
                                    font.pixelSize: Theme.mpx(12)
                                    color: Theme.foreground
                                    selectionColor: Theme.selection
                                    readOnly: true
                                    selectByMouse: true
                                    wrapMode: TextEdit.Wrap
                                }
                            }
                            // A diagram, drawn: fitted, full size a click away.
                            Item {
                                id: diagram
                                objectName: "wikiDiagram"
                                // (A flowchart drawn across is drawn down as well, and
                                // shown down when that fits the panel larger, as the
                                // graph is.)
                                readonly property string source: block.kind === "mermaid" ? block.modelData.code || "" : ""
                                readonly property string sourceDown: source.replace(/^(\s*(?:%%[^\n]*\n\s*)*)(flowchart|graph)([ \t]+)(LR|RL)\b/, "$1$2$3TB")
                                function draw(src) {
                                    return src !== "" && Diagrams.revision >= 0
                                        ? JSON.parse(Diagrams.diagram(src, Theme.isLight, JSON.stringify({ background: String(Theme.canvas), surface: String(Theme.raised), text: String(Theme.foreground),
                                                                                                           muted: String(Theme.muted), accent: String(Theme.primary), font: Theme.uiFont })))
                                        : null
                                }
                                readonly property var across: draw(source)
                                readonly property var down: sourceDown !== source ? draw(sourceDown) : null
                                readonly property bool downward: !!across && !!down && across.state === "ready" && down.state === "ready"
                                    && Math.min(1, parent.width / Math.max(1, down.width)) > 1.15 * Math.min(1, parent.width / Math.max(1, across.width))
                                readonly property var pic: downward ? down : across
                                readonly property string shownSource: downward ? sourceDown : source
                                readonly property bool ready: !!pic && pic.state === "ready"
                                // (Never under 0.7, its labels' readable size: wider, it scrolls sideways.)
                                readonly property real fit: ready ? Math.max(0.7, Math.min(1, parent.width / Math.max(1, pic.width))) : 1
                                visible: block.kind === "mermaid"
                                width: parent.width
                                height: !visible ? 0 : ready ? Math.round(pic.height * fit) : diagramNote.implicitHeight
                                Label {
                                    id: diagramNote
                                    width: parent.width
                                    visible: !diagram.ready
                                    text: !diagram.pic ? "" : diagram.pic.state === "drawing" ? "Drawing the diagram…" : (diagram.pic.error || "")
                                    color: Theme.textMuted
                                    font.pixelSize: Theme.px(12)
                                    wrapMode: Text.Wrap
                                }
                                // (Wider than the page, it scrolls sideways.)
                                Flickable {
                                    id: diagramScroll
                                    width: parent.width
                                    height: diagramImage.height
                                    visible: diagram.ready
                                    contentWidth: diagramImage.width
                                    contentHeight: diagramImage.height
                                    flickableDirection: Flickable.HorizontalFlick
                                    interactive: contentWidth > width
                                    boundsBehavior: Flickable.StopAtBounds
                                    clip: true
                                    ScrollBar.horizontal: ScrollBar { policy: diagramScroll.contentWidth > diagramScroll.width ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }
                                    Image {
                                        id: diagramImage
                                        x: Math.max(0, (diagramScroll.width - width) / 2)
                                        visible: diagram.ready
                                        width: diagram.ready ? Math.round(diagram.pic.width * diagram.fit) : 0
                                        height: diagram.ready ? Math.round(diagram.pic.height * diagram.fit) : 0
                                        source: diagram.ready ? diagram.pic.url : ""
                                        sourceSize.width: width * 2
                                        fillMode: Image.PreserveAspectFit
                                        smooth: true
                                        mipmap: true
                                        asynchronous: true
                                        cache: false
                                        Accessible.role: Accessible.Graphic
                                        Accessible.name: "Diagram"
                                        HoverHandler { id: diagramHover; cursorShape: Qt.PointingHandCursor }
                                        TapHandler { onTapped: viewer.show(root.pageView.title, [{ label: "Diagram", url: diagram.pic.url, width: diagram.pic.width, height: diagram.pic.height }], 0, "Diagram", () => Diagrams.openInBrowser(diagram.shownSource, Theme.isLight), 2) }
                                        ToolTip.visible: diagramHover.hovered
                                        ToolTip.text: "Open full size"
                                        ToolTip.delay: 600
                                    }
                                }
                                // More to the side: that edge fades out.
                                Rectangle {
                                    visible: diagramScroll.contentX > 1
                                    x: diagramScroll.x
                                    y: diagramScroll.y
                                    width: 18
                                    height: diagramScroll.height
                                    gradient: Gradient {
                                        orientation: Gradient.Horizontal
                                        GradientStop { position: 0; color: Theme.canvas }
                                        GradientStop { position: 1; color: Qt.rgba(Theme.canvas.r, Theme.canvas.g, Theme.canvas.b, 0) }
                                    }
                                }
                                Rectangle {
                                    objectName: "wikiMoreRight"
                                    visible: diagramScroll.visible && diagramScroll.contentX + diagramScroll.width < diagramScroll.contentWidth - 1
                                    x: diagramScroll.x + diagramScroll.width - width
                                    y: diagramScroll.y
                                    width: 28
                                    height: diagramScroll.height
                                    gradient: Gradient {
                                        orientation: Gradient.Horizontal
                                        GradientStop { position: 0; color: Qt.rgba(Theme.canvas.r, Theme.canvas.g, Theme.canvas.b, 0) }
                                        GradientStop { position: 1; color: Theme.canvas }
                                    }
                                }
                            }
                        }
                    }
                    // Its footnotes: each opens the code it cites, at its
                    // lines now, marked when that code changed or is gone.
                    Label {
                        Layout.fillWidth: true
                        Layout.topMargin: 14
                        Layout.bottomMargin: 2
                        visible: footnoteRows.count > 0
                        text: "Footnotes"
                        color: Theme.textMuted
                        font.pixelSize: Theme.px(12)
                        font.weight: Font.DemiBold
                    }
                    Repeater {
                        id: footnoteRows
                        model: root.pageView && !root.pageView.error ? root.pageView.footnotes || [] : []
                        TextEdit {
                            required property var modelData
                            objectName: "wikiFootnote"
                            Layout.fillWidth: true
                            Layout.topMargin: 2
                            textFormat: TextEdit.RichText
                            text: {
                                const f = modelData, e = x => String(x).replace(/&/g, "&amp;").replace(/</g, "&lt;")
                                const where = e(f.path) + (f.locator ? ", " + e(f.locator) : "")
                                const link = f.state === "gone" ? where : "<a href=\"okf-fn:" + encodeURIComponent(f.label) + "\" style=\"font-family:'" + Theme.mono + "';font-size:" + Theme.mpx(12) + "px;\">" + where + "</a>"
                                const lines = f.start > 0 ? " <span style=\"color:" + Theme.textFaint + "\">lines " + (f.start === f.end ? f.start : f.start + "–" + f.end) + "</span>" : ""
                                const note = { stale: "its code changed since the page was written", missing: "names nothing in the file", gone: "the file isn't there" }[f.state]
                                return Theme.htmlStyle + "<span style=\"color:" + Theme.textFaint + "\">" + f.n + "</span>&nbsp; " + link + lines
                                    + (note ? " <span style=\"color:" + Theme.warning + "\">· " + note + "</span>" : "")
                            }
                            font.family: Theme.uiFont
                            font.pixelSize: Theme.px(12.5)
                            color: Theme.foreground
                            selectionColor: Theme.selection
                            selectedTextColor: Theme.foreground
                            readOnly: true
                            selectByMouse: true
                            wrapMode: TextEdit.Wrap
                            onLinkActivated: link => root.follow(decodeURIComponent(link))
                            HoverHandler { cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.IBeamCursor }
                        }
                    }
                    // The pages that link here.
                    Label {
                        Layout.fillWidth: true
                        Layout.topMargin: 14
                        Layout.bottomMargin: 2
                        visible: backlinkRows.count > 0
                        text: "Linked from"
                        color: Theme.textMuted
                        font.pixelSize: Theme.px(12)
                        font.weight: Font.DemiBold
                    }
                    Repeater {
                        id: backlinkRows
                        model: root.pageView && !root.pageView.error ? root.pageView.backlinks || [] : []
                        FlatButton {
                            required property var modelData
                            objectName: "wikiBacklink"
                            Layout.leftMargin: -8
                            outlined: false
                            padding: 4
                            text: modelData.title
                            font.pixelSize: Theme.px(12.5)
                            onClicked: root.openWiki(modelData.path, "", true)
                        }
                    }
                }
            }
        }

        // ---- the graph ----
        Flickable {
            id: graphBox
            objectName: "wikiGraph"
            Layout.fillWidth: true
            Layout.fillHeight: true
            // (As tall as the graph: its caption sits under it.)
            Layout.maximumHeight: graphCol.implicitHeight + 16
            Layout.topMargin: 10
            visible: root.mode === "graph" && root.st.exists && !root.reading && !root.showingResults
            clip: true
            contentHeight: graphCol.implicitHeight + 16
            contentWidth: Math.max(width, graphImage.Layout.preferredWidth + 32)
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            ScrollBar.horizontal: ScrollBar { }
            readonly property bool wider: graphImage.ready && contentWidth > width + 1
            onWidthChanged: Qt.callLater(root.centreGraph)
            onContentWidthChanged: Qt.callLater(root.centreGraph)
            // Ctrl+wheel zooms; the wheel alone scrolls.
            WheelHandler {
                acceptedModifiers: Qt.ControlModifier
                onWheel: event => root.graphZoom = Math.max(0.5, Math.min(3, root.graphZoom * (event.angleDelta.y > 0 ? 1.15 : 1 / 1.15)))
            }
            ColumnLayout {
                id: graphCol
                x: 18
                width: Math.max(graphBox.width - 32, graphImage.Layout.preferredWidth)
                spacing: 8
                Label {
                    Layout.fillWidth: true
                    visible: !!root.graphPic && root.graphPic.state !== "ready"
                    text: !root.graphPic ? "" : root.graphPic.state === "drawing" ? "Drawing the graph…" : (root.graphPic.error || "")
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(12)
                    wrapMode: Text.Wrap
                }
                Image {
                    id: graphImage
                    objectName: "wikiGraphPicture"
                    readonly property bool ready: !!root.graphPic && root.graphPic.state === "ready"
                    // Fitted to the panel, but never so small its names drop
                    // under the readable size (16px labels at 0.7: 11px); wider
                    // than the panel, it scrolls sideways.
                    readonly property real fit: ready ? Math.max(0.7, Math.min(1, root.graphRoom / Math.max(1, root.graphPic.width))) * root.graphZoom : 1
                    visible: ready
                    Layout.alignment: Qt.AlignHCenter
                    Layout.preferredWidth: ready ? Math.round(root.graphPic.width * fit) : 0
                    Layout.preferredHeight: ready ? Math.round(root.graphPic.height * fit) : 0
                    source: ready ? root.graphPic.url : ""
                    sourceSize.width: Layout.preferredWidth * 2
                    fillMode: Image.PreserveAspectFit
                    smooth: true
                    mipmap: true
                    asynchronous: true
                    cache: false
                    Accessible.role: Accessible.Graphic
                    Accessible.name: "The wiki's pages and their links"
                    HoverHandler { id: graphHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler {
                        onTapped: if (graphImage.hoveredBox < 0) root.openGraphFull()
                    }
                    onBoxesChanged: Qt.callLater(root.centreGraph)
                    ToolTip.visible: graphHover.hovered && hoveredBox < 0
                    ToolTip.text: "Open full size"
                    ToolTip.delay: 600
                    // Each page's box opens it.
                    property int hoveredBox: -1
                    readonly property var boxes: ready ? JSON.parse(Diagrams.boxes(root.graphShown)) : []
                    Repeater {
                        model: graphImage.boxes
                        Item {
                            id: box
                            required property var modelData
                            required property int index
                            readonly property var page: root.graphPages[Number(String(modelData.id).replace(/^p/, ""))] || null
                            objectName: "wikiGraphBox"
                            visible: page !== null
                            x: modelData.x * graphImage.width
                            y: modelData.y * graphImage.height
                            width: modelData.w * graphImage.width
                            height: modelData.h * graphImage.height
                            Accessible.role: Accessible.Button
                            Accessible.name: page ? "Read " + page.title + (lastRead ? " (read last)" : "") : ""
                            activeFocusOnTab: true
                            Keys.onReturnPressed: root.openWiki(page.path, "", false)
                            Keys.onSpacePressed: root.openWiki(page.path, "", false)
                            readonly property bool lastRead: !!page && page.path === root.lastRead
                            Rectangle {
                                anchors.fill: parent
                                anchors.margins: -2
                                radius: 3
                                color: "transparent"
                                border.width: 2
                                border.color: Theme.primary
                                // (The page read last stays ringed, fainter.)
                                visible: boxHover.hovered || box.activeFocus || box.lastRead
                                opacity: boxHover.hovered || box.activeFocus ? 1 : 0.55
                            }
                            HoverHandler {
                                id: boxHover
                                cursorShape: Qt.PointingHandCursor
                                onHoveredChanged: graphImage.hoveredBox = hovered ? box.index : (graphImage.hoveredBox === box.index ? -1 : graphImage.hoveredBox)
                            }
                            TapHandler { gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: root.openWiki(box.page.path, "", false) }
                            ToolTip.visible: boxHover.hovered
                            ToolTip.text: box.page ? "Read " + box.page.title : ""
                            ToolTip.delay: 400
                        }
                    }
                }
            }
        }

        // The graph's caption and zoom, kept in view under it (the zoom
        // under the caption in a narrow panel).
        GridLayout {
            objectName: "wikiGraphCaption"
            Layout.fillWidth: true
            Layout.leftMargin: 18
            Layout.rightMargin: 14
            Layout.bottomMargin: 8
            visible: graphBox.visible && graphImage.visible
            columns: root.width < 360 ? 1 : 2
            columnSpacing: 6
            rowSpacing: 2
            Label {
                Layout.fillWidth: true
                text: (root.graphPages.length < root.pages.length ? "The open page and the pages it links to or from. " : "") + "An arrow is a link from one page to another. Click a page to read it."
                      + (graphBox.wider ? " It's wider than the panel: scroll sideways, or open it full size." : "")
                color: Theme.textFaint
                font.pixelSize: Theme.px(11.5)
                wrapMode: Text.Wrap
            }
            RowLayout {
                Layout.alignment: Qt.AlignTop | (root.width < 360 ? Qt.AlignLeft : Qt.AlignRight)
                Layout.leftMargin: root.width < 360 ? -6 : 0
                spacing: 2
                ComposerIconButton {
                    objectName: "wikiGraphZoomOut"
                    iconName: "subtract"; tint: Theme.muted
                    tip: "Zoom out (Ctrl+wheel)"
                    enabled: root.graphZoom > 0.51
                    onClicked: root.graphZoom = Math.max(0.5, root.graphZoom / 1.25)
                }
                FlatButton {
                    objectName: "wikiGraphZoom"
                    Layout.minimumWidth: 40
                    // (The size it's drawn at, not the zoom over its fit.)
                    text: Math.round((graphImage.ready ? graphImage.fit : root.graphZoom) * 100) + "%"
                    font.pixelSize: Theme.px(11)
                    outlined: false
                    padding: 4
                    ToolTip.visible: hovered
                    ToolTip.text: "Fit the panel"
                    ToolTip.delay: 600
                    onClicked: root.graphZoom = 1
                }
                ComposerIconButton {
                    objectName: "wikiGraphZoomIn"
                    iconName: "add"; tint: Theme.muted
                    tip: "Zoom in (Ctrl+wheel)"
                    enabled: root.graphZoom < 2.99
                    onClicked: root.graphZoom = Math.min(3, root.graphZoom * 1.25)
                }
                ComposerIconButton {
                    objectName: "wikiGraphFull"
                    iconName: "fullscreen"; tint: Theme.muted
                    tip: "Open full size"
                    onClicked: root.openGraphFull()
                }
            }
        }
        // (Over the graph: its sides fade while there's more.)
        Item {
            Layout.preferredHeight: 0
            Layout.fillWidth: true
            visible: graphBox.visible
            // More to the side: that edge fades out.
            Rectangle {
                visible: graphBox.contentX > 1
                x: 0
                y: graphBox.y - parent.y
                width: 18
                height: graphBox.height
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0; color: Theme.canvas }
                    GradientStop { position: 1; color: Qt.rgba(Theme.canvas.r, Theme.canvas.g, Theme.canvas.b, 0) }
                }
            }
            Rectangle {
                objectName: "wikiMoreRight"
                visible: graphBox.visible && graphBox.contentX + graphBox.width < graphBox.contentWidth - 1
                x: graphBox.width - width
                y: graphBox.y - parent.y
                width: 28
                height: graphBox.height
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0; color: Qt.rgba(Theme.canvas.r, Theme.canvas.g, Theme.canvas.b, 0) }
                    GradientStop { position: 1; color: Theme.canvas }
                }
            }
        }

        // ---- the pages ----
        // By type, when there's more than one.
        Flow {
            objectName: "wikiTypes"
            Layout.fillWidth: true
            Layout.leftMargin: 12
            Layout.rightMargin: 12
            Layout.topMargin: 8
            visible: list.visible && root.types.length > 1
            spacing: 4
            Repeater {
                model: [""].concat(root.types)
                FlatButton {
                    required property string modelData
                    objectName: "wikiType-" + (modelData || "all")
                    text: modelData || "All"
                    outlined: root.typeFilter === modelData
                    padding: 5
                    font.pixelSize: Theme.px(12)
                    Accessible.name: (modelData || "All") + " pages"
                    onClicked: root.typeFilter = modelData
                }
            }
        }
        ListView {
            id: list
            objectName: "wikiPages"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.topMargin: 8
            visible: root.st.exists && root.mode === "pages" && !root.reading && !root.showingResults
            clip: true
            model: root.pageRows
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            section.property: "group"
            section.delegate: Label {
                required property string section
                width: ListView.view.width
                leftPadding: 18
                topPadding: section === "" ? 0 : 14
                bottomPadding: section === "" ? 0 : 4
                height: section === "" ? 0 : implicitHeight
                text: section
                color: Theme.label
                font.pixelSize: Theme.px(12)
                font.weight: Font.Medium
            }
            delegate: ItemDelegate {
                id: row
                objectName: "wikiPageRow"
                required property var modelData
                width: ListView.view.width
                leftInset: 8
                rightInset: 8
                leftPadding: 18
                rightPadding: 14
                topPadding: 7
                bottomPadding: 7
                hoverEnabled: true
                background: Rectangle { radius: 8; color: row.hovered || row.activeFocus ? Theme.rowHover : "transparent" }
                contentItem: ColumnLayout {
                    spacing: 2
                    PlainLabel {
                        Layout.fillWidth: true
                        text: row.modelData.title
                        color: Theme.text
                        font.pixelSize: Theme.px(13)
                        font.weight: Font.Medium
                        elide: Text.ElideRight
                    }
                    PlainLabel {
                        Layout.fillWidth: true
                        visible: text !== ""
                        text: row.modelData.description
                        color: Theme.textMuted
                        font.pixelSize: Theme.px(12)
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                    }
                    // What it owes the code, or an edit not checked yet.
                    PlainLabel {
                        objectName: "wikiPageMark"
                        Layout.fillWidth: true
                        visible: text !== ""
                        text: row.modelData.owes > 0 ? (row.modelData.owes === 1 ? "1 thing to do" : row.modelData.owes + " things to do")
                            : row.modelData.edited ? "Edited, not checked yet" : ""
                        color: row.modelData.owes > 0 ? Theme.warning : Theme.textFaint
                        font.pixelSize: Theme.px(11.5)
                        elide: Text.ElideRight
                    }
                }
                onClicked: root.openWiki(modelData.path, "", false)
            }
        }
        Label {
            Layout.margins: 18
            Layout.fillWidth: true
            visible: root.st.exists && root.pages.length > 0 && root.pageRows.length === 0
            wrapMode: Text.Wrap
            text: "No " + root.typeFilter + " pages."
            color: Theme.textMuted
            font.pixelSize: Theme.px(12.5)
        }
        Item { Layout.fillHeight: true; visible: !list.visible && !hitList.visible && !(root.reading && root.st.exists) }

        // ---- what it owes the code ----
        Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.hairline; visible: footer.visible }
        ColumnLayout {
            id: footer
            objectName: "wikiFooter"
            Layout.fillWidth: true
            Layout.leftMargin: 18
            Layout.rightMargin: 14
            Layout.topMargin: 10
            Layout.bottomMargin: 12
            visible: root.st.project >= 0 && ((root.st.exists && root.pages.length > 0) || !!root.st.running || root.runError !== "")
            spacing: 8
            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                // What it owes, in phrases that each stay whole on a line
                // where they fit: a narrow panel breaks at the dot, and a
                // phrase wider than the panel breaks between its words.
                Flow {
                    id: freshness
                    objectName: "wikiFreshness"
                    Layout.fillWidth: true
                    readonly property var phrases: {
                        const w = root.work, n = (k, one, many) => k + " " + (k === 1 ? one : many)
                        if (root.st.running) return ["A session is writing the wiki."]
                        if (root.st.checking) return ["Checking what changed…"]
                        if (root.st.loading) return ["Reading the wiki…"]
                        const out = []
                        if (w.stale > 0) out.push(n(w.stale, "footnote's code changed", "footnotes' code changed"))
                        if (w.new > 0) out.push(n(w.new, "new declaration no page mentions", "new declarations no page mentions"))
                        if (w.unowned > 0) out.push(n(w.unowned, "file no page covers", "files no page covers"))
                        if (w.broken > 0) out.push(n(w.broken, "footnote finds no code", "footnotes find no code"))
                        if (out.length > 0) return out
                        return ["Up to date"].concat(root.st.last ? ["written " + root.ago(root.st.last)] : [])
                    }
                    readonly property string text: phrases.join(" · ")
                    Repeater {
                        model: freshness.phrases
                        Label {
                            required property int index
                            required property string modelData
                            text: modelData + (index < freshness.phrases.length - 1 ? " ·\u00a0" : "")
                            width: Math.min(implicitWidth, freshness.width)
                            wrapMode: Text.WordWrap
                            color: root.work.count > 0 && !root.st.running ? Theme.foreground : Theme.textMuted
                            font.pixelSize: Theme.px(12)
                        }
                    }
                }
                // The session writing it, unless it's this one.
                FlatButton {
                    objectName: "wikiWatch"
                    visible: !!root.st.running && !!root.st.session && root.st.session !== root.sessionKey
                    text: "Show the session"
                    font.pixelSize: Theme.px(12)
                    onClicked: Workspace.open(root.st.session)
                }
                FlatButton {
                    objectName: "wikiRun"
                    visible: !root.st.running && !root.st.checking && root.pages.length > 0
                    // (Owing the code: the one thing to do here.)
                    variant: root.work.count > 0 ? "primary" : "outline"
                    text: "Update"
                    font.pixelSize: Theme.px(12)
                    onClicked: root.start("update")
                    ToolTip.visible: hovered
                    ToolTip.text: (root.work.count > 0 ? "Start a session that deals with what the wiki owes the code"
                                   : "harlo checks the code first, and starts a session only when there's something to do") + (root.updaterText ? ". " + root.updaterText + "." : "")
                    ToolTip.delay: 600
                }
            }
            Label {
                Layout.fillWidth: true
                visible: root.runError !== ""
                text: root.runError
                color: Theme.error
                font.pixelSize: Theme.px(12)
                wrapMode: Text.Wrap
            }
            // Whether the project's agents use it.
            RowLayout {
                Layout.fillWidth: true
                visible: root.st.exists
                spacing: 10
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: root.st.enabled ? "Agents are pointed to it, in sessions started from now on." : "Agents aren't told of it: switched off for this project."
                    color: Theme.textFaint
                    font.pixelSize: Theme.px(11.5)
                }
                AppSwitch {
                    objectName: "wikiSwitch"
                    checked: !!root.st.enabled
                    Accessible.name: "Agents use the wiki"
                    onToggled: { Workspace.setSetting("wiki_" + root.st.project, checked ? "1" : "0"); root.load() }
                }
            }
        }
    }
}
