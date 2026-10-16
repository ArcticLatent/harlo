import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// omp side questions (`/btw`): ask about the session without interrupting
// the agent; answers stream in and never enter the transcript.
Popup {
    id: root
    property ChatSession session
    readonly property var records: session && session.btwJson !== "" ? JSON.parse(session.btwJson) : []
    readonly property bool running: records.some(r => latest(r).status === "running")
    property string topic: ""          // record id to follow up on

    function latest(r) { return r.followUps && r.followUps.length ? r.followUps[r.followUps.length - 1] : r }
    function turns(r) { return [r].concat(r.followUps || []) }
    function ask() {
        if (field.text.trim() === "") return
        session.askBtw(field.text.trim(), topic)
        field.text = ""
    }

    width: 520
    height: Math.min(560, Math.max(180, content.implicitHeight + 24))
    padding: 12
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
    background: GlassSurface { radius: 10 }
    onOpened: field.forceActiveFocus()

    contentItem: ColumnLayout {
        id: content
        spacing: 8
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Icon { name: "chat-thread"; color: Theme.primary; implicitWidth: 15; implicitHeight: 15 }
            Label { text: "Side question"; color: Theme.foreground; font.pixelSize: Theme.px(13); font.bold: true }
            Label { Layout.fillWidth: true; text: "answered from the session's context; not added to it"; color: Theme.muted; font.pixelSize: Theme.px(11); elide: Text.ElideRight }
            FlatButton { visible: root.running; text: "Cancel"; accent: Theme.error; font.pixelSize: Theme.px(11); padding: 3; onClicked: root.session.cancelBtw() }
        }

        Flickable {
            id: flick
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredHeight: Math.min(380, list.implicitHeight)
            visible: root.records.length > 0
            contentHeight: list.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            onContentHeightChanged: contentY = Math.max(0, contentHeight - height)
            ColumnLayout {
                id: list
                width: flick.width - 10
                spacing: 10
                Repeater {
                    model: root.records
                    ColumnLayout {
                        id: rec
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 4
                        Repeater {
                            model: root.turns(rec.modelData)
                            ColumnLayout {
                                required property var modelData
                                Layout.fillWidth: true
                                spacing: 2
                                Label { Layout.fillWidth: true; text: modelData.question; color: Theme.foreground; font.pixelSize: Theme.px(13); font.bold: true; wrapMode: Text.Wrap }
                                TextEdit {
                                    Layout.fillWidth: true
                                    text: modelData.answer !== "" ? modelData.answer : (modelData.status === "running" ? "…" : "(" + modelData.status + ")")
                                    color: modelData.status === "error" ? Theme.error : Theme.foreground
                                    font.family: Theme.uiFont
                                    font.pixelSize: Theme.px(13)
                                    wrapMode: TextEdit.Wrap
                                    readOnly: true
                                    selectByMouse: true
                                    selectionColor: Theme.selection
                                }
                            }
                        }
                        FlatButton {
                            text: root.topic === rec.modelData.id ? "Following up ✓" : "Follow up"
                            outlined: false
                            accent: root.topic === rec.modelData.id ? Theme.primary : Theme.muted
                            font.pixelSize: Theme.px(11)
                            padding: 2
                            onClicked: { root.topic = root.topic === rec.modelData.id ? "" : rec.modelData.id; field.forceActiveFocus() }
                        }
                    }
                }
            }
        }

        AppTextField {
            id: field
            Layout.fillWidth: true
            placeholderText: root.topic !== "" ? "Follow-up question…" : "Ask a side question… (the agent keeps working)"
            onAccepted: root.ask()
        }
    }
}
