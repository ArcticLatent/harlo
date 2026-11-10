import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// A file open in a tab: always editable, syntax-highlighted, Ctrl+S saves.
// The text is a CodeView (drawn in Rust). A file's unsaved edits (its text
// and undo) are kept by the view's side when you switch tabs; `drafts` (shared
// with the owner) says which files have them. Ctrl+F finds (Ctrl+H replaces), Ctrl+G goes to a line,
// Ctrl+/ comments lines out; Enter keeps the indentation, Tab and Shift+Tab
// indent and outdent lines, and the bracket next to the cursor is shown
// with its partner.
Rectangle {
    id: root
    required property string path      // absolute; "" when no file tab is active
    required property var drafts       // { path: true } of files with unsaved edits
    signal draftsEdited()
    signal saved(string path)
    color: Theme.background

    TextFile { id: file }
    // Images are shown, not edited.
    function isImagePath(p) { return Images.isImage(p) }
    readonly property bool isImage: isImagePath(path)
    // The loaded file is the one on screen (an older load may still be pending).
    readonly property bool current: file.path === path
    readonly property bool modified: drafts[path] !== undefined
    // The editor for the current file. A new one is built for every file:
    // re-using one editor and swapping its text crashed Qt's syntax
    // highlighter, which still had queued work for the old text.
    readonly property var editor: editorLoader.item ? editorLoader.item.editor : null
    readonly property alias imageViewer: imageViewer

    onPathChanged: { closeBars(); reload() }
    Component.onCompleted: reload()
    function reload() {
        // The file shown until now: its unsaved edits kept for when it's
        // opened again (unless they were dropped).
        const ed = root.editor
        if (ed && ed.modified && root.drafts[ed.path] !== undefined) ed.keep()
        diskNotice = ""
        imageVersion = 0
        imageGone = false
        imageStamp = isImagePath(path) ? Workspace.fileStamp(path) : ""
        editorLoader.active = false
        // An image: the grid when asked for (a folder's "View images"), and
        // the keys.
        if (path !== "" && isImagePath(path)) {
            imageViewer.gridMode = false
            if (gridNext) Qt.callLater(() => imageViewer.showGrid(true))
            else Qt.callLater(() => { if (root.visible) imageViewer.focusView() })
        }
        gridNext = false
        // Test the new path itself: `isImage` may not be re-evaluated yet
        // when this handler runs, and would still describe the previous file.
        if (path !== "" && !isImagePath(path)) file.load(path)
    }
    /// The next image opens in its folder's grid.
    property bool gridNext: false

    // ---- the file changing on disk (e.g. an agent edited it) ---------------
    // "" | "changed" | "deleted": shown as a bar while there are unsaved edits
    // (or the file is gone); without edits a change just reloads.
    property string diskNotice: ""
    // Cursor and scroll to restore after a reload.
    property var restoreView: null
    // A line to show once its file is open ({path, line}): a listing's line
    // number, clicked in a session.
    property var pendingLine: null
    function showLine(p, line) {
        root.pendingLine = { path: p, line: line }
        if (p === root.path && editorLoader.item && editorLoader.item.ready) editorLoader.item.applyLine()
    }
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
        root.restoreView = area ? { path: root.path, cursor: area.editor.cursorPosition, x: area.editor.contentX, y: area.editor.contentY } : null
        root.diskNotice = ""
        reload()
    }
    // Images: reload when the file changes on disk (an agent regenerated
    // it, say).
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
            if (stamp !== "" && root.imageStamp !== "" && stamp !== root.imageStamp) {
                root.imageVersion++
                imageViewer.view.reload()
            }
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
            if (root.editor && root.editor.path === path) root.editor.markSaved()
            else CodeDrafts.markSaved(path)
            delete root.drafts[path]
            root.draftsEdited()
            root.saved(path)
        }
    }
    Shortcut { sequence: "Ctrl+S"; enabled: root.visible && root.path !== ""; onActivated: root.save() }

    // For `--selftest`.
    readonly property string language: editorLoader.item ? editorLoader.item.language : ""
    readonly property string state: file.state
    /// The text is all in (a long file comes in a slice at a time).
    readonly property bool loaded: editorLoader.item ? editorLoader.item.ready : false
    readonly property bool diskBarVisible: diskNotice !== ""
    readonly property string shownMessage: messageLabel.visible ? messageLabel.text : ""
    readonly property string imageInfo: isImage ? imageViewer.view.status + " " + imageViewer.view.imageWidth + "x" + imageViewer.view.imageHeight : ""
    function typeText(t) { if (root.editor) root.editor.insert(root.editor.cursorPosition, t) }

    // ---- edits in one undo step ---------------------------------------------
    // `edits`: [[start, end, text], ...] in the text as it is now.
    function apply(edits) { return root.editor ? root.editor.apply(JSON.stringify(edits)) : false }

    // ---- find and replace, go to line ---------------------------------------
    property bool findOpen: false
    property bool replaceOpen: false
    property bool lineOpen: false
    property bool matchCase: false
    // Where the query is found (start positions), and which one is current.
    property var matches: []
    property int matchLength: 0
    property int currentMatch: -1
    // "Replaced 12", a moment after Replace all.
    property string replacedNote: ""
    readonly property int matchLimit: 50000

    function openFind(replace) {
        if (!root.editor) return
        lineOpen = false
        const sel = root.editor.selectedText
        if (sel !== "" && sel.indexOf("\u2029") < 0 && sel.indexOf("\n") < 0) findField.text = sel
        findOpen = true
        if (replace) replaceOpen = true
        findField.selectAll()
        findField.forceActiveFocus()
        search(true)
    }
    function openGoToLine() {
        if (!root.editor) return
        findOpen = false
        lineOpen = true
        lineField.text = ""
        lineField.forceActiveFocus()
    }
    function closeBars() {
        const had = findOpen || lineOpen
        findOpen = false
        lineOpen = false
        matches = []
        currentMatch = -1
        if (root.editor) root.editor.clearFind()
        if (had && root.editor) root.editor.forceActiveFocus()
    }
    // Finds the query again (after typing it, or an edit); `select` also
    // selects the first match from the cursor and shows it.
    function search(select) {
        const ed = root.editor, q = findField.text
        if (!ed || !findOpen || q === "") { matches = []; currentMatch = -1; if (ed) ed.clearFind(); return }
        // (Found, and marked in the text, by the view.)
        const list = JSON.parse(ed.findAll(q, matchCase, matchLimit))
        matchLength = q.length
        matches = list
        if (list.length === 0) { currentMatch = -1; return }
        const from = ed.selectionStart
        let i = 0
        while (i < list.length && list[i] < from) i++
        if (select) go(i)
        else currentMatch = i < list.length ? i : 0
    }
    function go(i) {
        const ed = root.editor
        if (!ed || matches.length === 0) return
        currentMatch = (i % matches.length + matches.length) % matches.length
        const s = matches[currentMatch]
        ed.reveal(s, false)
        ed.select(s, s + matchLength)
    }
    // Enter / F3: the next match (Shift: the one before), from the cursor
    // if it moved off the current one.
    function findStep(dir) {
        if (!findOpen || findField.text === "") { openFind(false); return }
        const ed = root.editor
        if (!ed || matches.length === 0) return
        const s = matches[currentMatch]
        if (currentMatch >= 0 && ed.selectionStart === s && ed.selectionEnd === s + matchLength) { go(currentMatch + dir); return }
        const at = ed.cursorPosition
        let i = 0
        while (i < matches.length && matches[i] < at) i++
        go(dir > 0 ? i : i - 1)
    }
    function replaceOne() {
        const ed = root.editor
        if (!ed || currentMatch < 0) return
        const s = matches[currentMatch]
        // (What's replaced is the match you were shown: select it first.)
        if (ed.selectionStart !== s || ed.selectionEnd !== s + matchLength) { go(currentMatch); return }
        const r = replaceField.text
        apply([[s, s + matchLength, r]])
        ed.cursorPosition = s + r.length
        search(true)
    }
    function replaceAll() {
        if (!root.editor || matches.length === 0) return
        const r = replaceField.text, n = matches.length
        apply(matches.map(s => [s, s + matchLength, r]))
        search(false)
        replacedNote = "Replaced " + n
        replacedFade.restart()
    }
    Timer { id: replacedFade; interval: 2500; onTriggered: root.replacedNote = "" }
    Timer { id: findDebounce; interval: 120; onTriggered: root.search(true) }
    Timer { id: refind; interval: 150; onTriggered: root.search(false) }
    function goToLine() {
        const n = parseInt(lineField.text, 10)
        if (!(n >= 1) || !root.editor) return
        lineOpen = false
        showLine(root.path, Math.min(n, root.editor.lineCount))
    }

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
                    objectName: "editorLineLabel"
                    visible: !!root.editor
                    text: root.editor ? "Ln " + (root.editor.cursorLine + 1) + "  ·  " + root.editor.lineCount + " lines" : ""
                    color: lineHover.hovered ? Theme.foreground : Theme.muted
                    font.pixelSize: Theme.px(11)
                    HoverHandler { id: lineHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: root.openGoToLine() }
                    ToolTip.visible: lineHover.hovered
                    ToolTip.delay: 400
                    ToolTip.text: "Go to line (Ctrl+G)"
                }
                Button {
                    id: findButton
                    objectName: "editorFindButton"
                    visible: !!root.editor
                    flat: true
                    padding: 0
                    implicitWidth: 22
                    implicitHeight: 22
                    focusPolicy: Qt.NoFocus
                    contentItem: Item {
                        Icon { anchors.centerIn: parent; name: "search"; implicitWidth: 13; implicitHeight: 13
                               color: root.findOpen || findButton.hovered ? Theme.foreground : Theme.muted }
                    }
                    background: Rectangle { radius: 5; color: root.findOpen ? Theme.selected : findButton.hovered ? Theme.hover : "transparent" }
                    ToolTip.visible: hovered
                    ToolTip.delay: 400
                    ToolTip.text: "Find" + Keymap.hint("findInSession") + ", replace (Ctrl+H)"
                    onClicked: root.findOpen ? root.closeBars() : root.openFind(false)
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

        // Find (and replace): a strip under the header, so it covers no
        // text.
        Rectangle {
            id: findCard
            objectName: "editorFind"
            Layout.fillWidth: true
            visible: root.findOpen && !!root.editor && root.current
            implicitHeight: findColumn.implicitHeight + 12
            color: Theme.subtle
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.border }
            ColumnLayout {
                id: findColumn
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                anchors.leftMargin: 8
                anchors.rightMargin: 8
                spacing: 4
                RowLayout {
                    spacing: 2
                    ComposerIconButton {
                        objectName: "editorReplaceToggle"
                        iconName: root.replaceOpen ? "arrow-down-s" : "arrow-right-s"
                        tip: root.replaceOpen ? "Hide replace" : "Replace (Ctrl+H)"
                        onClicked: { root.replaceOpen = !root.replaceOpen; (root.replaceOpen ? replaceField : findField).forceActiveFocus() }
                    }
                    AppTextField {
                        id: findField
                        objectName: "editorFindField"
                        Layout.fillWidth: true
                        Layout.maximumWidth: 340
                        implicitHeight: 28
                        leftPadding: 28
                        placeholderText: "Find in this file"
                        Icon { x: 8; anchors.verticalCenter: parent.verticalCenter; name: "search"; color: Theme.muted; implicitWidth: 14; implicitHeight: 14 }
                        onTextEdited: findDebounce.restart()
                        Keys.onReturnPressed: event => root.findStep((event.modifiers & Qt.ShiftModifier) ? -1 : 1)
                        Keys.onEnterPressed: event => root.findStep((event.modifiers & Qt.ShiftModifier) ? -1 : 1)
                        Keys.onEscapePressed: root.closeBars()
                        Keys.onShortcutOverride: event => event.accepted = event.key === Qt.Key_F3 || ((event.modifiers & Qt.ControlModifier) && (event.key === Qt.Key_H || event.key === Qt.Key_G))
                        Keys.onPressed: event => {
                            if (event.key === Qt.Key_F3) { root.findStep((event.modifiers & Qt.ShiftModifier) ? -1 : 1); event.accepted = true }
                            else if ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_H) { root.replaceOpen = true; replaceField.forceActiveFocus(); event.accepted = true }
                            else if ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_G) { root.openGoToLine(); event.accepted = true }
                        }
                    }
                    Button {
                        id: caseButton
                        objectName: "editorMatchCase"
                        flat: true
                        padding: 0
                        implicitWidth: 28
                        implicitHeight: 26
                        Layout.leftMargin: 4
                        focusPolicy: Qt.NoFocus
                        contentItem: Label {
                            text: "Aa"
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                            color: root.matchCase ? Theme.foreground : Theme.textMuted
                            font.pixelSize: Theme.px(12)
                            font.weight: root.matchCase ? Font.DemiBold : Font.Normal
                        }
                        background: Rectangle { radius: 6; color: root.matchCase ? Theme.selected : caseButton.hovered ? Theme.hover : "transparent" }
                        ToolTip.visible: hovered
                        ToolTip.delay: 400
                        ToolTip.text: root.matchCase ? "Matching case" : "Match case"
                        onClicked: { root.matchCase = !root.matchCase; root.search(true) }
                    }
                    Label {
                        objectName: "editorFindCount"
                        Layout.minimumWidth: 56
                        horizontalAlignment: Text.AlignHCenter
                        text: root.replacedNote !== "" ? root.replacedNote
                            : root.matches.length > 0 ? (root.currentMatch + 1) + " of " + root.matches.length + (root.matches.length >= root.matchLimit ? "+" : "")
                            : findField.text !== "" && !findDebounce.running ? "None" : ""
                        color: Theme.textMuted
                        font.pixelSize: Theme.px(12)
                    }
                    ComposerIconButton { objectName: "editorFindPrev"; iconName: "arrow-up-s"; tip: "Previous (Shift+Enter)"; enabled: root.matches.length > 0; onClicked: root.findStep(-1) }
                    ComposerIconButton { objectName: "editorFindNext"; iconName: "arrow-down-s"; tip: "Next (Enter)"; enabled: root.matches.length > 0; onClicked: root.findStep(1) }
                    Item { Layout.fillWidth: true }
                    ComposerIconButton { objectName: "editorFindClose"; iconName: "close"; tip: "Close (Esc)"; onClicked: root.closeBars() }
                }
                RowLayout {
                    visible: root.replaceOpen
                    spacing: 4
                    Item { implicitWidth: 24 }
                    AppTextField {
                        id: replaceField
                        objectName: "editorReplaceField"
                        Layout.fillWidth: true
                        Layout.maximumWidth: 340
                        implicitHeight: 28
                        leftPadding: 28
                        placeholderText: "Replace with"
                        Icon { x: 8; anchors.verticalCenter: parent.verticalCenter; name: "edit"; color: Theme.muted; implicitWidth: 14; implicitHeight: 14 }
                        Keys.onReturnPressed: root.replaceOne()
                        Keys.onEnterPressed: root.replaceOne()
                        Keys.onEscapePressed: root.closeBars()
                    }
                    FlatButton { objectName: "editorReplaceOne"; text: "Replace"; font.pixelSize: Theme.px(11); padding: 4; Layout.leftMargin: 4; enabled: root.matches.length > 0; onClicked: root.replaceOne() }
                    FlatButton { objectName: "editorReplaceAll"; text: "Replace all"; font.pixelSize: Theme.px(11); padding: 4; enabled: root.matches.length > 0; onClicked: root.replaceAll() }
                    Item { Layout.fillWidth: true }
                }
            }
        }
        // Go to line: the same strip.
        Rectangle {
            objectName: "editorGoToLine"
            Layout.fillWidth: true
            visible: root.lineOpen && !!root.editor && root.current
            implicitHeight: 40
            color: Theme.subtle
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.border }
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 34
                anchors.rightMargin: 8
                spacing: 8
                AppTextField {
                    id: lineField
                    objectName: "editorLineField"
                    Layout.preferredWidth: 160
                    implicitHeight: 28
                    placeholderText: "Go to line"
                    inputMethodHints: Qt.ImhDigitsOnly
                    validator: IntValidator { bottom: 1 }
                    Keys.onReturnPressed: root.goToLine()
                    Keys.onEnterPressed: root.goToLine()
                    Keys.onEscapePressed: root.closeBars()
                }
                Label {
                    text: root.editor ? "1–" + root.editor.lineCount : ""
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(12)
                }
                Item { Layout.fillWidth: true }
                ComposerIconButton { iconName: "close"; tip: "Close (Esc)"; onClicked: root.closeBars() }
            }
        }

        ImageViewer {
            id: imageViewer
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.isImage
            path: root.isImage ? root.path : ""
            source: root.imagePath
            remote: root.remote
            gone: root.imageGone
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
                        CodeDrafts.discard(root.path)
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
                        CodeDrafts.discard(root.path)
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
            readonly property string language: editor.language
            readonly property real lineHeight: editor.lineHeight
            property bool ready: false

            Component.onCompleted: {
                // (The path first: the text's language, comments and
                // indentation; set once, as this view goes with its file.)
                editor.path = root.path
                const kept = root.drafts[root.path] !== undefined && editor.restore(root.path)
                if (!kept) {
                    CodeDrafts.discard(root.path)
                    if (root.drafts[root.path] !== undefined) { delete root.drafts[root.path]; root.draftsEdited() }
                    editor.text = file.text
                }
                const view = root.restoreView && root.restoreView.path === root.path ? root.restoreView : null
                root.restoreView = null
                if (!kept) editor.cursorPosition = view ? Math.min(view.cursor, editor.length) : 0
                area.ready = true
                if (root.findOpen) root.search(false)
                editor.forceActiveFocus()
                if (view) area.whenSized(() => { editor.contentX = view.x; editor.contentY = view.y })
                else if (kept) area.whenSized(() => editor.reveal(editor.cursorPosition, false))
                area.applyLine()
            }
            // Runs `f` once the view has its size (it scrolls by it).
            property var sized: []
            function whenSized(f) {
                if (editor.height > 0) { f(); return }
                sized.push(f)
                sizedWait.tries = 0
                sizedWait.restart()
            }
            Timer {
                id: sizedWait
                property int tries: 0
                interval: 40
                onTriggered: {
                    if (editor.height <= 0 && ++tries < 25) { restart(); return }
                    const list = area.sized
                    area.sized = []
                    list.forEach(f => f())
                }
            }
            // The line asked for: the cursor at its start, a third of the
            // way down the view, tinted a moment.
            function applyLine() {
                const want = root.pendingLine
                if (!want || want.path !== root.path) return
                root.pendingLine = null
                const line = Math.max(0, Math.min(want.line, editor.lineCount) - 1)
                const at = editor.lineStart(line)
                editor.cursorPosition = at
                editor.forceActiveFocus()
                area.whenSized(() => {
                    editor.contentX = 0
                    editor.reveal(at, true)
                    editor.arrivalLine = line
                    arrivalFade.restart()
                })
            }
            // (For the self-tests: the line arrived at, from 1; 0 none.)
            readonly property int arrival: editor.arrivalLine + 1
            SequentialAnimation {
                id: arrivalFade
                PropertyAction { target: editor; property: "arrivalGlow"; value: 1 }
                PauseAnimation { duration: 1100 }
                NumberAnimation { target: editor; property: "arrivalGlow"; to: 0; duration: Math.max(1, Theme.motion * 2.5); easing.type: Easing.OutCubic }
                PropertyAction { target: editor; property: "arrivalLine"; value: -1 }
            }

            // Keys the editor leaves to this file's bars (the view handles
            // typing, moving, selecting, undo, the clipboard, Enter, Tab and
            // Ctrl+/ itself).
            function claims(e) {
                const ctrl = e.modifiers & Qt.ControlModifier, alt = e.modifiers & Qt.AltModifier
                return (ctrl && !alt && (e.key === Qt.Key_H || e.key === Qt.Key_G))
                    || e.key === Qt.Key_F3 || (e.key === Qt.Key_Escape && (root.findOpen || root.lineOpen))
            }
            function press(e) {
                const ctrl = e.modifiers & Qt.ControlModifier, alt = e.modifiers & Qt.AltModifier, shift = e.modifiers & Qt.ShiftModifier
                let done = true
                if (ctrl && !alt && e.key === Qt.Key_H) root.openFind(true)
                else if (ctrl && !alt && e.key === Qt.Key_G) root.openGoToLine()
                else if (e.key === Qt.Key_F3) root.findStep(shift ? -1 : 1)
                else if (e.key === Qt.Key_Escape && (root.findOpen || root.lineOpen)) root.closeBars()
                else done = false
                e.accepted = done
            }

            CodeView {
                id: editor
                objectName: "fileCode"
                anchors.fill: parent
                Accessible.role: Accessible.EditableText
                Accessible.name: root.path.split("/").pop() + ", line " + (cursorLine + 1)
                fontFamily: Theme.mono
                fontPixelSize: Theme.mpx(13)
                gutterPixelSize: Theme.mpx(12)
                colors: Theme.codeColors()
                currentMatch: root.currentMatch
                Keys.onShortcutOverride: event => event.accepted = area.claims(event)
                Keys.onPressed: event => area.press(event)
                // Which files have unsaved edits (for the tabs and leaving).
                onModifiedChanged: {
                    if (!area.ready || path !== root.path) return
                    if (modified) root.drafts[path] = true
                    else delete root.drafts[path]
                    root.draftsEdited()
                }
                onTextChanged: if (root.findOpen && area.ready) refind.restart()
                onContextMenuRequested: (x, y) => editMenu.popup(editor, x, y)
            }

            ScrollBar {
                id: vbar
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: hbar.visible ? hbar.top : parent.bottom
                orientation: Qt.Vertical
                size: editor.contentHeight > 0 ? Math.min(1, editor.height / editor.contentHeight) : 1
                position: editor.contentHeight > 0 ? editor.contentY / editor.contentHeight : 0
                policy: size < 1 ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff
                onPositionChanged: if (active) editor.contentY = position * editor.contentHeight
            }
            ScrollBar {
                id: hbar
                anchors.left: parent.left
                anchors.leftMargin: editor.textX
                anchors.right: vbar.visible ? vbar.left : parent.right
                anchors.bottom: parent.bottom
                orientation: Qt.Horizontal
                size: editor.contentWidth > 0 ? Math.min(1, editor.width / editor.contentWidth) : 1
                position: editor.contentWidth > 0 ? editor.contentX / editor.contentWidth : 0
                policy: size < 1 ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff
                visible: policy !== ScrollBar.AlwaysOff
                onPositionChanged: if (active) editor.contentX = position * editor.contentWidth
            }

            AppMenu {
                id: editMenu
                objectName: "editorMenu"
                readonly property bool selected: editor.selectionStart !== editor.selectionEnd
                AppMenuItem { text: "Cut"; hint: "Ctrl+X"; enabled: editMenu.selected; onTriggered: editor.cut() }
                AppMenuItem { text: "Copy"; hint: "Ctrl+C"; enabled: editMenu.selected; onTriggered: editor.copy() }
                AppMenuItem { text: "Paste"; hint: "Ctrl+V"; onTriggered: editor.paste() }
                AppMenuSeparator { }
                AppMenuItem { text: "Select all"; hint: "Ctrl+A"; onTriggered: editor.selectAll() }
                onClosed: editor.forceActiveFocus()
            }
        }
    }
}
