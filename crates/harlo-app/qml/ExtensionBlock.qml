import QtQuick
import harlo

// One block of a pi extension's view (see ExtensionView): some of its
// lines on the cell grid, its Markdown drawn as a reply's, or its picture.
Item {
    id: block
    /// `{kind, row, rows}` (lines), `{kind, parts, bg, pad}` (markdown) or
    /// `{kind, data, mime, cells, bg, pad}` (image).
    required property var modelData
    /// The view's lines, styled.
    required property var lines
    required property real cellW
    required property real cellH
    /// The grid's colours (Theme.terminalColors).
    required property var colors
    readonly property string kind: modelData.kind
    readonly property real inset: (modelData.pad || 0) * cellW
    /// The link at `x`, `y` (this item's), or "".
    function linkAt(x, y) {
        if (kind !== "markdown") return ""
        const p = markdown.mapFromItem(block, x, y)
        return markdown.linkAt(p.x, p.y)
    }
    implicitHeight: kind === "lines" ? (modelData.rows || 0) * cellH
                  : kind === "markdown" ? markdown.implicitHeight + cellH
                  : picture.height + cellH

    // (A Box's background, as the lines around it have it.)
    Rectangle {
        anchors.fill: parent
        visible: block.kind !== "lines" && !!block.modelData.bg
        color: block.modelData.bg || "transparent"
    }
    ExtensionLines {
        objectName: "extBlockLines"
        visible: block.kind === "lines"
        anchors.fill: parent
        lines: visible ? block.lines.slice(block.modelData.row || 0, (block.modelData.row || 0) + (block.modelData.rows || 0)) : []
        cols: Math.max(1, Math.round(width / block.cellW))
        fontFamily: Theme.mono
        fontPixelSize: Theme.mpx(13)
        padding: 0
        readonly property var colors: block.colors
        onColorsChanged: setPalette(colors)
        Component.onCompleted: setPalette(colors)
    }
    ExtensionMarkdown {
        id: markdown
        visible: block.kind === "markdown"
        x: block.inset
        y: block.cellH / 2
        width: parent.width - 2 * block.inset
        parts: visible ? (block.modelData.parts || []) : []
    }
    Image {
        id: picture
        objectName: "extPicture"
        visible: block.kind === "image"
        x: block.inset
        y: block.cellH / 2
        source: visible && block.modelData.data ? "data:" + block.modelData.mime + ";base64," + block.modelData.data : ""
        // (Its own size at most, and as wide as the extension allows: the
        // picture is in memory whole already, as the frame's text.)
        width: Math.min(parent.width - 2 * block.inset, (block.modelData.cells || 60) * block.cellW, implicitWidth)
        height: implicitWidth > 0 ? width * implicitHeight / implicitWidth : 0
        fillMode: Image.PreserveAspectFit
        asynchronous: true
        Accessible.role: Accessible.Graphic
        Accessible.name: qsTr("A picture")
    }
}
