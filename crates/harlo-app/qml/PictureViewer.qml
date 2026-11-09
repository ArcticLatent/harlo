import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// A reply's picture (a mockup, a diagram) at full size inside harlo: fitted
// to the window at first, 100% a click away, Ctrl+wheel or + and − to zoom,
// dragged to look around. A mockup with one of how it is now switches
// between Before and After here too. Esc closes it.
Popup {
    id: viewer
    objectName: "pictureViewer"
    property string title
    /// [{label, url, width, height}], in the order they're offered.
    property var pictures: []
    property int current: 0
    /// What the picture is ("Mockup", "Diagram"), for its name when read out.
    property string kind: ""
    /// How far fitting may enlarge it (a diagram is drawn at twice its
    /// size, so 2 stays sharp; a mockup's picture is taken at 1).
    property real maxFit: 1
    /// Opens picture i in the browser (set by show()).
    property var openWith: null

    readonly property var pic: pictures.length > 0 ? pictures[Math.min(current, pictures.length - 1)] : null
    readonly property real fit: !pic ? 1 : Math.min(maxFit, (flick.width - 32) / Math.max(1, pic.width), (flick.height - 32) / Math.max(1, pic.height))
    property real zoom: 1
    property bool fitted: true
    readonly property real scaleNow: fitted ? fit : zoom

    function show(title, pictures, current, kind, openWith, maxFit) {
        viewer.maxFit = maxFit || 1
        viewer.title = title
        viewer.openWith = openWith || null
        viewer.pictures = pictures
        viewer.current = current
        viewer.kind = kind || ""
        fitted = true
        open()
        body.forceActiveFocus()
    }
    function zoomTo(z) {
        const before = scaleNow
        zoom = Math.max(0.1, Math.min(4, z))
        fitted = false
        // Keep the middle of what's in view in the middle.
        const cx = (flick.contentX + flick.width / 2) / Math.max(0.01, before)
        const cy = (flick.contentY + flick.height / 2) / Math.max(0.01, before)
        Qt.callLater(() => {
            flick.contentX = Math.max(0, Math.min(flick.contentWidth - flick.width, cx * zoom - flick.width / 2))
            flick.contentY = Math.max(0, Math.min(flick.contentHeight - flick.height, cy * zoom - flick.height / 2))
        })
    }

    parent: Overlay.overlay
    x: Math.round((parent.width - width) / 2)
    y: Math.round((parent.height - height) / 2)
    width: Math.max(360, parent.width - 48)
    height: Math.max(300, parent.height - 48)
    padding: 0
    modal: true
    focus: true
    background: GlassSurface { radius: 14; kind: "dialog" }
    Overlay.modal: GlassScrim { }

    contentItem: Item {
        id: body
        focus: true
        Keys.onPressed: event => {
            if (event.key === Qt.Key_Plus || event.key === Qt.Key_Equal) { viewer.zoomTo(viewer.scaleNow * 1.25); event.accepted = true }
            else if (event.key === Qt.Key_Minus) { viewer.zoomTo(viewer.scaleNow / 1.25); event.accepted = true }
            else if (event.key === Qt.Key_0) { viewer.fitted = true; event.accepted = true }
            else if (event.key === Qt.Key_1) { viewer.zoomTo(1); event.accepted = true }
            else if (viewer.pictures.length > 1 && (event.key === Qt.Key_Left || event.key === Qt.Key_Right)) {
                viewer.current = (viewer.current + (event.key === Qt.Key_Right ? 1 : viewer.pictures.length - 1)) % viewer.pictures.length
                event.accepted = true
            }
        }

        RowLayout {
            id: head
            x: 16
            width: parent.width - 28
            height: 52
            spacing: 8
            Label {
                Layout.fillWidth: true
                text: viewer.title
                color: Theme.foreground
                font.pixelSize: Theme.px(14.5)
                font.weight: Font.DemiBold
                elide: Text.ElideRight
            }
            SegmentedControl {
                visible: viewer.pictures.length > 1
                labels: viewer.pictures.map(p => p.label)
                current: viewer.current
                namePrefix: "viewerPicture"
                onPicked: i => viewer.current = i
            }
            FlatButton {
                objectName: "viewerZoom"
                // (Nothing to switch when fitting shows it at its own size.)
                visible: !(viewer.fitted && Math.abs(viewer.fit - 1) < 0.01)
                text: viewer.fitted ? "100%" : "Fit"
                padding: 4
                font.pixelSize: Theme.px(12)
                onClicked: viewer.fitted ? viewer.zoomTo(1) : viewer.fitted = true
                ToolTip.visible: hovered
                ToolTip.text: viewer.fitted ? "Show it at its own size (1)" : "Fit it to the window (0)"
                ToolTip.delay: 600
            }
            FlatButton {
                text: "Open in browser"
                padding: 4
                font.pixelSize: Theme.px(12)
                visible: viewer.openWith !== null
                onClicked: viewer.openWith(viewer.current)
            }
            PanelButton { round: true; iconName: "close"; tip: "Close (Esc)"; onClicked: viewer.close() }
        }
        Rectangle { anchors.top: head.bottom; width: parent.width; height: 1; color: Theme.hairline }

        Flickable {
            id: flick
            anchors.top: head.bottom
            anchors.topMargin: 1
            anchors.bottom: parent.bottom
            width: parent.width
            clip: true
            contentWidth: Math.max(width, frame.width + 32)
            contentHeight: Math.max(height, frame.height + 32)
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            ScrollBar.horizontal: ScrollBar { }

            Rectangle {
                id: frame
                objectName: "viewerPictureFrame"
                x: Math.round((flick.contentWidth - width) / 2)
                y: Math.round((flick.contentHeight - height) / 2)
                width: viewer.pic ? Math.round(viewer.pic.width * viewer.scaleNow) + 2 : 0
                height: viewer.pic ? Math.round(viewer.pic.height * viewer.scaleNow) + 2 : 0
                color: "transparent"
                border.width: 1
                border.color: Theme.hairline
                Image {
                    x: 1
                    y: 1
                    width: parent.width - 2
                    height: parent.height - 2
                    source: viewer.pic ? viewer.pic.url : ""
                    fillMode: Image.PreserveAspectFit
                    smooth: true
                    mipmap: true
                    asynchronous: true
                    cache: false
                    Accessible.role: Accessible.Graphic
                    Accessible.name: [viewer.kind, viewer.title, viewer.pictures.length > 1 && viewer.pic ? viewer.pic.label : ""].filter(t => t !== "").join(", ")
                }
            }
            TapHandler {
                onDoubleTapped: viewer.fitted ? viewer.zoomTo(1) : viewer.fitted = true
            }
            WheelHandler {
                acceptedModifiers: Qt.ControlModifier
                onWheel: event => viewer.zoomTo(viewer.scaleNow * (event.angleDelta.y > 0 ? 1.15 : 1 / 1.15))
            }
            PinchHandler {
                target: null
                property real start: 1
                onActiveChanged: if (active) start = viewer.scaleNow
                onActiveScaleChanged: if (active) viewer.zoomTo(start * activeScale)
            }
        }
    }
}
