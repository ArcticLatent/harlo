import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// The image viewer: one image on the GPU (ImageView), zoomed at the
// pointer, dragged, turned, an animation played; or its images as a grid
// of thumbnails. In the file tab it steps through the image's folder (by
// swapping the tab's file) and F fills the window; over the chat
// (`pictureList`) it steps through the conversation's pictures.
Item {
    id: root
    objectName: "imageViewer"
    // (Narrow: the bar's ⋯ menu.)
    readonly property alias moreMenu: moreMenu
    /// The file tab's path (ssh:// for a remote one) and the local file shown.
    property string path
    property string source
    property bool remote: false
    /// The file was deleted on disk.
    property bool gone: false
    /// The folder's images as a grid.
    property bool gridMode: false
    /// Fills the window.
    property bool filled: false
    readonly property bool fillNow: filled && root.visible
    readonly property alias view: view
    readonly property alias grid: grid
    /// Its own pictures to step through (the chat's), instead of a folder's.
    property var pictureList: null
    readonly property bool ownList: pictureList !== null
    /// The chat's pictures in words (where each came from, by path).
    property var labels: ({})
    function labelOf(p) { return labels[p] || p.substring(p.lastIndexOf("/") + 1) }
    /// Behind the image (over the chat: its own layer, set apart from it).
    property color backdrop: Theme.codeBg
    /// Over the chat: its file to the file tab, or close.
    signal openInTab(string path)
    signal closeRequested()

    // The folder's images (listed again when the folder changes or the grid opens).
    readonly property string folder: ownList || remote || path === "" ? "" : path.substring(0, path.lastIndexOf("/"))
    property var images: []
    property string listedFolder: ""
    readonly property int index: images.indexOf(path)
    function relist() {
        // (`pictureList` itself: `ownList` may not have caught up yet.)
        images = pictureList !== null ? pictureList : folder === "" ? [] : JSON.parse(Images.list(folder))
        listedFolder = folder
    }
    onFolderChanged: if (folder !== listedFolder) relist()
    onPictureListChanged: relist()
    Component.onCompleted: relist()

    function openImage(p) {
        if (!p || p === path) return
        if (ownList) { path = p; source = p }
        else Workspace.openFileTab(p)
    }
    function step(delta) {
        if (index < 0) relist()
        const n = images.length
        if (n < 2) return
        openImage(images[((index < 0 ? 0 : index) + delta + n) % n])
    }
    function showGrid(on) {
        if (on && folder === "" && !ownList) return
        if (on) {
            relist()
            gridMode = true
            grid.currentIndex = Math.max(0, index)
            grid.positionViewAtIndex(grid.currentIndex, GridView.Center)
            grid.forceActiveFocus()
        } else {
            gridMode = false
            view.forceActiveFocus()
        }
    }
    function focusView() { (gridMode ? grid : view).forceActiveFocus() }
    // The next and previous images decoded ahead, for stepping.
    function prefetchAround() {
        const n = images.length
        if (n < 2 || index < 0) return
        const ahead = [images[(index + 1) % n]]
        if (n > 2) ahead.push(images[(index - 1 + n) % n])
        view.prefetch(JSON.stringify(ahead))
    }
    onPathChanged: if (path === "") filled = false

    function singleKey(event) {
        const k = event.key
        const mods = event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)
        const shift = (event.modifiers & Qt.ShiftModifier) !== 0
        let done = true
        if (k === Qt.Key_Plus || k === Qt.Key_Equal) view.zoomIn()
        else if (k === Qt.Key_Minus || k === Qt.Key_Underscore) view.zoomOut()
        else if (mods !== 0) done = false
        else if (k === Qt.Key_Right || k === Qt.Key_J || k === Qt.Key_Space || k === Qt.Key_PageDown) root.step(1)
        else if (k === Qt.Key_Left || k === Qt.Key_K || k === Qt.Key_Backspace || k === Qt.Key_PageUp) root.step(-1)
        else if (k === Qt.Key_Home && images.length > 0) root.openImage(images[0])
        else if (k === Qt.Key_End && images.length > 0) root.openImage(images[images.length - 1])
        else if (k === Qt.Key_Up) view.pan(0, view.height * 0.15)
        else if (k === Qt.Key_Down) view.pan(0, -view.height * 0.15)
        else if (k === Qt.Key_1) view.actualSize()
        else if (k === Qt.Key_0 || k === Qt.Key_W) view.fitView()
        else if (k === Qt.Key_R) view.rotate(!shift)
        else if (k === Qt.Key_G) root.showGrid(true)
        else if (k === Qt.Key_F && !ownList) root.filled = !root.filled
        else if (k === Qt.Key_T && ownList) root.openInTab(root.path)
        else if (k === Qt.Key_P && view.frameCount > 1) view.playing = !view.playing
        else if ((k === Qt.Key_Period || k === Qt.Key_Comma) && view.frameCount > 1) view.stepFrame(k === Qt.Key_Period)
        else if (k === Qt.Key_Escape && root.filled) root.filled = false
        else if (k === Qt.Key_Escape && ownList) root.closeRequested()
        else done = false
        event.accepted = done
    }
    function gridKey(event) {
        const k = event.key
        const mods = event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)
        let done = true
        if (k === Qt.Key_Plus || k === Qt.Key_Equal) root.cell = Math.min(384, root.cell + 32)
        else if (k === Qt.Key_Minus) root.cell = Math.max(96, root.cell - 32)
        else if (mods !== 0) done = false
        else if (k === Qt.Key_H) grid.moveCurrentIndexLeft()
        else if (k === Qt.Key_L) grid.moveCurrentIndexRight()
        else if (k === Qt.Key_K) grid.moveCurrentIndexUp()
        else if (k === Qt.Key_J) grid.moveCurrentIndexDown()
        else if (k === Qt.Key_Home) grid.currentIndex = 0
        else if (k === Qt.Key_End) grid.currentIndex = grid.count - 1
        else if (k === Qt.Key_Return || k === Qt.Key_Enter) root.openFromGrid(grid.currentIndex)
        else if (k === Qt.Key_Escape || k === Qt.Key_G) root.showGrid(false)
        else if (k === Qt.Key_F && !ownList) root.filled = !root.filled
        else done = false
        event.accepted = done
    }
    function openFromGrid(i) {
        const p = images[i]
        if (!p) return
        root.gridMode = false
        if (p === path) view.forceActiveFocus()
        else root.openImage(p)
    }

    /// The grid's cells (logical pixels, before the name).
    property int cell: 168
    // The grid's cells' shape (height over width): its pictures' most
    // common one, read when the grid opens.
    property real gridShape: 1
    onGridModeChanged: if (gridMode) gridShape = Images.commonShape(JSON.stringify(images))

    // The checkerboard under transparency, from the theme.
    readonly property color checkDark: Theme.codeBg
    readonly property color checkLight: Qt.tint(Theme.codeBg, Qt.rgba(Theme.foreground.r, Theme.foreground.g, Theme.foreground.b, 0.07))
    function applyBackdrop() { view.setBackdrop(checkLight.toString(), checkDark.toString()) }
    onCheckDarkChanged: applyBackdrop()
    onCheckLightChanged: applyBackdrop()

    Connections {
        target: view
        function onStatusChanged() { if (view.status === "ready") root.prefetchAround() }
    }

    Item {
        id: stage
        parent: root.fillNow ? Overlay.overlay : root
        anchors.fill: parent
        z: root.fillNow ? 1000 : 0

        Rectangle { anchors.fill: parent; color: root.backdrop }

        ImageView {
            id: view
            objectName: "imageView"
            anchors { left: parent.left; right: parent.right; top: parent.top; bottom: bar.top }
            visible: !root.gridMode
            path: root.source
            focus: true
            activeFocusOnTab: true
            Accessible.role: Accessible.Graphic
            Accessible.name: root.path.substring(root.path.lastIndexOf("/") + 1)
            Keys.onPressed: event => root.singleKey(event)
            Component.onCompleted: root.applyBackdrop()

            Label {
                anchors.centerIn: parent
                visible: view.status === "error" && !root.gone
                text: "Could not load this image."
                color: Theme.error
            }
            Label {
                id: loadingLabel
                anchors.centerIn: parent
                visible: false
                text: "Loading…"
                color: Theme.muted
                font.pixelSize: Theme.px(12)
            }
            // (Only a slow one says so.)
            Timer {
                interval: 200
                running: view.status === "loading"
                onRunningChanged: if (!running) loadingLabel.visible = false
                onTriggered: loadingLabel.visible = view.status === "loading"
            }
        }

        GridView {
            id: grid
            objectName: "imageGrid"
            anchors { left: parent.left; right: parent.right; top: parent.top; bottom: bar.top; leftMargin: 8; topMargin: 8 }
            visible: root.gridMode
            clip: true
            model: root.gridMode ? root.images : []
            cellWidth: Math.max(96, Math.floor((width - 8) / Math.max(1, Math.round((width - 8) / root.cell))))
            // (Shaped like most of its pictures: wide ones don't leave a
            // blank band over each row.)
            cellHeight: Math.round((cellWidth - 20) * root.gridShape) + 42
            keyNavigationEnabled: true
            highlightFollowsCurrentItem: false
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            Keys.onPressed: event => root.gridKey(event)
            WheelHandler {
                acceptedModifiers: Qt.ControlModifier
                onWheel: event => {
                    root.cell = Math.max(96, Math.min(384, root.cell + (event.angleDelta.y > 0 ? 24 : -24)))
                }
            }
            delegate: Item {
                id: cellItem
                required property int index
                required property string modelData
                readonly property bool current: GridView.isCurrentItem
                width: grid.cellWidth
                height: grid.cellHeight
                Rectangle {
                    anchors.fill: parent
                    anchors.rightMargin: 8
                    anchors.bottomMargin: 8
                    radius: 8
                    color: cellItem.current ? Theme.selected : cellHover.hovered ? Theme.hover : "transparent"
                    border.width: cellItem.current && grid.activeFocus ? 1 : 0
                    border.color: Theme.accent
                }
                // One gone from disk: a tile saying so, as in the chat (in
                // light, a shade under the stage: the pictures there have
                // no card).
                Rectangle {
                    anchors.fill: thumb
                    visible: thumb.status === "error"
                    radius: 8
                    color: Theme.isLight ? Qt.darker(root.backdrop, 1.035) : Theme.codeBg
                    border.color: Theme.border
                }
                ImageThumb {
                    id: thumb
                    objectName: "imageThumb"
                    anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 6; rightMargin: 14; topMargin: 6 }
                    height: Math.round(width * root.gridShape)
                    // (Over its name, however wide it is.)
                    bottomAligned: true
                    path: cellItem.modelData
                }
                Label {
                    anchors.centerIn: thumb
                    width: thumb.width - 12
                    visible: thumb.status === "error"
                    text: "No longer on disk"
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(11)
                }
                // The picture's name: where it came from, for the chat's.
                Label {
                    objectName: "imageThumbName"
                    anchors { left: parent.left; right: parent.right; bottom: parent.bottom; leftMargin: 8; rightMargin: 16; bottomMargin: 10 }
                    text: root.ownList ? root.labelOf(cellItem.modelData) : cellItem.modelData.substring(cellItem.modelData.lastIndexOf("/") + 1)
                    color: cellItem.modelData === root.path ? Theme.foreground : Theme.muted
                    font.pixelSize: Theme.px(11)
                    // (A chat's say who sent it first; a file's name keeps its end.)
                    elide: root.ownList ? Text.ElideRight : Text.ElideMiddle
                    horizontalAlignment: Text.AlignHCenter
                }
                HoverHandler { id: cellHover; cursorShape: Qt.PointingHandCursor }
                TapHandler {
                    onTapped: { grid.currentIndex = cellItem.index; root.openFromGrid(cellItem.index) }
                }
            }
        }

        Rectangle {
            visible: root.gone
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.top: parent.top
            anchors.topMargin: 12
            implicitWidth: goneLabel.implicitWidth + 20
            implicitHeight: 28
            radius: 6
            color: Qt.rgba(Theme.warning.r, Theme.warning.g, Theme.warning.b, 0.16)
            border.color: Theme.warning
            Label { id: goneLabel; anchors.centerIn: parent; text: "This image was deleted on disk."; color: Theme.foreground; font.pixelSize: Theme.px(12) }
        }

        // The bar under the image: where it is in its folder, its size and
        // zoom, and the view's controls. Narrow, turning, the grid and
        // filling go into a menu and the size gives way, so nothing is cut
        // off at the edge.
        Rectangle {
            id: bar
            readonly property bool tight: width < 640
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
            height: 34
            color: Theme.subtle
            Rectangle { width: parent.width; height: 1; color: Theme.border }
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 8
                spacing: 2
                ComposerIconButton {
                    objectName: "imagePrev"
                    visible: root.images.length > 1
                    iconName: "arrow-left-s"; tint: Theme.muted
                    tip: "Previous (←, K)"
                    focusPolicy: Qt.NoFocus
                    onClicked: root.step(-1)
                }
                Label {
                    visible: root.images.length > 1
                    text: (root.index + 1) + " / " + root.images.length
                    color: Theme.muted
                    font.pixelSize: Theme.px(11)
                    font.features: { "tnum": 1 }
                }
                ComposerIconButton {
                    objectName: "imageNext"
                    visible: root.images.length > 1
                    iconName: "arrow-right-s"; tint: Theme.muted
                    tip: "Next (→, J, Space)"
                    focusPolicy: Qt.NoFocus
                    onClicked: root.step(1)
                }
                // Over the chat: where the picture came from.
                Label {
                    objectName: "imageOrigin"
                    Layout.leftMargin: 8
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    visible: root.ownList && !root.gridMode && !bar.tight
                    text: root.labelOf(root.path)
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(11.5)
                    elide: Text.ElideMiddle
                }
                Label {
                    Layout.leftMargin: 8
                    visible: !root.gridMode && view.imageWidth > 0 && !bar.tight
                    text: view.imageWidth + " × " + view.imageHeight
                          + (view.frameCount > 1 ? "  ·  frame " + (view.frame + 1) + " of " + view.frameCount : "")
                    color: Theme.muted
                    font.pixelSize: Theme.px(11)
                    font.features: { "tnum": 1 }
                }
                Label {
                    visible: root.gridMode
                    Layout.leftMargin: 8
                    text: root.images.length === 1 ? "1 picture" : root.images.length + (root.ownList ? " pictures in this chat" : " images")
                    color: Theme.muted
                    font.pixelSize: Theme.px(11)
                }
                Item { Layout.fillWidth: true; visible: !(root.ownList && !root.gridMode && !bar.tight) }
                ComposerIconButton {
                    objectName: "imagePlay"
                    visible: !root.gridMode && view.frameCount > 1
                    iconName: view.playing ? "pause" : "play"; tint: Theme.muted
                    tip: view.playing ? "Pause (P)" : "Play (P)"
                    focusPolicy: Qt.NoFocus
                    onClicked: view.playing = !view.playing
                }
                ComposerIconButton {
                    visible: !root.gridMode
                    iconName: "subtract"; tint: Theme.muted
                    tip: "Zoom out (−)"
                    focusPolicy: Qt.NoFocus
                    onClicked: view.zoomOut()
                }
                Label {
                    objectName: "imageZoom"
                    visible: !root.gridMode
                    Layout.minimumWidth: 40
                    horizontalAlignment: Text.AlignHCenter
                    text: Math.round(view.zoom * 100) + "%"
                    color: Theme.muted
                    font.pixelSize: Theme.px(11)
                    font.features: { "tnum": 1 }
                }
                ComposerIconButton {
                    visible: !root.gridMode
                    iconName: "add"; tint: Theme.muted
                    tip: "Zoom in (+)"
                    focusPolicy: Qt.NoFocus
                    onClicked: view.zoomIn()
                }
                FlatButton {
                    objectName: "imageFit"
                    visible: !root.gridMode
                    text: view.fit ? "1:1" : "Fit"
                    font.pixelSize: Theme.px(11)
                    padding: 4
                    outlined: false
                    focusPolicy: Qt.NoFocus
                    ToolTip.visible: hovered
                    ToolTip.delay: 400
                    ToolTip.text: view.fit ? "Actual size (1)" : "Fit (0)"
                    onClicked: view.fit ? view.actualSize() : view.fitView()
                }
                ComposerIconButton {
                    objectName: "imageTurnLeft"
                    visible: !root.gridMode && !bar.tight
                    iconName: "rotate-left"; tint: Theme.muted
                    tip: "Turn left (Shift+R)"
                    focusPolicy: Qt.NoFocus
                    onClicked: view.rotate(false)
                }
                ComposerIconButton {
                    visible: !root.gridMode && !bar.tight
                    iconName: "rotate-right"; tint: Theme.muted
                    tip: "Turn right (R)"
                    focusPolicy: Qt.NoFocus
                    onClicked: view.rotate(true)
                }
                ComposerIconButton {
                    objectName: "imageGridButton"
                    visible: (root.folder !== "" || (root.ownList && root.images.length > 1)) && (!bar.tight || root.gridMode)
                    iconName: "grid"; tint: Theme.muted
                    on: root.gridMode
                    tip: root.gridMode ? "Back to the image (G, Esc)" : root.ownList ? "All the chat's pictures (G)" : "The folder's images (G)"
                    focusPolicy: Qt.NoFocus
                    onClicked: root.showGrid(!root.gridMode)
                }
                ComposerIconButton {
                    objectName: "imageOpenInTab"
                    visible: root.ownList
                    iconName: "folder-open"; tint: Theme.muted
                    tip: "Open in a tab, with its folder (T)"
                    focusPolicy: Qt.NoFocus
                    onClicked: root.openInTab(root.path)
                }
                // Narrow: the rest of the controls, in a menu.
                ComposerIconButton {
                    id: moreButton
                    objectName: "imageMore"
                    visible: bar.tight && !root.gridMode
                    iconName: "more"; tint: Theme.muted
                    tip: "More"
                    focusPolicy: Qt.NoFocus
                    onClicked: moreMenu.popup(moreButton, 0, -moreMenu.implicitHeight)
                    AppMenu {
                        id: moreMenu
                        // (What was chosen runs once the menu has gone, so
                        // the keys go where it puts them, not back to the
                        // image.)
                        property var after: null
                        onClosed: { const f = after; after = null; if (f) f() }
                        // (Too narrow for it in the bar: where it came from.)
                        AppMenuItem {
                            objectName: "imageOriginItem"
                            visible: root.ownList
                            enabled: false
                            text: root.labelOf(root.path)
                        }
                        AppMenuSeparator { visible: root.ownList; height: visible ? implicitHeight : 0 }
                        AppMenuItem { text: "Turn left"; hint: "Shift+R"; onTriggered: moreMenu.after = () => view.rotate(false) }
                        AppMenuItem { text: "Turn right"; hint: "R"; onTriggered: moreMenu.after = () => view.rotate(true) }
                        AppMenuItem {
                            visible: root.folder !== "" || (root.ownList && root.images.length > 1)
                            text: root.ownList ? "All the chat's pictures" : "The folder's images"
                            hint: "G"
                            onTriggered: moreMenu.after = () => root.showGrid(true)
                        }
                        AppMenuItem {
                            visible: !root.ownList
                            text: root.filled ? "Back into the tab" : "Fill the window"
                            hint: "F"
                            onTriggered: moreMenu.after = () => { root.filled = !root.filled; root.focusView() }
                        }
                    }
                }
                ComposerIconButton {
                    visible: !root.ownList && (!bar.tight || root.gridMode)
                    iconName: root.filled ? "fullscreen-exit" : "fullscreen"; tint: Theme.muted
                    tip: root.filled ? "Back into the tab (F, Esc)" : "Fill the window (F)"
                    focusPolicy: Qt.NoFocus
                    onClicked: { root.filled = !root.filled; root.focusView() }
                }
                // (Last, where it's looked for.)
                ComposerIconButton {
                    objectName: "imageClose"
                    visible: root.ownList
                    iconName: "close"; tint: Theme.muted
                    tip: "Close (Esc)"
                    focusPolicy: Qt.NoFocus
                    onClicked: root.closeRequested()
                }
            }
        }
    }
}
