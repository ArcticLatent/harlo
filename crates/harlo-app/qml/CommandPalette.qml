import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Ctrl+K: fuzzy search over sessions, projects and actions.
// ">" lists actions only; "@" searches files in the active session's folder.
Popup {
    id: root
    required property var actions    // function(): [{title, hint, run}]
    required property ChatSession session
    signal newSessionRequested(int projectId)
    signal insertFile(string path)

    function openWith(prefix) {
        input.text = prefix || ""
        open()
        input.forceActiveFocus()
        refresh()
    }

    x: Math.round((parent.width - width) / 2)
    y: Math.round(parent.height * 0.12)
    width: Math.min(680, parent.width - 40)
    height: Math.min(body.implicitHeight + topPadding + bottomPadding, parent.height * 0.7)
    padding: 8
    modal: true
    focus: true
    background: GlassSurface { radius: 14; kind: "dialog" }
    Overlay.modal: GlassScrim { }

    property var items: []
    // Test hook: the rows that can be chosen (not the headings).
    readonly property var choices: items.filter(i => i.kind !== "header")
    readonly property int current: list.currentIndex

    // Light fuzzy test for the short action list: letters in order.
    function fuzzy(needle, hay) {
        needle = needle.toLowerCase(); hay = hay.toLowerCase()
        let i = 0
        for (const c of hay) if (c === needle[i]) i++
        return i === needle.length
    }

    // The words typed: a session's or action's title with the letters that
    // matched marked. A word found whole (at a word's start if it can be)
    // is one mark; otherwise its letters in order, marked only where they
    // start a word or run two or more, so a loose match isn't confetti.
    readonly property string words: input.text.startsWith(">") || input.text.startsWith("@") ? input.text.substring(1).trim() : input.text.trim()
    function wordStart(lower, i) { return i === 0 || !/[a-z0-9]/.test(lower[i - 1]) }
    function findWord(lower, w) {
        let at = lower.indexOf(w), first = at
        while (at >= 0 && !wordStart(lower, at)) at = lower.indexOf(w, at + 1)
        if (at >= 0) return at
        return w.length >= 2 ? first : -1
    }
    function marks(title, query) {
        const lower = title.toLowerCase()
        const on = new Array(title.length).fill(false)
        for (const w of query.toLowerCase().split(/\s+/).filter(x => x)) {
            const at = findWord(lower, w)
            if (at >= 0) {
                for (let i = at; i < at + w.length; i++) on[i] = true
                continue
            }
            const hit = []
            let i = 0
            for (const c of w) {
                while (i < lower.length && lower[i] !== c) i++
                if (i < lower.length) hit.push(i++)
            }
            for (let k = 0; k < hit.length;) {
                let e = k
                while (e + 1 < hit.length && hit[e + 1] === hit[e] + 1) e++
                if (e > k || wordStart(lower, hit[k]))
                    for (let j = k; j <= e; j++) on[hit[j]] = true
                k = e + 1
            }
        }
        return on
    }
    // 1 when every word is found whole (or starts a word), else 0: the
    // strong matches are listed first.
    function strength(title, query) {
        const lower = title.toLowerCase()
        return query.toLowerCase().split(/\s+/).filter(x => x).every(w => findWord(lower, w) >= 0) ? 1 : 0
    }
    function marked(title, query) {
        const esc = c => c === "&" ? "&amp;" : c === "<" ? "&lt;" : c === ">" ? "&gt;" : c
        if (!query) return title.split("").map(esc).join("")
        const on = marks(title, query)
        let out = "", open = false
        for (let i = 0; i < title.length; i++) {
            if (on[i] !== open) {
                out += on[i] ? "<b><font color='" + Theme.rgb(Theme.accent) + "'>" : "</font></b>"
                open = on[i]
            }
            out += esc(title[i])
        }
        return out + (open ? "</font></b>" : "")
    }

    function refresh() {
        const q = input.text
        const out = []
        const section = (title, rows) => {
            if (rows.length === 0) return
            if (title !== "") out.push({ kind: "header", title: title })
            for (const r of rows) out.push(r)
        }
        if (q.startsWith("@")) {
            section("", root.session.fileMatches.map(f => ({ kind: "file", title: f, run: () => root.insertFile(f) })))
        } else {
            const actionOnly = q.startsWith(">")
            const query = actionOnly ? q.substring(1).trim() : q.trim()
            const acts = root.actions().filter(a => query === "" || fuzzy(query, a.title)).map(a => Object.assign({ kind: "action" }, a))
            if (actionOnly) {
                section("", acts)
            } else {
                const sessions = [], projects = []
                const titles = new Set(acts.map(a => a.title))
                for (const line of Workspace.search(query, query === "" ? 8 : 40)) {
                    const p = line.split("\t")
                    if (p[0] === "session") {
                        sessions.push({ kind: "session", title: p[2], subtitle: p[3], harness: p[4], modified: Number(p[5]) || 0, status: p[6] || "", run: () => Workspace.open(p[1]) })
                    } else if (p[0] === "project" && query !== "") {
                        // (An action already offers the project shown.)
                        const title = "New session in " + p[2]
                        if (titles.has(title)) continue
                        const id = Number(p[1].split(":")[1])
                        projects.push({ kind: "project", title: title, subtitle: p[3], run: () => root.newSessionRequested(id) })
                    }
                }
                // (All in one project: it needn't be said on every row.)
                if (sessions.length > 1 && sessions.every(r => r.subtitle === sessions[0].subtitle))
                    for (const r of sessions) r.subtitle = ""
                if (query !== "") {
                    // (Stable: within each, the ranking's order.)
                    const strong = list => list.filter(r => strength(r.title, query) === 1).concat(list.filter(r => strength(r.title, query) === 0))
                    sessions.splice(0, sessions.length, ...strong(sessions))
                    acts.splice(0, acts.length, ...strong(acts))
                }
                if (query === "") {
                    section("Recent sessions", sessions)
                    section("Actions", acts)
                } else {
                    section("Actions", acts.slice(0, 6))
                    section("Sessions", sessions)
                    section("Projects", projects)
                }
            }
        }
        items = out
        list.currentIndex = out.findIndex(i => i.kind !== "header")
        list.positionViewAtBeginning()
    }
    // Up and down skip the headings.
    function move(d) {
        let i = list.currentIndex
        do { i += d } while (i >= 0 && i < items.length && items[i].kind === "header")
        if (i >= 0 && i < items.length) {
            list.currentIndex = i
            // (With its heading, when it's the first under one.)
            list.positionViewAtIndex(i > 0 && items[i - 1].kind === "header" ? i - 1 : i, ListView.Contain)
            list.positionViewAtIndex(i, ListView.Contain)
        } else if (d < 0) {
            list.positionViewAtBeginning()
        }
    }

    function runCurrent() {
        const item = items[list.currentIndex]
        if (!item || item.kind === "header") return
        close()
        item.run()
    }

    Connections {
        target: root.session
        function onFileMatchesChanged() { if (root.opened && input.text.startsWith("@")) root.refresh() }
    }

    // For `--selftest` screenshots (a Popup's own content item can't be grabbed).
    function grab(path) { body.grabToImage(r => r.saveToFile(path)) }

    ColumnLayout {
        id: body
        anchors.fill: parent
        spacing: 6
        AppTextField {
            id: input
            objectName: "paletteField"
            Layout.fillWidth: true
            leftPadding: 32
            placeholderText: "Go to a session or action…"
            font.pixelSize: Theme.px(14)
            Icon { x: 10; anchors.verticalCenter: parent.verticalCenter; name: "search"; color: Theme.muted; implicitWidth: 15; implicitHeight: 15 }
            onTextChanged: {
                if (text.startsWith("@")) root.session.searchFiles(text.substring(1))
                root.refresh()
            }
            Keys.onDownPressed: root.move(1)
            Keys.onUpPressed: root.move(-1)
            Keys.onReturnPressed: root.runCurrent()
            Keys.onEnterPressed: root.runCurrent()
            Keys.onEscapePressed: root.close()
        }
        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredHeight: contentHeight
            visible: count > 0
            clip: true
            model: root.items
            highlightMoveDuration: 0
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            delegate: Loader {
                id: rowLoader
                required property var modelData
                required property int index
                width: ListView.view.width
                sourceComponent: modelData.kind === "header" ? headerRow : itemRow
                Component {
                    id: headerRow
                    Label {
                        topPadding: rowLoader.index > 0 ? 10 : 4
                        bottomPadding: 4
                        leftPadding: 10
                        text: rowLoader.modelData.title
                        color: Theme.textMuted
                        font.pixelSize: Theme.px(11.5)
                        font.weight: Font.DemiBold
                    }
                }
                Component {
                    id: itemRow
                    ItemDelegate {
                        id: row
                        readonly property var item: rowLoader.modelData
                        height: item.subtitle ? 42 : 32
                        topPadding: 2
                        bottomPadding: 2
                        leftPadding: 10
                        rightPadding: 10
                        highlighted: rowLoader.index === list.currentIndex
                        background: Rectangle { radius: 6; color: row.highlighted ? Theme.selected : row.hovered ? Theme.hover : "transparent" }
                        onClicked: { list.currentIndex = rowLoader.index; root.runCurrent() }
                        contentItem: RowLayout {
                            spacing: 10
                            Item {
                                implicitWidth: 18
                                implicitHeight: 16
                                ProviderMark {
                                    visible: row.item.kind === "session"
                                    anchors.centerIn: parent
                                    provider: visible ? row.item.harness : ""
                                    implicitWidth: 15
                                    implicitHeight: 15
                                    color: Theme.textMuted
                                }
                                FileIcon {
                                    visible: row.item.kind === "file"
                                    anchors.centerIn: parent
                                    path: visible ? row.item.title : ""
                                    width: 16
                                    height: 16
                                }
                                Icon {
                                    visible: row.item.kind === "action" || row.item.kind === "project"
                                    anchors.centerIn: parent
                                    name: row.item.kind === "project" ? "folder" : (row.item.icon || "arrow-right-s")
                                    color: Theme.textMuted
                                    implicitWidth: 15
                                    implicitHeight: 15
                                }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 1
                                Label {
                                    Layout.fillWidth: true
                                    text: row.item.kind === "file" ? row.item.title : root.marked(row.item.title, root.words)
                                    textFormat: row.item.kind === "file" ? Text.PlainText : Text.StyledText
                                    color: Theme.foreground
                                    elide: Text.ElideRight
                                    font.pixelSize: Theme.px(13)
                                    font.family: row.item.kind === "file" ? Theme.mono : Qt.application.font.family
                                }
                                Label {
                                    Layout.fillWidth: true
                                    visible: !!row.item.subtitle
                                    text: row.item.subtitle || ""
                                    color: Theme.textMuted
                                    elide: Text.ElideMiddle
                                    font.pixelSize: Theme.px(11)
                                }
                            }
                            StatusDot { status: row.item.kind === "session" && row.item.status !== "live" ? (row.item.status || "") : "" }
                            // Sessions: when; actions: their keys.
                            Label {
                                visible: row.item.kind === "session"
                                text: row.item.kind === "session" ? Theme.ago(row.item.modified, Date.now()) : ""
                                color: Theme.textMuted
                                font.pixelSize: Theme.px(11)
                            }
                            Row {
                                visible: row.item.kind === "action" && !!row.item.hint
                                spacing: 3
                                Repeater {
                                    model: row.item.kind === "action" && row.item.hint ? row.item.hint.split(/\+(?!$)/) : []
                                    KeyCap { required property string modelData; text: modelData }
                                }
                            }
                        }
                    }
                }
            }
        }
        // Nothing found: say so, and how else to look.
        ColumnLayout {
            objectName: "paletteNone"
            visible: root.items.length === 0
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.topMargin: 4
            Layout.bottomMargin: 6
            spacing: 2
            Label {
                Layout.fillWidth: true
                elide: Text.ElideRight
                text: input.text.startsWith("@") ? (input.text.length > 1 ? "No file in this session's folder matches “" + input.text.substring(1) + "”." : "No files to show.")
                    : input.text.startsWith(">") ? "No action matches “" + input.text.substring(1).trim() + "”."
                    : "No session or action matches “" + input.text.trim() + "”."
                color: Theme.text
                font.pixelSize: Theme.px(12.5)
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                visible: !input.text.startsWith("@")
                text: "To search inside messages, use Search all sessions (" + Keymap.seq("searchAll") + ")."
                color: Theme.textMuted
                font.pixelSize: Theme.px(12)
            }
        }
        Label {
            objectName: "paletteFooter"
            visible: root.items.length > 0
            Layout.leftMargin: 10
            Layout.fillWidth: true
            elide: Text.ElideRight
            text: input.text.startsWith("@") ? "↑↓ to choose · Enter to insert" : "↑↓ to choose · Enter to open · > for actions only · @ for files"
            color: Theme.textMuted
            font.pixelSize: Theme.px(11)
        }
    }
}
