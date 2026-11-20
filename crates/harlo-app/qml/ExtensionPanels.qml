pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// A pi extension's views of some kinds (`kinds`, and for widgets
// `placement`), one under the other: its header, its widgets, a screen of
// its own at the message box, its footer. Each stays one item while its
// lines change (an animation sends many), so the keyboard stays with it.
ColumnLayout {
    id: panels
    required property ChatSession session
    /// Every view of the session (`extViewsJson`, parsed).
    required property var views
    property var kinds: []
    /// Widgets only: "aboveEditor" or "belowEditor".
    property string placement: ""
    readonly property int count: shown.count
    spacing: 6
    visible: count > 0

    // The views here, by id (their delegates read their frames from it).
    readonly property var byId: {
        const m = {}
        for (const v of views) {
            if (kinds.indexOf(v.kind) < 0 || v.hidden) continue
            if (v.kind === "widget" && placement !== "" && (v.placement || "aboveEditor") !== placement) continue
            m[v.id] = v
        }
        return m
    }
    onByIdChanged: {
        for (let i = shown.count - 1; i >= 0; i--)
            if (!(shown.get(i).vid in byId)) shown.remove(i)
        const have = []
        for (let i = 0; i < shown.count; i++) have.push(shown.get(i).vid)
        for (const id in byId)
            if (have.indexOf(id) < 0) shown.append({ vid: id })
    }
    ListModel { id: shown }

    Repeater {
        model: shown
        delegate: Item {
            id: slot
            required property string vid
            readonly property var frame: panels.byId[vid] || null
            readonly property bool own: !!frame && frame.kind === "screen"
            Layout.fillWidth: true
            implicitHeight: lines.implicitHeight
            ExtensionView {
                id: lines
                objectName: "extPanel-" + (slot.frame ? slot.frame.kind : "")
                width: parent.width
                session: panels.session
                view: slot.frame
                background: slot.own ? Theme.codeBg : Theme.canvas
                // (A screen that asks for the keys gets them.)
                Component.onCompleted: if (slot.frame && slot.frame.focus) forceActiveFocus()
            }
            // A screen of its own: a way to close it (as Esc does).
            ComposerIconButton {
                objectName: "extPanelClose"
                visible: slot.own
                anchors.top: parent.top
                anchors.right: parent.right
                anchors.margins: 4
                iconName: "close"
                tip: qsTr("Close")
                onClicked: lines.send({ op: "close" })
            }
        }
    }
}
