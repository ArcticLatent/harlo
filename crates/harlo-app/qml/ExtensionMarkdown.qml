import QtQuick
import harlo

// Markdown a pi extension draws, as a reply's is: its prose in the text
// font, its code blocks in wells (`parts`, from ChatSession: `{kind,
// html}`, kind "prose" or "code").
Column {
    id: md
    required property var parts
    spacing: 6
    Accessible.role: Accessible.StaticText
    /// The link at `x`, `y` (this item's), or "".
    function linkAt(x, y) {
        for (let i = 0; i < partList.count; i++) {
            const it = partList.itemAt(i)
            if (!it || it.code) continue
            const p = it.prose.mapFromItem(md, x, y)
            if (p.x >= 0 && p.y >= 0 && p.x < it.prose.width && p.y < it.prose.height) return it.prose.linkAt(p.x, p.y)
        }
        return ""
    }
    Repeater {
        id: partList
        model: md.parts
        Item {
            id: part
            required property var modelData
            readonly property bool code: modelData.kind === "code"
            readonly property alias prose: prose
            width: md.width
            implicitHeight: code ? codeWell.height : prose.implicitHeight
            TextEdit {
                id: prose
                objectName: "extMarkdown"
                visible: !part.code
                width: parent.width
                textFormat: TextEdit.RichText
                text: visible ? Theme.htmlStyle + Theme.proseHtml(part.modelData.html, -1, true) : ""
                font.family: Theme.uiFont
                font.pixelSize: Theme.px(13.5)
                color: Theme.foreground
                readOnly: true
                wrapMode: TextEdit.Wrap
            }
            Rectangle {
                id: codeWell
                objectName: "extMarkdownCode"
                visible: part.code
                width: parent.width
                height: visible ? codeText.implicitHeight + 16 : 0
                radius: 2
                color: Theme.well
                TextEdit {
                    id: codeText
                    x: 10
                    y: 8
                    width: parent.width - 20
                    textFormat: TextEdit.RichText
                    text: codeWell.visible ? Theme.htmlStyle + part.modelData.html : ""
                    font.family: Theme.mono
                    font.pixelSize: Theme.mpx(12)
                    color: Theme.foreground
                    readOnly: true
                    wrapMode: TextEdit.Wrap
                }
            }
        }
    }
}
