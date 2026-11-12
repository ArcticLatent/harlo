import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// The wiki of the active session's repository, kept by OpenWiki in its
// openwiki/ folder: its pages by folder, each read here (links between
// pages open in the panel, with Back; links to code open the file at their
// lines), a search, the pages' links drawn as a graph, and how fresh it is.
// Create, Update and Continue start a session that writes it with the
// project's agent; the result shows in Changes like any edit.
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
    function load() {
        st = JSON.parse(Workspace.wikiJson(sessionKey))
        if (st.root !== "" && st.root !== shownRoot) {
            shownRoot = st.root
            closePage(); results = null; query = ""; searchField.text = ""; mode = "pages"
        }
    }
    onSessionKeyChanged: if (visible) load()
    onVisibleChanged: if (visible) load()
    Component.onCompleted: if (visible) load()
    // While a run writes it, its progress.
    Timer {
        interval: 2500
        repeat: true
        running: root.visible && (!!root.st.running || !!root.st.run)
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
            reader.contentY = 0
            if (root.wantSection !== "") Qt.callLater(root.showSection, root.wantSection)
        }
        function onWikiSearchReady(query, json) {
            if (query !== root.query) return
            root.searching = false
            root.results = JSON.parse(json)
        }
    }

    // ---- reading ----

    /// Opens a page (as a search ref or a link names it: "openwiki/x.md",
    /// "x.md"), at a section when given.
    function openWiki(page, section, keep) {
        const p = String(page).replace(/^\.?\//, "").replace(/^openwiki\//, "")
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
        if (href.startsWith("repo://")) {
            const m = /^repo:\/\/([^#]+)(?:#L(\d+))?/.exec(href)
            if (m) root.openFileRequested(root.st.root + "/" + decodeURIComponent(m[1]), m[2] ? Number(m[2]) : 0)
            return
        }
        if (href.startsWith("#")) { root.showSection(href.substring(1)); return }
        if (/^[a-z][a-z0-9+.-]*:/i.test(href)) { Qt.openUrlExternally(href); return }
        const [path, section] = href.split("#")
        if (!path.endsWith(".md")) { root.openFileRequested(root.st.root + "/" + path, 0); return }
        const parts = root.openPage.split("/").slice(0, -1)
        for (const c of path.split("/")) {
            if (c === "..") parts.pop()
            else if (c !== "." && c !== "") parts.push(c)
        }
        root.openWiki(parts.join("/"), section || "", true)
    }

    // ---- searching ----

    function search(text) {
        root.query = text.trim()
        if (root.query === "") { root.results = null; root.searching = false; return }
        root.searching = true
        Workspace.requestWikiSearch(root.sessionKey, root.query, root.workspace)
    }

    /// A hit's snippet as styled text, the query's words in the text
    /// colour and medium weight (the rest stays muted), as session search
    /// marks its matches.
    function marked(text, query) {
        const esc = s => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
        const words = query.toLowerCase().split(/[^0-9A-Za-z\u00C0-\uFFFF]+/).filter(w => w.length > 1)
        if (words.length === 0) return esc(text)
        // (The whole word a match is in: "amended" for "amend".)
        const re = new RegExp("([0-9A-Za-z\u00C0-\uFFFF]*(?:" + words.map(w => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|") + ")[0-9A-Za-z\u00C0-\uFFFF]*)", "gi")
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
    readonly property var pageRows: pages.map(p => Object.assign({ group: groupOf(p.path) }, p))
    function titleOf(path) {
        const p = pages.find(x => x.path === path.replace(/^openwiki\//, ""))
        return p ? p.title : path.replace(/^openwiki\//, "")
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
    /// the open page (else the quickstart) and its neighbours.
    readonly property var graphPages: {
        if (pages.length < 2) return []
        if (pages.length <= 40) return pages
        const at = root.openPage !== "" ? root.openPage : (pages[0] ? pages[0].path : "")
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
    function drawGraph(source) {
        return mode === "graph" && source !== "" && Diagrams.revision >= 0
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
    readonly property bool reading: openPage !== "" || loadingPage !== ""
    readonly property bool showingResults: results !== null || searching

    PictureViewer { id: viewer }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        PanelHeader {
            title: "Wiki"
            subtitle: root.st.root.replace(/\/+$/, "").split("/").pop()
            onCloseRequested: root.closeRequested()
            PanelButton { objectName: "wikiRefresh"; iconName: "refresh"; tip: "Refresh"; onClicked: { root.load(); if (root.openPage !== "") root.openWiki(root.openPage, "", false) } }
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
            visible: root.st.project >= 0 && !!root.st.loading
            wrapMode: Text.Wrap
            text: "Reading the wiki on " + root.st.host + "…"
            color: Theme.textMuted
            font.pixelSize: Theme.px(12.5)
        }

        // ---- none yet ----
        ColumnLayout {
            objectName: "wikiEmpty"
            Layout.fillWidth: true
            Layout.margins: 18
            visible: root.st.project >= 0 && !root.st.exists && !root.st.loading
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
                text: "A wiki of this project, written by your agent from the code: how its parts work and why, each fact tied to the lines it came from. Agents search it instead of reading through the code, and you can read it here."
                color: Theme.textMuted
                font.pixelSize: Theme.px(12.5)
                lineHeight: 1.15
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: "It's written in a session you can watch, a page at a time, and spends that agent's tokens: more for a bigger repository. The pages land in openwiki/ and show in Changes for you to review and commit."
                color: Theme.textFaint
                font.pixelSize: Theme.px(12)
                lineHeight: 1.15
            }
            FlatButton {
                objectName: "wikiCreate"
                Layout.topMargin: 4
                variant: "primary"
                enabled: !!root.st.runtime && !root.st.running
                text: root.st.running ? "Writing the wiki…" : "Create wiki"
                font.pixelSize: Theme.px(12.5)
                onClicked: root.start("create")
            }
            Label {
                Layout.fillWidth: true
                visible: !root.st.runtime
                wrapMode: Text.Wrap
                text: "This harlo has no OpenWiki: reinstall harlo to create one."
                color: Theme.warning
                font.pixelSize: Theme.px(12)
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
                        : root.results.workspaces.length > 0 ? "This wiki is linked to others. Search which workspace?"
                        : root.results.hits.length === 0 ? "Nothing in the wiki matches “" + root.query + "”."
                        : root.results.plain ? "OpenWiki couldn't run, so these are the pages that have its words."
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
                    Label {
                        Layout.fillWidth: true
                        text: hit.modelData.heading || root.titleOf(hit.modelData.page)
                        color: Theme.text
                        font.pixelSize: Theme.px(13)
                        font.weight: Font.Medium
                        elide: Text.ElideRight
                    }
                    Label {
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
                    Label {
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
                    Label {
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
                    Label {
                        objectName: "wikiPageMeta"
                        Layout.fillWidth: true
                        Layout.topMargin: 8
                        Layout.bottomMargin: 10
                        visible: text !== ""
                        text: !root.pageView || root.pageView.error ? "" : [root.pageView.type ? root.pageView.type.charAt(0).toUpperCase() + root.pageView.type.slice(1) : "",
                                                                           root.pageView.verified ? "checked against the code " + root.ago(root.pageView.verified) : ""].filter(x => x !== "").join(" · ")
                        color: Theme.textFaint
                        font.pixelSize: Theme.px(11.5)
                        elide: Text.ElideRight
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
                                    text: codeWell.visible ? block.modelData.html : ""
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
                                readonly property var pic: block.kind === "mermaid" && Diagrams.revision >= 0
                                    ? JSON.parse(Diagrams.diagram(block.modelData.code, Theme.isLight, JSON.stringify({ background: String(Theme.canvas), surface: String(Theme.raised), text: String(Theme.foreground),
                                                                                                                        muted: String(Theme.muted), accent: String(Theme.primary), font: Theme.uiFont })))
                                    : null
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
                                        TapHandler { onTapped: viewer.show(root.pageView.title, [{ label: "Diagram", url: diagram.pic.url, width: diagram.pic.width, height: diagram.pic.height }], 0, "Diagram", () => Diagrams.openInBrowser(block.modelData.code, Theme.isLight), 2) }
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
                    // Where in the code the page comes from: its sources and
                    // the lines its facts cite, each opening the file there.
                    Label {
                        Layout.fillWidth: true
                        Layout.topMargin: 14
                        Layout.bottomMargin: 2
                        visible: sourceRows.count > 0
                        text: "Sources"
                        color: Theme.textMuted
                        font.pixelSize: Theme.px(12)
                        font.weight: Font.DemiBold
                    }
                    Repeater {
                        id: sourceRows
                        model: root.pageView && !root.pageView.error ? root.pageView.sources || [] : []
                        TextEdit {
                            required property var modelData
                            objectName: "wikiSource"
                            Layout.fillWidth: true
                            Layout.topMargin: 2
                            textFormat: TextEdit.RichText
                            text: {
                                const p = modelData.path, e = s => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;")
                                const lines = (modelData.ranges || []).map(r => "<a href=\"repo://" + encodeURI(p) + "#L" + r[0] + "-L" + r[1] + "\">" + (r[0] === r[1] ? r[0] : r[0] + "–" + r[1]) + "</a>")
                                const first = (modelData.ranges || [])[0]
                                const cited = modelData.claims === 1 ? "cited once" : modelData.claims === 2 ? "cited twice" : "cited " + modelData.claims + " times"
                                return Theme.htmlStyle + "<a href=\"repo://" + encodeURI(p) + (first ? "#L" + first[0] : "") + "\" style=\"font-family:'" + Theme.mono + "';font-size:" + Theme.mpx(12) + "px;\">" + e(p) + "</a>"
                                    + (lines.length ? " <span style=\"color:" + Theme.textFaint + "\">lines</span> " + lines.join(", ") : "")
                                    + (modelData.claims ? " <span style=\"color:" + Theme.textFaint + "\">· " + cited + "</span>" : "")
                            }
                            font.family: Theme.uiFont
                            font.pixelSize: Theme.px(12.5)
                            color: Theme.foreground
                            selectionColor: Theme.selection
                            selectedTextColor: Theme.foreground
                            readOnly: true
                            selectByMouse: true
                            wrapMode: TextEdit.Wrap
                            onLinkActivated: link => root.follow(link)
                            HoverHandler { cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.IBeamCursor }
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
                        onTapped: if (graphImage.hoveredBox < 0) viewer.show("Wiki", [{ label: "Graph", url: root.graphPic.url, width: root.graphPic.width, height: root.graphPic.height }], 0, "Graph",
                                                                             () => Diagrams.openInBrowser(root.graphShown, Theme.isLight), 2)
                    }
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
                            Accessible.name: page ? "Read " + page.title : ""
                            activeFocusOnTab: true
                            Keys.onReturnPressed: root.openWiki(page.path, "", false)
                            Keys.onSpacePressed: root.openWiki(page.path, "", false)
                            Rectangle {
                                anchors.fill: parent
                                anchors.margins: -2
                                radius: 3
                                color: "transparent"
                                border.width: 2
                                border.color: Theme.primary
                                visible: boxHover.hovered || box.activeFocus
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

        // The graph's caption and zoom, kept in view under it.
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 18
            Layout.rightMargin: 14
            Layout.bottomMargin: 8
            visible: graphBox.visible && graphImage.visible
            spacing: 2
            Label {
                Layout.fillWidth: true
                text: (root.graphPages.length < root.pages.length ? "The open page and the pages it links to or from. " : "") + "An arrow is a link from one page to another. Click a page to read it."
                color: Theme.textFaint
                font.pixelSize: Theme.px(11.5)
                wrapMode: Text.Wrap
            }
            ComposerIconButton {
                objectName: "wikiGraphZoomOut"
                Layout.alignment: Qt.AlignTop
                iconName: "subtract"; tint: Theme.muted
                tip: "Zoom out (Ctrl+wheel)"
                enabled: root.graphZoom > 0.51
                onClicked: root.graphZoom = Math.max(0.5, root.graphZoom / 1.25)
            }
            FlatButton {
                objectName: "wikiGraphZoom"
                Layout.alignment: Qt.AlignTop
                Layout.minimumWidth: 40
                text: Math.round(root.graphZoom * 100) + "%"
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
                Layout.alignment: Qt.AlignTop
                iconName: "add"; tint: Theme.muted
                tip: "Zoom in (Ctrl+wheel)"
                enabled: root.graphZoom < 2.99
                onClicked: root.graphZoom = Math.min(3, root.graphZoom * 1.25)
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
                    Label {
                        Layout.fillWidth: true
                        text: row.modelData.title
                        color: Theme.text
                        font.pixelSize: Theme.px(13)
                        font.weight: Font.Medium
                        elide: Text.ElideRight
                    }
                    Label {
                        Layout.fillWidth: true
                        visible: text !== ""
                        text: row.modelData.description
                        color: Theme.textMuted
                        font.pixelSize: Theme.px(12)
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                    }
                }
                onClicked: root.openWiki(modelData.path, "", false)
            }
        }
        Label {
            Layout.margins: 18
            Layout.fillWidth: true
            visible: root.st.exists && root.pages.length === 0 && !root.st.run
            wrapMode: Text.Wrap
            text: "The wiki has no pages yet."
            color: Theme.textMuted
            font.pixelSize: Theme.px(12.5)
        }
        Item { Layout.fillHeight: true; visible: !list.visible && !hitList.visible && !(root.reading && root.st.exists) }

        // ---- how fresh it is, and its runs ----
        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.hairline; visible: footer.visible }
        ColumnLayout {
            id: footer
            objectName: "wikiFooter"
            Layout.fillWidth: true
            Layout.leftMargin: 18
            Layout.rightMargin: 14
            Layout.topMargin: 10
            Layout.bottomMargin: 12
            visible: root.st.project >= 0 && (root.st.exists || !!root.st.run || root.runError !== "")
            spacing: 8
            readonly property var run: root.st.run || null
            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                Label {
                    objectName: "wikiFreshness"
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: {
                        const r = footer.run, l = root.st.last
                        if (r && root.st.running) return r.phase === "planning" || r.total === 0 ? "Planning the wiki…" : "Writing pages: " + r.done + " of " + r.total
                        if (r) return "A run stopped part way" + (r.total > 0 ? " (" + r.done + " of " + r.total + " pages)" : "") + "."
                        if (root.st.running) return "Starting a run…"
                        if (!l) return "Not written by OpenWiki yet."
                        const when = "Updated " + root.ago(l.at)
                        const b = root.st.behind
                        return b === null || b === undefined ? when : b === 0 ? when + " · up to date" : when + " · " + b + (b === 1 ? " commit" : " commits") + " since"
                    }
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(12)
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
                    visible: !root.st.running && !!root.st.runtime
                    // (Behind the code: the one thing to do here.)
                    variant: footer.run || (root.st.behind || 0) > 0 ? "primary" : "outline"
                    text: footer.run ? "Continue" : "Update"
                    font.pixelSize: Theme.px(12)
                    onClicked: root.start(footer.run ? "continue" : "update")
                    ToolTip.visible: hovered
                    ToolTip.text: footer.run ? "Start a session that finishes this run" : "Start a session that brings the wiki in line with the code"
                    ToolTip.delay: 600
                }
            }
            AppProgressBar {
                Layout.fillWidth: true
                visible: !!footer.run && !!root.st.running
                indeterminate: !!footer.run && (footer.run.phase === "planning" || footer.run.total === 0)
                from: 0
                to: footer.run ? Math.max(1, footer.run.total) : 1
                value: footer.run ? footer.run.done : 0
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
                    text: root.st.enabled ? "Agents search it, in sessions started from now on." : "Agents don't use it: switched off for this project."
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
