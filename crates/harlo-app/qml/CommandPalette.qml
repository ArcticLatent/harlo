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
    height: Math.min(list.contentHeight + input.height + 28, parent.height * 0.7)
    padding: 8
    modal: true
    focus: true
    background: GlassSurface { radius: 14; kind: "dialog" }
    Overlay.modal: GlassScrim { }

    property var items: []

    // Light fuzzy test for the short action list: letters in order.
    function fuzzy(needle, hay) {
        needle = needle.toLowerCase(); hay = hay.toLowerCase()
        let i = 0
        for (const c of hay) if (c === needle[i]) i++
        return i === needle.length
    }

    function refresh() {
        const q = input.text
        let out = []
        if (q.startsWith("@")) {
            for (const f of root.session.fileMatches)
                out.push({ kind: "file", title: f, hint: "insert", run: () => root.insertFile(f) })
        } else {
            const actionOnly = q.startsWith(">")
            const query = actionOnly ? q.substring(1).trim() : q.trim()
            const acts = root.actions().filter(a => query === "" || fuzzy(query, a.title))
            if (actionOnly || query !== "")
                out = out.concat(acts.slice(0, actionOnly ? 100 : 6).map(a => Object.assign({ kind: "action" }, a)))
            if (!actionOnly) {
                for (const line of Workspace.search(query, 40)) {
                    const p = line.split("\t")
                    if (p[0] === "session") {
                        out.push({ kind: "session", title: p[2], subtitle: p[3], hint: p[4], status: p[6] || "", run: () => Workspace.open(p[1]) })
                    } else if (p[0] === "project") {
                        const id = Number(p[1].split(":")[1])
                        out.push({ kind: "action", title: "New session in " + p[2], subtitle: p[3], hint: p[4], run: () => root.newSessionRequested(id) })
                    }
                }
                if (query === "")
                    out = out.concat(acts.map(a => Object.assign({ kind: "action" }, a)))
            }
        }
        items = out
        list.currentIndex = 0
    }

    function runCurrent() {
        const item = items[list.currentIndex]
        if (!item) return
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
        TextField {
            id: input
            Layout.fillWidth: true
            placeholderText: "Search sessions and projects   ·   > actions   ·   @ files"
            placeholderTextColor: Theme.muted
            color: Theme.foreground
            font.pixelSize: Theme.px(15)
            background: Rectangle { radius: 6; color: Theme.background; border.color: Theme.primary }
            onTextChanged: {
                if (text.startsWith("@")) root.session.searchFiles(text.substring(1))
                root.refresh()
            }
            Keys.onDownPressed: list.incrementCurrentIndex()
            Keys.onUpPressed: list.decrementCurrentIndex()
            Keys.onReturnPressed: root.runCurrent()
            Keys.onEnterPressed: root.runCurrent()
            Keys.onEscapePressed: root.close()
        }
        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.items
            highlightMoveDuration: 0
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            delegate: ItemDelegate {
                id: row
                required property var modelData
                required property int index
                width: ListView.view.width
                height: modelData.subtitle ? 44 : 32
                topPadding: 2
                bottomPadding: 2
                highlighted: ListView.isCurrentItem
                background: Rectangle { radius: 6; color: row.highlighted ? Theme.selected : row.hovered ? Theme.hover : "transparent" }
                contentItem: RowLayout {
                    spacing: 10
                    Label {
                        visible: row.modelData.kind !== "file"
                        text: row.modelData.kind === "session" ? "💬" : "›"
                        color: Theme.muted
                        Layout.preferredWidth: 18
                        horizontalAlignment: Text.AlignHCenter
                    }
                    FileIcon {
                        visible: row.modelData.kind === "file"
                        path: visible ? row.modelData.title : ""
                        Layout.preferredWidth: 18
                        Layout.preferredHeight: 16
                        fillMode: Image.Pad
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 1
                        Label {
                            Layout.fillWidth: true
                            text: row.modelData.title
                            color: Theme.foreground
                            elide: Text.ElideRight
                            font.pixelSize: Theme.px(13)
                            font.family: row.modelData.kind === "file" ? Theme.mono : Qt.application.font.family
                        }
                        Label {
                            Layout.fillWidth: true
                            visible: !!row.modelData.subtitle
                            text: row.modelData.subtitle || ""
                            color: Theme.muted
                            elide: Text.ElideMiddle
                            font.pixelSize: Theme.px(11)
                        }
                    }
                    StatusDot { status: row.modelData.status === "live" ? "" : (row.modelData.status || "") }
                    Label { text: row.modelData.hint || ""; color: Theme.muted; font.pixelSize: Theme.px(11) }
                }
                onClicked: { list.currentIndex = index; root.runCurrent() }
            }
            Label {
                anchors.centerIn: parent
                visible: root.items.length === 0
                text: input.text.startsWith("@") ? "No matching files" : "No matches"
                color: Theme.muted
            }
        }
    }
}
