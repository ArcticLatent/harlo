import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// A file open in a tab: always editable, syntax-highlighted, Ctrl+S saves.
// Unsaved text lives in `drafts` (shared with the owner) so it survives
// switching tabs.
Rectangle {
    id: root
    required property string path      // absolute; "" when no file tab is active
    required property var drafts       // { path: text } of unsaved files
    signal draftsEdited()
    signal saved(string path)
    color: Theme.background

    TextFile { id: file }
    // Images are shown, not edited.
    function isImagePath(p) { return /\.(png|jpe?g|gif|webp|bmp|svg|ico|tiff?)$/i.test(p) }
    readonly property bool isImage: isImagePath(path)
    // The loaded file is the one on screen (an older load may still be pending).
    readonly property bool current: file.path === path
    readonly property bool modified: drafts[path] !== undefined
    // The editor for the current file. A new one is built for every file:
    // re-using one editor and swapping its text crashed Qt's syntax
    // highlighter, which still had queued work for the old text.
    readonly property var editor: editorLoader.item ? editorLoader.item.editor : null

    onPathChanged: reload()
    Component.onCompleted: reload()
    function reload() {
        diskNotice = ""
        imageVersion = 0
        imageGone = false
        imageStamp = isImagePath(path) ? Workspace.fileStamp(path) : ""
        editorLoader.active = false
        zoomFit = true
        // Test the new path itself: `isImage` may not be re-evaluated yet
        // when this handler runs, and would still describe the previous file.
        if (path !== "" && !isImagePath(path)) file.load(path)
    }
    property bool zoomFit: true

    // ---- the file changing on disk (e.g. an agent edited it) ---------------
    // "" | "changed" | "deleted": shown as a bar while there are unsaved edits
    // (or the file is gone); without edits a change just reloads.
    property string diskNotice: ""
    // Cursor and scroll to restore after a reload.
    property var restoreView: null
    Timer {
        interval: 1500
        repeat: true
        running: root.visible && root.current && file.state === "text" && !root.isImage
        onTriggered: {
            const disk = file.diskState()
            if (disk === "same") {
                if (root.diskNotice === "deleted") root.diskNotice = ""
                return
            }
            if (disk === "changed" && !root.modified) root.reloadKeepingView()
            else root.diskNotice = disk
        }
    }
    function reloadKeepingView() {
        const area = editorLoader.item
        root.restoreView = area ? { path: root.path, cursor: area.editor.cursorPosition, x: area.flick.contentX, y: area.flick.contentY } : null
        root.diskNotice = ""
        reload()
    }
    // Images: reload when the file changes on disk (an agent regenerated
    // it, say); the version suffix makes Image fetch it again.
    property string imageStamp: ""
    property int imageVersion: 0
    property bool imageGone: false
    Timer {
        interval: 1500
        repeat: true
        // (A remote image is a copy; it isn't watched.)
        running: root.visible && root.isImage && root.path !== "" && !root.remote
        onTriggered: {
            const stamp = Workspace.fileStamp(root.path)
            root.imageGone = stamp === ""
            if (stamp !== "" && root.imageStamp !== "" && stamp !== root.imageStamp)
                root.imageVersion++
            root.imageStamp = stamp
        }
    }
    onIsImageChanged: { imageStamp = isImage && !remote ? Workspace.fileStamp(path) : ""; imageGone = false }
    /// On a remote host: images show from a local copy.
    readonly property bool remote: path.startsWith("ssh://")
    // (A remote image: copied here off the UI thread; shown once it is.)
    readonly property string imagePath: isImage && path !== "" ? (remote ? (Workspace.copiesRevision >= 0 ? Workspace.localCopy(path) : "") : path) : ""
    function keepMine() {
        file.acknowledgeDisk()
        root.diskNotice = ""
    }
    Connections {
        target: file
        function onStateChanged() {
            if (file.state === "text" && file.path === root.path)
                editorLoader.active = true
        }
    }

    // (A remote file's save finishes over SSH: `savedTo` says when.)
    function save() {
        if (file.state !== "text" || !root.editor || file.saving) return
        file.save(root.editor.text)
    }
    Connections {
        target: file
        function onSavedTo(path) {
            delete root.drafts[path]
            root.draftsEdited()
            root.saved(path)
        }
    }
    Shortcut { sequence: "Ctrl+S"; enabled: root.visible && root.path !== ""; onActivated: root.save() }

    // For `--selftest`.
    readonly property string language: editorLoader.item ? editorLoader.item.language : ""
    readonly property string state: file.state
    readonly property bool diskBarVisible: diskNotice !== ""
    readonly property string shownMessage: messageLabel.visible ? messageLabel.text : ""
    readonly property string imageInfo: isImage ? image.status + " " + image.sourceSize.width + "x" + image.sourceSize.height : ""
    function typeText(t) { if (root.editor) root.editor.insert(root.editor.cursorPosition, t) }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // Header
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: 40
            color: Theme.subtle
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.border }
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 14
                anchors.rightMargin: 8
                spacing: 8
                Label {
                    Layout.fillWidth: true
                    text: root.path.replace(/^\/home\/[^/]+/, "~") + (root.modified ? "  ●" : "")
                    color: Theme.foreground
                    font.family: Theme.mono
                    font.pixelSize: Theme.mpx(12)
                    elide: Text.ElideMiddle
                }
                Label {
                    visible: root.language !== ""
                    text: root.language
                    color: Theme.muted
                    font.pixelSize: Theme.px(11)
                }
                Label {
                    visible: root.isImage && image.status === Image.Ready
                    text: image.sourceSize.width + " × " + image.sourceSize.height + (root.zoomFit && image.paintedWidth < image.sourceSize.width ? "  ·  " + Math.round(100 * image.paintedWidth / image.sourceSize.width) + "%" : "")
                    color: Theme.muted
                    font.pixelSize: Theme.px(11)
                }
                FlatButton {
                    visible: root.isImage
                    text: root.zoomFit ? "100%" : "Fit"
                    font.pixelSize: Theme.px(11)
                    padding: 4
                    onClicked: root.zoomFit = !root.zoomFit
                }
                Label {
                    visible: !!root.editor
                    text: root.editor ? "Ln " + (root.editor.text.substring(0, root.editor.cursorPosition).split("\n").length) + "  ·  " + root.editor.lineCount + " lines" : ""
                    color: Theme.muted
                    font.pixelSize: Theme.px(11)
                }
                FlatButton { visible: !root.isImage; text: file.saving ? "Saving…" : "Save"; accent: Theme.primary; font.pixelSize: Theme.px(11); padding: 4; enabled: root.modified && !file.saving; onClicked: root.save() }
                FlatButton {
                    visible: !root.remote
                    text: "↗"; font.pixelSize: Theme.px(12); outlined: false; padding: 3
                    ToolTip.visible: hovered; ToolTip.text: "Open in default app"
                    onClicked: Qt.openUrlExternally("file://" + root.path)
                }
            }
        }

        // Image viewer: fit to the area, or actual size with scrolling.
        Flickable {
            id: imageFlick
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.isImage
            clip: true
            contentWidth: root.zoomFit ? width : Math.max(width, image.sourceSize.width + 32)
            contentHeight: root.zoomFit ? height : Math.max(height, image.sourceSize.height + 32)
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            ScrollBar.horizontal: ScrollBar { }
            // Checkerboard-ish backdrop so transparent images read clearly.
            Rectangle { anchors.fill: parent; color: Theme.codeBg }
            Image {
                id: image
                source: root.imagePath !== "" ? "file://" + root.imagePath + (root.imageVersion > 0 ? "?v=" + root.imageVersion : "") : ""
                asynchronous: true
                cache: false
                fillMode: Image.PreserveAspectFit
                smooth: true
                mipmap: true
                // Never upscale small images when fitting.
                width: root.zoomFit ? Math.min(imageFlick.width - 32, sourceSize.width) : sourceSize.width
                height: root.zoomFit ? Math.min(imageFlick.height - 32, sourceSize.height) : sourceSize.height
                x: Math.max(16, (imageFlick.contentWidth - width) / 2)
                y: Math.max(16, (imageFlick.contentHeight - height) / 2)
            }
            Label {
                anchors.centerIn: parent
                visible: image.status === Image.Error && !root.imageGone
                text: "Could not load this image."
                color: Theme.error
            }
            Rectangle {
                visible: root.imageGone
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
        }

        // The file changed (or vanished) on disk while you have unsaved edits.
        Rectangle {
            Layout.fillWidth: true
            visible: root.diskNotice !== "" && root.current
            implicitHeight: 38
            color: Qt.rgba(Theme.warning.r, Theme.warning.g, Theme.warning.b, 0.16)
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.warning; opacity: 0.6 }
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 14
                anchors.rightMargin: 8
                spacing: 8
                Label {
                    Layout.fillWidth: true
                    text: root.diskNotice === "deleted"
                        ? "This file was deleted on disk."
                        : "This file changed on disk while you have unsaved edits."
                    color: Theme.foreground
                    font.pixelSize: Theme.px(12)
                    elide: Text.ElideRight
                }
                FlatButton {
                    visible: root.diskNotice === "changed"
                    text: "Reload (drop my edits)"
                    font.pixelSize: Theme.px(11)
                    padding: 4
                    accent: Theme.warning
                    onClicked: {
                        delete root.drafts[root.path]
                        root.draftsEdited()
                        root.reloadKeepingView()
                    }
                }
                FlatButton {
                    text: root.diskNotice === "deleted" ? "Keep editing" : "Keep mine"
                    font.pixelSize: Theme.px(11)
                    padding: 4
                    ToolTip.visible: hovered
                    ToolTip.text: root.diskNotice === "deleted" ? "Saving will create the file again" : "Saving will replace the version on disk"
                    onClicked: root.keepMine()
                }
                FlatButton {
                    visible: root.diskNotice === "deleted"
                    text: "Close tab"
                    font.pixelSize: Theme.px(11)
                    padding: 4
                    onClicked: {
                        delete root.drafts[root.path]
                        root.draftsEdited()
                        Workspace.closeTab("file:" + root.path)
                    }
                }
            }
        }

        Label {
            id: messageLabel
            Layout.margins: 16
            visible: !root.isImage && root.path !== "" && !(root.current && file.state === "text")
            text: !root.current || file.state === "loading" ? "Loading…" : file.message
            color: file.error || file.state === "large" ? Theme.error : Theme.muted
            wrapMode: Text.Wrap
        }

        Loader {
            id: editorLoader
            Layout.fillWidth: true
            Layout.fillHeight: true
            active: false
            visible: root.current && file.state === "text" && !root.isImage
            sourceComponent: editorComponent
        }

        Label {
            Layout.fillWidth: true
            Layout.margins: 6
            Layout.leftMargin: 14
            visible: root.current && file.message !== "" && file.state === "text"
            text: file.message
            color: file.error ? Theme.error : Theme.muted
            font.pixelSize: Theme.px(11)
        }
    }

    Component {
        id: editorComponent
        Item {
            id: area
            property alias editor: editor
            property alias flick: flick
            readonly property string language: highlight.item ? highlight.item.language : ""
            readonly property real lineHeight: editor.lineCount > 0 ? editor.contentHeight / editor.lineCount : 16
            property bool ready: false

            Component.onCompleted: {
                // Text first, then the highlighter, then start tracking edits.
                editor.text = root.drafts[root.path] !== undefined ? root.drafts[root.path] : file.text
                const view = root.restoreView && root.restoreView.path === root.path ? root.restoreView : null
                root.restoreView = null
                editor.cursorPosition = view ? Math.min(view.cursor, editor.length) : 0
                highlight.active = true
                ready = true
                editor.forceActiveFocus()
                if (view) Qt.callLater(() => { flick.contentX = view.x; flick.contentY = Math.min(view.y, Math.max(0, flick.contentHeight - flick.height)) })
            }

            Loader {
                id: highlight
                active: false
                source: "EditorHighlighter.qml"
                onLoaded: {
                    item.fileName = root.path.split("/").pop()
                    item.textEdit = editor
                }
            }

            // Line numbers, scrolled with the text.
            ListView {
                id: gutter
                width: 52
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.topMargin: 8
                interactive: false
                clip: true
                model: editor.lineCount
                contentY: flick.contentY - originY
                delegate: Label {
                    required property int index
                    width: gutter.width - 10
                    height: area.lineHeight
                    text: index + 1
                    horizontalAlignment: Text.AlignRight
                    verticalAlignment: Text.AlignVCenter
                    color: Theme.muted
                    opacity: 0.55
                    font.family: Theme.mono
                    font.pixelSize: Theme.mpx(12)
                }
            }
            Rectangle { x: gutter.width; width: 1; height: parent.height; color: Theme.border; opacity: 0.6 }

            Flickable {
                id: flick
                anchors.fill: parent
                anchors.leftMargin: gutter.width + 8
                anchors.topMargin: 8
                clip: true
                contentWidth: Math.max(width, editor.implicitWidth + 40)
                contentHeight: Math.max(height, editor.implicitHeight + 40)
                boundsBehavior: Flickable.StopAtBounds
                pixelAligned: true
                ScrollBar.vertical: ScrollBar { }
                ScrollBar.horizontal: ScrollBar { }

                // Keep the cursor in view while typing or moving with the keyboard.
                function ensureVisible(r) {
                    if (contentX >= r.x) contentX = r.x
                    else if (contentX + width <= r.x + r.width + 20) contentX = r.x + r.width + 20 - width
                    if (contentY >= r.y) contentY = r.y
                    else if (contentY + height <= r.y + r.height + 20) contentY = r.y + r.height + 20 - height
                }

                TextArea {
                    id: editor
                    width: Math.max(flick.width, implicitWidth)
                    padding: 0
                    leftPadding: 0
                    topPadding: 0
                    bottomPadding: 0
                    background: null
                    wrapMode: TextEdit.NoWrap
                    font.family: Theme.mono
                    font.pixelSize: Theme.mpx(13)
                    color: Theme.foreground
                    selectionColor: Theme.selection
                    selectedTextColor: Theme.foreground
                    persistentSelection: true
                    tabStopDistance: 4 * metrics.averageCharacterWidth
                    FontMetrics { id: metrics; font: editor.font }
                    onCursorRectangleChanged: flick.ensureVisible(cursorRectangle)
                    onTextChanged: {
                        if (!area.ready) return
                        if (text === file.text) delete root.drafts[root.path]
                        else root.drafts[root.path] = text
                        root.draftsEdited()
                    }
                }
            }
        }
    }
}
