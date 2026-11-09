import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// A ```mockup block's picture (its HTML file, taken by a browser without a
// window), fitted to the column; with one of how it is now, Before and After
// switch between them. Clicking it opens it full size.
Item {
    id: view
    /// Visuals.mockup's.
    property var mock: null
    property bool showBefore: false
    signal openRequested(bool before)
    signal beforeChosen(bool before)
    readonly property var pic: mock ? (showBefore && mock.before ? mock.before : mock.after) : null
    readonly property bool ready: pic !== null && pic.state === "ready"
    implicitHeight: col.implicitHeight + 20

    ColumnLayout {
        id: col
        x: 10
        y: 10
        width: parent.width - 20
        spacing: 6
        // (Narrow, Before | After goes under the title, which keeps its width.)
        GridLayout {
            Layout.fillWidth: true
            columns: col.width >= 420 ? 2 : 1
            columnSpacing: 8
            rowSpacing: 6
            visible: caption.text !== "" || beforeAfter.visible
            Label {
                id: caption
                objectName: "mockupCaption"
                Layout.fillWidth: true
                text: !view.mock ? "" : view.mock.title || ""
                color: Theme.textMuted
                font.pixelSize: Theme.px(12)
                visible: text !== ""
                wrapMode: Text.Wrap
                maximumLineCount: 2
                elide: Text.ElideRight
            }
            SegmentedControl {
                id: beforeAfter
                objectName: "mockupBeforeAfter"
                visible: !!view.mock && !!view.mock.before
                labels: ["Before", "After"]
                current: view.showBefore ? 0 : 1
                namePrefix: "mockup"
                onPicked: i => view.beforeChosen(i === 0)
            }
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
            border.width: activeFocus ? 2 : 1
            border.color: activeFocus ? Theme.primary : Theme.hairline
            radius: 3
            // (From the keyboard too: Tab to it, Enter or Space.)
            activeFocusOnTab: true
            Keys.onReturnPressed: view.openRequested(view.showBefore && !!view.mock.before)
            Keys.onSpacePressed: view.openRequested(view.showBefore && !!view.mock.before)
            Accessible.role: Accessible.Button
            Accessible.name: (view.mock && view.mock.title ? view.mock.title : "Mockup") + (view.mock && view.mock.before ? (view.showBefore ? ", before" : ", after") : "") + ". Open full size"
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
            ToolTip.text: "Open full size"
            ToolTip.delay: 600
        }
    }
}
