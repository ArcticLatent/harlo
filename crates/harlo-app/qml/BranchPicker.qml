import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Branch switcher for the Changes panel: search, local
// branches (most recent first), remote branches without a local one, and a
// new branch from HEAD named by what you typed. ↑ ↓ move, Enter picks.
Popup {
    id: pop
    required property GitPanel git
    /// `remote`: an `origin/x` name; `create`: a new branch from HEAD.
    signal chosen(string name, bool remote, bool create)

    padding: 6
    // Stays inside the window: near its right edge it slides left.
    margins: 8
    width: Math.min(320, (Overlay.overlay ? Overlay.overlay.width : 336) - 16)
    height: Math.min(460, search.implicitHeight + list.contentHeight + 22)
    focus: true

    // (Read off the UI thread: over SSH it can take a moment; the last
    // list shows meanwhile.)
    readonly property var info: { try { return git.branchesList !== "" ? JSON.parse(git.branchesList) : { current: "", local: [], remote: [] } } catch (e) { return { current: "", local: [], remote: [] } } }
    property string query: ""
    onAboutToShow: {
        git.loadBranches()
        search.text = ""
        query = ""
        Qt.callLater(() => { search.forceActiveFocus(); list.currentIndex = list.firstPick() })
    }

    function ago(ts) {
        if (!ts) return ""
        const s = Date.now() / 1000 - ts
        return s < 3600 ? Math.max(1, Math.round(s / 60)) + "m" : s < 86400 ? Math.round(s / 3600) + "h"
             : s < 86400 * 30 ? Math.round(s / 86400) + "d" : s < 86400 * 365 ? Math.round(s / (86400 * 30)) + "mo" : Math.round(s / (86400 * 365)) + "y"
    }
    readonly property var items: {
        const q = query.trim().toLowerCase()
        const match = b => q === "" || b.name.toLowerCase().includes(q)
        const local = (info.local || []).filter(match)
        const remote = (info.remote || []).filter(match)
        const out = []
        const name = query.trim()
        if (name !== "" && !(info.local || []).some(b => b.name === name))
            out.push({ kind: "create", name: name })
        if (local.length > 0) out.push({ kind: "header", name: "Local branches" })
        for (const b of local) out.push({ kind: "branch", name: b.name, ahead: b.ahead, date: b.date, remote: false, current: b.name === info.current })
        if (remote.length > 0) out.push({ kind: "header", name: "Remote branches" })
        for (const b of remote) out.push({ kind: "branch", name: b.name, ahead: 0, date: b.date, remote: true, current: false })
        return out
    }
    onItemsChanged: Qt.callLater(() => list.currentIndex = list.firstPick())

    function pick(i) {
        const it = items[i]
        if (!it || it.kind === "header" || it.current) return
        close()
        chosen(it.name, it.remote === true, it.kind === "create")
    }

    background: GlassSurface { radius: 10 }
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 90 }
        NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: 90; easing.type: Easing.OutCubic }
    }
    exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 70 } }

    ColumnLayout {
        anchors.fill: parent
        spacing: 6
        AppTextField {
            id: search
            Layout.fillWidth: true
            placeholderText: "Search or name a new branch…"
            onTextChanged: pop.query = text
            Keys.onDownPressed: list.step(1)
            Keys.onUpPressed: list.step(-1)
            Keys.onReturnPressed: pop.pick(list.currentIndex)
            Keys.onEnterPressed: pop.pick(list.currentIndex)
            Keys.onEscapePressed: pop.close()
        }
        Label {
            Layout.fillWidth: true
            Layout.margins: 6
            visible: pop.info.error !== undefined
            text: pop.info.error || ""
            color: Theme.error
            wrapMode: Text.Wrap
            font.pixelSize: Theme.px(12)
        }
        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: pop.items
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            function pickable(i) { const it = pop.items[i]; return it && it.kind !== "header" && !it.current }
            function firstPick() {
                for (let i = 0; i < pop.items.length; i++) if (pickable(i)) return i
                return -1
            }
            function step(d) {
                let i = currentIndex
                for (let n = 0; n < pop.items.length; n++) {
                    i += d
                    if (i < 0 || i >= pop.items.length) return
                    if (pickable(i)) { currentIndex = i; positionViewAtIndex(i, ListView.Contain); return }
                }
            }
            delegate: Item {
                id: entry
                required property var modelData
                required property int index
                readonly property bool header: modelData.kind === "header"
                width: list.width
                height: header ? 26 : 32
                Label {
                    visible: entry.header
                    x: 8
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 4
                    text: entry.modelData.name
                    color: Theme.muted
                    font.pixelSize: Theme.px(11)
                }
                Rectangle {
                    visible: !entry.header
                    anchors.fill: parent
                    radius: 6
                    color: (list.currentIndex === entry.index || hover.hovered) && !entry.modelData.current ? Theme.hover : "transparent"
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 8
                        anchors.rightMargin: 8
                        spacing: 8
                        Icon {
                            name: entry.modelData.kind === "create" ? "add" : "git-branch"
                            color: entry.modelData.current || entry.modelData.kind === "create" ? Theme.primary : Theme.muted
                            implicitWidth: 14
                            implicitHeight: 14
                        }
                        Label {
                            Layout.fillWidth: true
                            text: entry.modelData.kind === "create" ? "Create branch “" + entry.modelData.name + "”" : entry.modelData.name
                            color: Theme.foreground
                            font.pixelSize: Theme.px(13)
                            elide: Text.ElideMiddle
                        }
                        Label {
                            visible: (entry.modelData.ahead || 0) > 0
                            text: "↑" + entry.modelData.ahead
                            color: Theme.warning
                            font.pixelSize: Theme.px(11)
                            ToolTip.visible: aheadHover.hovered
                            ToolTip.text: entry.modelData.ahead === 1 ? "1 commit not pushed" : entry.modelData.ahead + " commits not pushed"
                            HoverHandler { id: aheadHover }
                        }
                        Label {
                            visible: !entry.modelData.current && entry.modelData.kind === "branch"
                            text: pop.ago(entry.modelData.date)
                            color: Theme.muted
                            font.pixelSize: Theme.px(11)
                        }
                        Label {
                            visible: entry.modelData.current === true
                            text: "current"
                            color: Theme.primary
                            font.pixelSize: Theme.px(11)
                        }
                    }
                    HoverHandler { id: hover; cursorShape: entry.modelData.current ? Qt.ArrowCursor : Qt.PointingHandCursor }
                    TapHandler { onTapped: pop.pick(entry.index) }
                }
            }
        }
        Label {
            Layout.fillWidth: true
            Layout.margins: 6
            visible: pop.items.length === 0 && pop.info.error === undefined
            text: "No branches found."
            color: Theme.muted
            font.pixelSize: Theme.px(12)
        }
    }
}
