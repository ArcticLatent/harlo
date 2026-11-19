import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

// A declaration the Wiki panel's search found in the code: its kind, name,
// place and signature. Clicked, it opens its card from the code index
// (what it calls, what calls it), with Open to read it in the editor.
ItemDelegate {
    id: codeHit
    objectName: "wikiCodeHit"
    /// `{ path, name, kind, line, end, signature, doc }`.
    property var hit: ({})
    property bool open: false
    /// The card's text (undefined until it's read).
    property var card
    signal cardToggled()
    signal openFile()

    leftInset: 8
    rightInset: 8
    leftPadding: 18
    rightPadding: 14
    topPadding: 7
    bottomPadding: 7
    hoverEnabled: true
    Accessible.name: hit.kind + " " + hit.name
    background: Rectangle { radius: 8; color: codeHit.hovered || codeHit.activeFocus || codeHit.open ? Theme.rowHover : "transparent" }
    contentItem: ColumnLayout {
        spacing: 2
        PlainLabel {
            Layout.fillWidth: true
            text: codeHit.hit.kind + " " + codeHit.hit.name
            color: Theme.text
            font.family: Theme.mono
            font.pixelSize: Theme.mpx(12.5)
            elide: Text.ElideRight
        }
        PlainLabel {
            Layout.fillWidth: true
            text: codeHit.hit.path + ":" + codeHit.hit.line
            color: Theme.textFaint
            font.pixelSize: Theme.px(11.5)
            elide: Text.ElideMiddle
        }
        PlainLabel {
            Layout.fillWidth: true
            visible: !codeHit.open && (codeHit.hit.doc || "") !== ""
            text: codeHit.hit.doc || ""
            color: Theme.textMuted
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
            maximumLineCount: 2
            elide: Text.ElideRight
        }
        // The card: the index's answer, as it gives it.
        PlainLabel {
            objectName: "wikiCodeCard"
            Layout.fillWidth: true
            Layout.topMargin: 6
            visible: codeHit.open
            text: codeHit.card === undefined ? "Looking it up…" : codeHit.card
            color: codeHit.card === undefined ? Theme.textFaint : Theme.textMuted
            font.family: Theme.mono
            font.pixelSize: Theme.mpx(11.5)
            wrapMode: Text.Wrap
        }
        FlatButton {
            objectName: "wikiCodeOpen"
            Layout.topMargin: 4
            visible: codeHit.open
            text: "Open " + codeHit.hit.path.split("/").pop()
            font.pixelSize: Theme.px(12)
            onClicked: codeHit.openFile()
        }
    }
    onClicked: cardToggled()
}
