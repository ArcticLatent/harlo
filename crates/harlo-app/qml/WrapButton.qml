import QtQuick
import harlo

// Wrap for one box whose lines are cut at the right (Word wrap off), and
// "No wrap" to undo it; `key` its row, `code` its CodeText.
FlatButton {
    id: wrapButton
    /// The transcript's list (which boxes are wrapped).
    required property Item list
    property string key
    property Item code: null
    readonly property bool on: wrapButton.list.wraps[key] === true
    objectName: "codeWrap"
    visible: !Theme.wrapCode && !!code && code.visible && (code.cut || on)
    text: on ? "No wrap" : "Wrap"
    font.pixelSize: Theme.px(11)
    padding: 2
    onClicked: wrapButton.list.setWrap(key, !on)
    Accessible.name: on ? "Don't wrap this code" : "Wrap this code's long lines"
}
