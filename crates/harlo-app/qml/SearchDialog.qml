import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Ctrl+Shift+F: full-text search over every session's messages. Results
// are grouped by session; picking one opens the session at that message.
Popup {
    id: root
    signal picked(string path, int index)

    property var hits: []
    property int current: 0
    // Rows to show: a session header, then its hits.
    readonly property var rows: {
        const out = []
        let last = ""
        for (let i = 0; i < hits.length; i++) {
            const h = hits[i]
            if (h.path !== last) {
                out.push({ header: true, title: h.title || h.path.split("/").pop(), harness: h.harness, cwd: h.cwd, modified: h.modified })
                last = h.path
            }
            out.push({ header: false, hit: i, role: h.role, snippet: h.snippet })
        }
        return out
    }
    readonly property int hitRow: rows.findIndex(r => !r.header && r.hit === current)

    function openSearch() {
        open()
        field.selectAll()
        field.forceActiveFocus()
        run()
    }
    function run() {
        hits = field.text.trim() === "" ? [] : JSON.parse(Workspace.searchMessages(field.text, 80))
        current = 0
    }
    function pick(i) {
        const h = hits[i]
        if (!h) return
        close()
        root.picked(h.path, h.index)
    }
    // Test hook.
    property alias query: field.text

    x: Math.round((parent.width - width) / 2)
    y: Math.round(parent.height * 0.1)
    width: Math.min(760, parent.width - 40)
    height: Math.min(Math.max(120, list.contentHeight + field.height + 40), parent.height * 0.75)
    padding: 8
    modal: true
    focus: true
    background: GlassSurface { radius: 14; kind: "dialog" }
    Overlay.modal: GlassScrim { }

    Timer { id: debounce; interval: 150; onTriggered: root.run() }

    contentItem: ColumnLayout {
        spacing: 6
        AppTextField {
            id: field
            Layout.fillWidth: true
            leftPadding: 32
            placeholderText: "Search all sessions…"
            onTextEdited: debounce.restart()
            Icon { x: 10; anchors.verticalCenter: parent.verticalCenter; name: "search"; color: Theme.muted; implicitWidth: 15; implicitHeight: 15 }
            Keys.onDownPressed: root.current = Math.min(root.hits.length - 1, root.current + 1)
            Keys.onUpPressed: root.current = Math.max(0, root.current - 1)
            Keys.onReturnPressed: root.pick(root.current)
            Keys.onEnterPressed: root.pick(root.current)
        }
        Label {
            visible: field.text.trim() !== "" && root.hits.length === 0
            text: "No messages match."
            color: Theme.muted
            font.pixelSize: Theme.px(12)
            Layout.leftMargin: 6
        }
        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.rows
            currentIndex: root.hitRow
            onCurrentIndexChanged: if (currentIndex >= 0) positionViewAtIndex(currentIndex, ListView.Contain)
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            delegate: Loader {
                id: rowLoader
                required property var modelData
                required property int index
                width: ListView.view.width
                sourceComponent: modelData.header ? headerRow : hitRow
                Component {
                    id: headerRow
                    RowLayout {
                        spacing: 8
                        Item { implicitWidth: 2 }
                        Label {
                            Layout.topMargin: rowLoader.index > 0 ? 8 : 2
                            text: rowLoader.modelData.title
                            color: Theme.foreground
                            font.pixelSize: Theme.px(12)
                            font.bold: true
                            elide: Text.ElideRight
                            Layout.maximumWidth: list.width * 0.6
                        }
                        Label {
                            Layout.topMargin: rowLoader.index > 0 ? 8 : 2
                            Layout.fillWidth: true
                            text: [Theme.harnessLabel(rowLoader.modelData.harness), rowLoader.modelData.cwd, Theme.ago(rowLoader.modelData.modified, Date.now())].filter(x => x).join(" · ")
                            color: Theme.muted
                            font.pixelSize: Theme.px(11)
                            elide: Text.ElideRight
                        }
                    }
                }
                Component {
                    id: hitRow
                    Rectangle {
                        readonly property bool chosen: rowLoader.modelData.hit === root.current
                        implicitHeight: snippet.implicitHeight + 12
                        radius: 6
                        color: chosen ? Theme.selected : hitHover.hovered ? Theme.hover : "transparent"
                        HoverHandler { id: hitHover }
                        TapHandler { onTapped: root.pick(rowLoader.modelData.hit) }
                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 10
                            anchors.rightMargin: 10
                            spacing: 8
                            Label {
                                Layout.alignment: Qt.AlignTop
                                Layout.topMargin: 6
                                text: rowLoader.modelData.role === "user" ? "you" : "agent"
                                color: rowLoader.modelData.role === "user" ? Theme.primary : Theme.info
                                font.pixelSize: Theme.px(10)
                                font.bold: true
                                Layout.preferredWidth: 34
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
            }
        }
        Label {
            visible: root.hits.length > 0
            text: root.hits.length + (root.hits.length >= 80 ? "+" : "") + " matches · ↑↓ to choose · Enter to open"
            color: Theme.muted
            font.pixelSize: Theme.px(11)
            Layout.leftMargin: 6
        }
    }
}
