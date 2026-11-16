import QtQuick
import harlo

// Copy, then "Copied" for a moment.
FlatButton {
    id: button
    /// The text, or (worked out on the click) a function giving it.
    property string copyText
    property var copyFrom: null
    property bool copied: false
    text: copied ? "Copied" : "Copy"
    font.pixelSize: Theme.px(12)
    onClicked: {
        clip.active = true
        clip.item.copyText(button.copyFrom ? button.copyFrom() : button.copyText)
        clip.active = false
        button.copied = true
        copiedTimer.restart()
    }
    Timer { id: copiedTimer; interval: 1200; onTriggered: button.copied = false }
    // (QML has no clipboard API: a hidden TextEdit, made only to copy.)
    Loader {
        id: clip
        active: false
        sourceComponent: TextEdit {
            visible: false
            // Not named `copy`: that would shadow TextEdit.copy() and recurse.
            function copyText(t) { text = t; selectAll(); copy(); deselect(); text = "" }
        }
    }
}
