import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Ctrl+Shift+F: full-text search over every session's messages. Results
// are grouped by session; picking one opens the session at that message,
// its words marked. Empty, it offers the sessions used last.
Popup {
    id: root
    signal picked(string path, int index, string query)

    property var hits: []
    property var recents: []
    property int current: 0
    readonly property bool empty: field.text.trim() === ""
    // What ↑↓ and Enter move through: the hits, or (empty) the recent sessions.
    readonly property int count: empty ? recents.length : hits.length
    // Rows to show: a session header, then its hits; or the recent sessions.
    readonly property var rows: {
        const out = []
        if (empty) {
            for (let i = 0; i < recents.length; i++) out.push({ kind: "recent", item: i, session: recents[i] })
            return out
        }
        let last = ""
        for (let i = 0; i < hits.length; i++) {
            const h = hits[i]
            if (h.path !== last) {
                out.push({ kind: "header", session: h })
                last = h.path
            }
            out.push({ kind: "hit", item: i, role: h.role, harness: h.harness, snippet: h.snippet })
        }
        return out
    }
    readonly property int currentRow: rows.findIndex(r => r.kind !== "header" && r.item === current)

    function openSearch(text) {
        open()
        if (typeof text === "string") field.text = text
        recents = JSON.parse(Workspace.recentSessions(6))
        field.selectAll()
        field.forceActiveFocus()
        run()
    }
    // Searches run off the window's thread: the latest one asked for is
    // shown when it answers (an older answer is dropped), and the index
    // taking in more sessions asks again.
    property int generation: 0
    property string asked: ""
    property string shown: ""
    // Whether the hits shown answer what's typed now.
    readonly property bool settled: empty || shown === field.text
    function run() {
        generation++
        asked = field.text
        if (empty) {
            hits = []
            shown = asked
            current = 0
            return
        }
        Workspace.findMessages(asked, 80, generation)
    }
    Connections {
        target: Workspace
        function onMessagesFound(gen, json) {
            if (gen !== root.generation) return
            const same = root.shown === root.asked
            root.hits = JSON.parse(json)
            root.shown = root.asked
            // (More hits for the same words: the chosen one stays chosen.)
            if (!same || root.current >= root.hits.length) root.current = 0
        }
        function onSearchIndexGrew() {
            if (root.visible && !root.empty) root.run()
        }
    }
    function pick(i) {
        if (empty) {
            const r = recents[i]
            if (!r) return
            close()
            Workspace.open(r.path)
            return
        }
        const h = hits[i]
        if (!h) return
        close()
        root.picked(h.path, h.index, field.text)
    }
    // A session's line under its title: agent · project · when (the full
    // folder on hover).
    function meta(s) {
        const project = (s.cwd || "").split("/").filter(x => x !== "").pop() || ""
        return [Theme.harnessLabel(s.harness), project, Theme.ago(s.modified, Date.now())].filter(x => x).join(" · ")
    }
    // Test hook.
    property alias query: field.text

    x: Math.round((parent.width - width) / 2)
    y: Math.round(parent.height * 0.1)
    width: Math.min(760, parent.width - 40)
    height: Math.min(column.implicitHeight + topPadding + bottomPadding, parent.height * 0.75)
    padding: 8
    modal: true
    focus: true
    background: GlassSurface { radius: 14; kind: "dialog" }
    Overlay.modal: GlassScrim { }

    Timer { id: debounce; interval: 150; onTriggered: root.run() }

    contentItem: ColumnLayout {
        id: column
        spacing: 6
        AppTextField {
            id: field
            objectName: "searchField"
            Layout.fillWidth: true
            leftPadding: 32
            placeholderText: "Search the messages in all sessions…"
            onTextEdited: debounce.restart()
            Icon { x: 10; anchors.verticalCenter: parent.verticalCenter; name: "search"; color: Theme.muted; implicitWidth: 15; implicitHeight: 15 }
            Keys.onDownPressed: root.current = Math.min(root.count - 1, root.current + 1)
            Keys.onUpPressed: root.current = Math.max(0, root.current - 1)
            Keys.onReturnPressed: root.pick(root.current)
            Keys.onEnterPressed: root.pick(root.current)
        }
        // Empty: what it searches, then where you were.
        Label {
            objectName: "searchHint"
            visible: root.empty
            Layout.fillWidth: true
            Layout.leftMargin: 8
            Layout.topMargin: 4
            wrapMode: Text.Wrap
            text: "Finds words in your messages and the agents' replies, in every session." + (root.recents.length > 0 ? " Recent sessions:" : "")
            color: Theme.textMuted
            font.pixelSize: Theme.px(12)
        }
        ColumnLayout {
            objectName: "searchNone"
            visible: !root.empty && root.settled && root.hits.length === 0
            Layout.fillWidth: true
            Layout.leftMargin: 8
            Layout.topMargin: 4
            Layout.bottomMargin: 6
            spacing: 2
            Label {
                Layout.fillWidth: true
                elide: Text.ElideRight
                text: field.text.trim().split(/\s+/).length > 1 ? "No message has all of these words." : "No message has “" + field.text.trim() + "”."
                color: Theme.text
                font.pixelSize: Theme.px(12.5)
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: "Words match from their start: “auth” finds “authorize”. Fewer words find more."
                color: Theme.textMuted
                font.pixelSize: Theme.px(12)
            }
        }
        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredHeight: contentHeight
            visible: count > 0
            clip: true
            model: root.rows
            currentIndex: root.currentRow
            onCurrentIndexChanged: if (currentIndex >= 0) positionViewAtIndex(currentIndex, ListView.Contain)
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            delegate: Loader {
                id: rowLoader
                required property var modelData
                required property int index
                width: ListView.view.width
                sourceComponent: modelData.kind === "header" ? headerRow : modelData.kind === "recent" ? recentRow : hitRow
                Component {
                    id: headerRow
                    RowLayout {
                        spacing: 8
                        Item { implicitWidth: 2 }
                        Label {
                            Layout.topMargin: rowLoader.index > 0 ? 8 : 2
                            text: rowLoader.modelData.session.title
                            color: Theme.foreground
                            font.pixelSize: Theme.px(12)
                            font.bold: true
                            elide: Text.ElideRight
                            Layout.maximumWidth: list.width * 0.6
                        }
                        Label {
                            Layout.topMargin: rowLoader.index > 0 ? 8 : 2
                            Layout.fillWidth: true
                            text: root.meta(rowLoader.modelData.session)
                            color: Theme.muted
                            font.pixelSize: Theme.px(11)
                            elide: Text.ElideRight
                            HoverHandler { id: metaHover }
                            ToolTip.visible: metaHover.hovered && rowLoader.modelData.session.cwd !== ""
                            ToolTip.delay: 500
                            ToolTip.text: rowLoader.modelData.session.cwd
                        }
                    }
                }
                Component {
                    id: hitRow
                    Rectangle {
                        readonly property bool chosen: rowLoader.modelData.item === root.current
                        implicitHeight: snippet.implicitHeight + 12
                        radius: 6
                        color: chosen ? Theme.selected : hitHover.hovered ? Theme.hover : "transparent"
                        HoverHandler { id: hitHover }
                        TapHandler { onTapped: root.pick(rowLoader.modelData.item) }
                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 10
                            anchors.rightMargin: 10
                            spacing: 8
                            Label {
                                Layout.alignment: Qt.AlignTop
                                Layout.topMargin: 6
                                Layout.preferredWidth: 46
                                // Who said it, as the session says it.
                                text: rowLoader.modelData.role === "user" ? "You" : Theme.harnessLabel(rowLoader.modelData.harness) || "Agent"
                                color: rowLoader.modelData.role === "user" ? Theme.primary : Theme.info
                                font.pixelSize: Theme.px(11)
                                font.weight: Font.DemiBold
                                elide: Text.ElideRight
                            }
                            Label {
                                id: snippet
                                Layout.fillWidth: true
                                // Matches in the text colour, the rest muted.
                                text: rowLoader.modelData.snippet.replace(/<b>/g, "<b><font color='" + Theme.rgb(Theme.foreground) + "'>").replace(/<\/b>/g, "</font></b>")
                                textFormat: Text.StyledText
                                color: Theme.muted
                                font.pixelSize: Theme.px(12)
                                wrapMode: Text.Wrap
                                maximumLineCount: 3
                                elide: Text.ElideRight
                            }
                        }
                    }
                }
                Component {
                    id: recentRow
                    Rectangle {
                        readonly property bool chosen: rowLoader.modelData.item === root.current
                        implicitHeight: 32
                        radius: 6
                        color: chosen ? Theme.selected : recentHover.hovered ? Theme.hover : "transparent"
                        HoverHandler { id: recentHover }
                        TapHandler { onTapped: root.pick(rowLoader.modelData.item) }
                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 10
                            anchors.rightMargin: 10
                            spacing: 10
                            ProviderMark { provider: rowLoader.modelData.session.harness; tip: ""; implicitWidth: 14; implicitHeight: 14; color: Theme.textMuted }
                            Label {
                                text: rowLoader.modelData.session.title
                                color: Theme.text
                                font.pixelSize: Theme.px(12.5)
                                elide: Text.ElideRight
                                Layout.maximumWidth: list.width * 0.6
                            }
                            Label {
                                Layout.fillWidth: true
                                text: root.meta(rowLoader.modelData.session)
                                color: Theme.muted
                                font.pixelSize: Theme.px(11)
                                elide: Text.ElideRight
                            }
                        }
                    }
                }
            }
        }
        Label {
            visible: root.count > 0
            text: (root.empty ? "" : root.hits.length + (root.hits.length >= 80 ? "+" : "") + (root.hits.length === 1 ? " match · " : " matches · ")) + "↑↓ to choose · Enter to open"
            color: Theme.muted
            font.pixelSize: Theme.px(11)
            Layout.leftMargin: 6
        }
    }
}
