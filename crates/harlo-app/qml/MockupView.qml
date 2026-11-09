import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// A ```mockup block's picture (its HTML file, taken by a browser without a
// window), fitted to the column; with one of how it is now, Before shows
// that instead. Clicking it opens the file in the browser.
Item {
    id: view
    /// Visuals.mockup's.
    property var mock: null
    property bool showBefore: false
    signal openRequested(bool before)
    readonly property var pic: mock ? (showBefore && mock.before ? mock.before : mock.after) : null
    readonly property bool ready: pic !== null && pic.state === "ready"
    implicitHeight: col.implicitHeight + 20

    ColumnLayout {
        id: col
        x: 10
        y: 10
        width: parent.width - 20
        spacing: 6
        Label {
            objectName: "mockupCaption"
            Layout.fillWidth: true
            visible: text !== ""
            text: !view.mock ? "" : [view.mock.title || "", view.mock.before ? (view.showBefore ? "Before" : "After") : ""].filter(t => t !== "").join(" · ")
            color: Theme.textMuted
            font.pixelSize: Theme.px(12)
            elide: Text.ElideRight
        }
        Label {
            objectName: "mockupNote"
            Layout.fillWidth: true
            visible: !view.ready
            text: !view.pic ? "" : view.pic.state === "taking" ? "Taking a picture of it…" : (view.pic.error || "")
            color: view.pic && view.pic.state === "failed" ? Theme.warning : Theme.muted
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
        }
        Rectangle {
            id: frame
            objectName: "mockupPicture"
            visible: view.ready
            readonly property real fit: view.ready ? Math.min(1, col.width / Math.max(1, view.pic.width)) : 1
            Layout.alignment: Qt.AlignHCenter
            Layout.preferredWidth: view.ready ? Math.round(view.pic.width * fit) + 2 : 0
            Layout.preferredHeight: view.ready ? Math.round(view.pic.height * fit) + 2 : 0
            color: "transparent"
            border.width: 1
            border.color: Theme.hairline
            radius: 3
            Image {
                x: 1
                y: 1
                width: parent.width - 2
                height: parent.height - 2
                source: view.ready ? view.pic.url : ""
                fillMode: Image.PreserveAspectFit
                smooth: true
                mipmap: true
                asynchronous: true
                cache: false
            }
            HoverHandler { id: picHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: view.openRequested(view.showBefore && !!view.mock.before) }
            ToolTip.visible: picHover.hovered
            ToolTip.text: "Open in the browser"
            ToolTip.delay: 600
        }
    }
}
