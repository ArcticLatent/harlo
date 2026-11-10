import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// A file open in a tab: always editable, syntax-highlighted, Ctrl+S saves.
// Unsaved text lives in `drafts` (shared with the owner) so it survives
// switching tabs. Ctrl+F finds (Ctrl+H replaces), Ctrl+G goes to a line,
// Ctrl+/ comments lines out; Enter keeps the indentation, Tab and Shift+Tab
// indent and outdent lines, and the bracket next to the cursor is shown
// with its partner.
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

    onPathChanged: { closeBars(); reload() }
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
    /// The text is all in (a long file comes in a slice at a time).
    readonly property bool loaded: editorLoader.item ? editorLoader.item.ready : false
    readonly property bool diskBarVisible: diskNotice !== ""
    readonly property string shownMessage: messageLabel.visible ? messageLabel.text : ""
    readonly property string imageInfo: isImage ? image.status + " " + image.sourceSize.width + "x" + image.sourceSize.height : ""
    function typeText(t) { if (root.editor) root.editor.insert(root.editor.cursorPosition, t) }

    // ---- edits in one undo step ---------------------------------------------
    // `edits`: [[start, end, text], ...] in the text as it is now.
    function apply(edits) { return root.editor ? TextOps.apply(root.editor.textDocument, JSON.stringify(edits)) : false }

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
        if (had && root.editor) root.editor.forceActiveFocus()
    }
    // Finds the query again (after typing it, or an edit); `select` also
    // selects the first match from the cursor and shows it.
    function search(select) {
        const ed = root.editor, q = findField.text
        if (!ed || !findOpen || q === "") { matches = []; currentMatch = -1; return }
        const text = ed.text
        const hay = matchCase ? text : text.toLowerCase(), needle = matchCase ? q : q.toLowerCase()
        const list = []
        for (let i = hay.indexOf(needle); i >= 0 && list.length < matchLimit; i = hay.indexOf(needle, i + needle.length)) list.push(i)
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
        if (editorLoader.item) editorLoader.item.reveal(s)
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

    // ---- the language's comments --------------------------------------------
    // [line marker] or [open, close] for languages with only block comments;
    // [] where it isn't known.
    function commentFor(p) {
        const name = p.split("/").pop().toLowerCase()
        if (/^(makefile|gnumakefile|dockerfile|containerfile|cmakelists\.txt|\.gitignore|\.dockerignore|\.env.*|\.bashrc|\.zshrc|\.profile|pkgbuild|justfile)$/.test(name)) return ["#"]
        const ext = name.indexOf(".") >= 0 ? name.split(".").pop() : ""
        const by = {
            "//": "c h cc cpp cxx hpp hh hxx ino m mm rs go js jsx mjs cjs ts tsx mts cts java kt kts swift cs fs dart scala sc php qml proto zig groovy gradle jsonc json5 glsl frag vert wgsl hlsl sol v vala d",
            "#": "py pyi pyw sh bash zsh fish ksh rb pl pm r yaml yml toml conf cfg ini nix cmake mk tf tfvars hcl ps1 psm1 jl ex exs nim cr coffee properties gitignore env desktop service timer mount socket rake gemspec podspec bzl star",
            "--": "lua sql hs lhs elm ada adb ads",
            ";": "lisp lsp clj cljs cljc edn el scm ss rkt asm s",
            "%": "tex sty cls erl hrl",
            "\"": "vim"
        }
        for (const k in by) if (by[k].split(" ").indexOf(ext) >= 0) return [k]
        if (["html", "htm", "xhtml", "xml", "svg", "md", "markdown", "ui", "qrc", "plist", "xsl", "vue", "svelte"].indexOf(ext) >= 0) return ["<!--", "-->"]
        if (["css", "scss", "less"].indexOf(ext) >= 0) return ext === "css" ? ["/*", "*/"] : ["//"]
        return []
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
                    objectName: "editorLineLabel"
                    visible: !!root.editor
                    text: root.editor ? "Ln " + (root.editor.text.substring(0, root.editor.cursorPosition).split("\n").length) + "  ·  " + root.editor.lineCount + " lines" : ""
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
                // The highlighter, then the text (a long one a slice at a
                // time, each coloured as it comes), then edits are tracked.
                highlight.active = true
                const text = root.drafts[root.path] !== undefined ? root.drafts[root.path] : file.text
                unit = detectIndent(text)
                fill.put(text)
            }
            TextFill {
                id: fill
                editor: editor
                onDone: {
                    const view = root.restoreView && root.restoreView.path === root.path ? root.restoreView : null
                    root.restoreView = null
                    editor.cursorPosition = view ? Math.min(view.cursor, editor.length) : 0
                    area.ready = true
                    if (root.findOpen) root.search(false)
                    editor.forceActiveFocus()
                    if (view) Qt.callLater(() => { flick.contentX = view.x; flick.contentY = Math.min(view.y, Math.max(0, flick.contentHeight - flick.height)) })
                    area.applyLine()
                }
            }
            // The line asked for: the cursor at its start, a third of the
            // way down the view.
            function applyLine() {
                const want = root.pendingLine
                if (!want || want.path !== root.path) return
                root.pendingLine = null
                const text = editor.text
                let at = 0
                for (let n = 1; n < want.line; n++) {
                    const nl = text.indexOf("\n", at)
                    if (nl < 0) break
                    at = nl + 1
                }
                // (Placed once the view is laid out: until then it doesn't
                // follow the cursor.)
                lineScroll.tries = 0
                lineScroll.restart()
                editor.cursorPosition = at
                editor.forceActiveFocus()
            }
            Timer {
                id: lineScroll
                property int tries: 0
                interval: 40
                onTriggered: {
                    if (flick.height <= 0 && ++tries < 25) { restart(); return }
                    flick.contentX = 0
                    const y = editor.cursorRectangle.y
                    flick.contentY = Math.max(0, Math.min(y - flick.height / 3, flick.contentHeight - flick.height))
                    // (Where you arrived: tinted a moment, its number primary.)
                    area.arrivalY = y
                    area.arrival = editor.text.slice(0, editor.cursorPosition).split("\n").length
                    arrivalFade.restart()
                }
            }
            // Shows a position a third of the way down, unless it's in view.
            function reveal(pos) {
                const r = editor.positionToRectangle(pos)
                if (r.y < flick.contentY || r.y + r.height > flick.contentY + flick.height)
                    flick.contentY = Math.max(0, Math.min(r.y - flick.height / 3, flick.contentHeight - flick.height))
                if (r.x < flick.contentX || r.x > flick.contentX + flick.width - 40)
                    flick.contentX = Math.max(0, r.x - flick.width / 3)
            }

            // ---- indentation ----------------------------------------------
            // One level: a tab, or the spaces the file indents by (4 if it
            // doesn't say).
            property string unit: "    "
            readonly property bool colonIndents: /\.(py|pyi|pyw|ya?ml)$/i.test(root.path)
            function detectIndent(text) {
                if (/(^|\/)(gnu)?makefile$|\.mk$/i.test(root.path)) return "\t"
                const lines = text.slice(0, 400000).split("\n")
                let tabs = 0, spaces = 0, prev = 0
                const steps = {}
                for (const l of lines) {
                    if (l.trim() === "") continue
                    if (l[0] === "\t") { tabs++; continue }
                    let n = 0
                    while (l[n] === " ") n++
                    if (n > 0) spaces++
                    if (n > prev) steps[n - prev] = (steps[n - prev] || 0) + 1
                    prev = n
                }
                if (tabs > spaces) return "\t"
                let best = 4, seen = 0
                for (const d of [4, 2, 8, 3]) if ((steps[d] || 0) > seen) { best = d; seen = steps[d] }
                return " ".repeat(best)
            }
            function lineStart(t, pos) { return t.lastIndexOf("\n", pos - 1) + 1 }
            function lineEnd(t, pos) { const e = t.indexOf("\n", pos); return e < 0 ? t.length : e }
            readonly property var pairs: ({ "(": ")", "[": "]", "{": "}", ")": "(", "]": "[", "}": "{" })

            // Enter: the new line keeps this one's indentation, one level
            // more after an opening bracket (or a colon in Python and YAML);
            // between a pair of brackets the closing one gets a line of its
            // own.
            function newLine() {
                const t = editor.text, a = editor.selectionStart, b = editor.selectionEnd
                const ls = lineStart(t, a), before = t.slice(ls, a)
                const indent = before.match(/^[ \t]*/)[0]
                const last = before.replace(/\s+$/, "").slice(-1)
                const opens = last === "(" || last === "[" || last === "{"
                const deeper = opens || (last === ":" && colonIndents)
                // (No spaces left at the end of the line you leave.)
                const from = before.trim() === "" ? ls : a - (before.length - before.replace(/\s+$/, "").length)
                let ins = "\n" + indent + (deeper ? unit : "")
                const caret = from + ins.length
                if (opens && t[b] === pairs[last]) ins += "\n" + indent
                root.apply([[from, b, ins]])
                editor.cursorPosition = caret
            }
            // Tab: a level of indentation at the cursor, or on every line
            // a selection spans.
            function tab() {
                const t = editor.text, a = editor.selectionStart, b = editor.selectionEnd
                if (a !== b && t.slice(a, b).indexOf("\n") >= 0) { shiftLines(true); return }
                let ins = unit
                if (unit !== "\t") {
                    const col = a - lineStart(t, a)
                    ins = " ".repeat(unit.length - col % unit.length)
                }
                root.apply([[a, b, ins]])
                editor.cursorPosition = a + ins.length
            }
            // Indents (or outdents) the lines the cursor or selection is on.
            function shiftLines(indent) {
                const t = editor.text, a = editor.selectionStart, had = a !== editor.selectionEnd
                let b = editor.selectionEnd
                if (had && t[b - 1] === "\n") b--
                const first = lineStart(t, a), last = lineEnd(t, b)
                const width = unit === "\t" ? 4 : unit.length
                const old = t.slice(first, last)
                const lines = old.split("\n").map(l => {
                    if (indent) return l.trim() === "" ? l : unit + l
                    if (l[0] === "\t") return l.slice(1)
                    let n = 0
                    while (n < width && l[n] === " ") n++
                    return l.slice(n)
                })
                const joined = lines.join("\n")
                if (joined === old) return
                const cursor = editor.cursorPosition
                root.apply([[first, last, joined]])
                if (had) editor.select(first, first + joined.length)
                else editor.cursorPosition = Math.max(first, cursor + joined.length - old.length)
            }
            // Backspace in spaces before a line's text: back a level.
            function backspace() {
                const t = editor.text, a = editor.selectionStart
                if (unit === "\t" || a !== editor.selectionEnd) return false
                const before = t.slice(lineStart(t, a), a)
                if (before === "" || before.trim() !== "" || before.indexOf("\t") >= 0) return false
                const n = (before.length - 1) % unit.length + 1
                if (n < 2) return false
                root.apply([[a - n, a, ""]])
                return true
            }
            // A closing bracket typed on a line of only spaces: lined up
            // with the line its partner opened.
            function closer(ch) {
                const t = editor.text, a = editor.selectionStart
                if (a !== editor.selectionEnd) return false
                const ls = lineStart(t, a), before = t.slice(ls, a)
                if (before === "" || before.trim() !== "") return false
                let depth = 0, at = -1
                for (let i = a - 1, n = 0; i >= 0 && n < 200000; i--, n++) {
                    if (t[i] === ch) depth++
                    else if (t[i] === pairs[ch]) { if (depth === 0) { at = i; break } depth-- }
                }
                let indent
                if (at >= 0) { const os = lineStart(t, at); indent = t.slice(os, at).match(/^[ \t]*/)[0] }
                else indent = before.endsWith(unit) ? before.slice(0, before.length - unit.length) : before
                if (indent === before) return false
                root.apply([[ls, a, indent + ch]])
                editor.cursorPosition = ls + indent.length + 1
                return true
            }

            // Ctrl+/: comments the lines out, or back in when they all are.
            function toggleComment() {
                const marks = root.commentFor(root.path)
                if (marks.length === 0) return
                const open = marks[0], close = marks.length > 1 ? marks[1] : ""
                const t = editor.text, a = editor.selectionStart, had = a !== editor.selectionEnd
                let b = editor.selectionEnd
                if (had && t[b - 1] === "\n") b--
                const first = lineStart(t, a), last = lineEnd(t, b)
                const old = t.slice(first, last), lines = old.split("\n")
                const used = lines.filter(l => l.trim() !== "")
                if (used.length === 0) return
                const commented = l => { const x = l.trim(); return x.startsWith(open) && (close === "" || x.endsWith(close)) }
                const remove = used.every(commented)
                const indent = Math.min(...used.map(l => l.match(/^\s*/)[0].length))
                const out = lines.map(l => {
                    if (l.trim() === "") return l
                    if (!remove) return l.slice(0, indent) + open + " " + l.slice(indent) + (close !== "" ? " " + close : "")
                    const i = l.indexOf(open)
                    let rest = l.slice(i + open.length)
                    if (rest[0] === " ") rest = rest.slice(1)
                    if (close !== "") {
                        const j = rest.lastIndexOf(close)
                        rest = rest.slice(0, j).replace(/ $/, "") + rest.slice(j + close.length)
                    }
                    return l.slice(0, i) + rest
                })
                const joined = out.join("\n")
                const cursor = editor.cursorPosition
                root.apply([[first, last, joined]])
                if (had) editor.select(first, first + joined.length)
                else editor.cursorPosition = Math.max(first, Math.min(first + joined.length, cursor + joined.length - old.length))
            }

            // Keys the editor keeps from the app's shortcuts while it has
            // the focus.
            function claims(e) {
                const ctrl = e.modifiers & Qt.ControlModifier, alt = e.modifiers & Qt.AltModifier
                return (ctrl && !alt && (e.key === Qt.Key_Slash || e.key === Qt.Key_H || e.key === Qt.Key_G))
                    || e.key === Qt.Key_F3 || (e.key === Qt.Key_Escape && (root.findOpen || root.lineOpen))
            }
            function press(e) {
                const ctrl = e.modifiers & Qt.ControlModifier, alt = e.modifiers & Qt.AltModifier, shift = e.modifiers & Qt.ShiftModifier
                let done = true
                if (ctrl && !alt && e.key === Qt.Key_Slash) toggleComment()
                else if (ctrl && !alt && e.key === Qt.Key_H) root.openFind(true)
                else if (ctrl && !alt && e.key === Qt.Key_G) root.openGoToLine()
                else if (e.key === Qt.Key_F3) root.findStep(shift ? -1 : 1)
                else if (e.key === Qt.Key_Escape && (root.findOpen || root.lineOpen)) root.closeBars()
                else if ((e.key === Qt.Key_Return || e.key === Qt.Key_Enter) && !ctrl && !alt) newLine()
                else if (e.key === Qt.Key_Tab && !ctrl && !alt && !shift) tab()
                else if (e.key === Qt.Key_Backtab && !ctrl && !alt) shiftLines(false)
                else if (e.key === Qt.Key_Backspace && !ctrl && !alt && !shift) done = backspace()
                else if (!ctrl && !alt && (e.text === "}" || e.text === "]" || e.text === ")")) done = closer(e.text)
                else done = false
                e.accepted = done
            }

            // ---- marks: the matches in view, the bracket pair --------------
            property var hitRects: []
            property var bracketRects: []
            function placeHits() {
                const list = root.matches, len = root.matchLength
                if (!root.findOpen || list.length === 0) { hitRects = []; return }
                const top = editor.positionAt(0, Math.max(0, flick.contentY - area.lineHeight))
                const bottom = editor.positionAt(editor.width, flick.contentY + flick.height + area.lineHeight)
                let lo = 0, hi = list.length
                while (lo < hi) { const m = (lo + hi) >> 1; if (list[m] + len < top) lo = m + 1; else hi = m }
                const out = []
                for (let i = lo; i < list.length && list[i] <= bottom && out.length < 400; i++) {
                    const r = editor.positionToRectangle(list[i]), e = editor.positionToRectangle(list[i] + len)
                    out.push({ x: r.x, y: r.y, w: Math.max(2, e.y === r.y ? e.x - r.x : editor.width - r.x), h: r.height, current: i === root.currentMatch })
                }
                hitRects = out
            }
            function placeBrackets() {
                const t = editor.text, c = editor.cursorPosition
                let p = -1
                if (editor.selectionStart === editor.selectionEnd) {
                    if ("([{".indexOf(t[c]) >= 0 && t[c] !== undefined) p = c
                    else if (c > 0 && ")]}".indexOf(t[c - 1]) >= 0) p = c - 1
                    else if (")]}".indexOf(t[c]) >= 0 && t[c] !== undefined) p = c
                    else if (c > 0 && "([{".indexOf(t[c - 1]) >= 0) p = c - 1
                }
                const q = p >= 0 ? partner(t, p) : -1
                if (q < 0) { bracketRects = []; return }
                bracketRects = [p, q].map(i => { const r = editor.positionToRectangle(i), e = editor.positionToRectangle(i + 1); return { x: r.x, y: r.y, w: Math.max(4, e.x - r.x), h: r.height } })
            }
            function partner(t, p) {
                const ch = t[p], other = pairs[ch], step = "([{".indexOf(ch) >= 0 ? 1 : -1
                let depth = 0
                for (let i = p, n = 0; i >= 0 && i < t.length && n < 200000; i += step, n++) {
                    if (t[i] === ch) depth++
                    else if (t[i] === other && --depth === 0) return i
                }
                return -1
            }
            Connections {
                target: root
                function onMatchesChanged() { Qt.callLater(area.placeHits) }
                function onCurrentMatchChanged() { Qt.callLater(area.placeHits) }
                function onFindOpenChanged() { Qt.callLater(area.placeHits) }
            }
            Connections { target: flick; function onContentYChanged() { Qt.callLater(area.placeHits) } function onHeightChanged() { Qt.callLater(area.placeHits) } }

            property int arrival: 0
            property real arrivalY: 0
            property real arrivalGlow: 0
            SequentialAnimation {
                id: arrivalFade
                PropertyAction { target: area; property: "arrivalGlow"; value: 1 }
                PauseAnimation { duration: 1100 }
                NumberAnimation { target: area; property: "arrivalGlow"; to: 0; duration: Math.max(1, Theme.motion * 2.5); easing.type: Easing.OutCubic }
                PropertyAction { target: area; property: "arrival"; value: 0 }
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
                // (Set, not bound: a list's originY moves with its contentY,
                // so a binding on both loops.)
                function follow() { contentY = flick.contentY + originY }
                Connections { target: flick; function onContentYChanged() { gutter.follow() } }
                onCountChanged: Qt.callLater(follow)
                Component.onCompleted: follow()
                delegate: Label {
                    required property int index
                    width: gutter.width - 10
                    height: area.lineHeight
                    text: index + 1
                    horizontalAlignment: Text.AlignRight
                    verticalAlignment: Text.AlignVCenter
                    readonly property bool arrived: area.arrival === index + 1
                    color: arrived ? Theme.primary : Theme.muted
                    opacity: arrived ? 0.55 + 0.45 * area.arrivalGlow : 0.55
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
                    if (lineScroll.running) return   // (a line asked for: placed once laid out)
                    if (contentX >= r.x) contentX = r.x
                    else if (contentX + width <= r.x + r.width + 20) contentX = r.x + r.width + 20 - width
                    if (contentY >= r.y) contentY = r.y
                    else if (contentY + height <= r.y + r.height + 20) contentY = r.y + r.height + 20 - height
                }

                // The line you arrived at (a listing's line number, clicked).
                Rectangle {
                    objectName: "arrivalLine"
                    visible: area.arrivalGlow > 0
                    y: area.arrivalY
                    width: flick.contentWidth
                    height: area.lineHeight
                    color: Qt.rgba(Theme.primary.r, Theme.primary.g, Theme.primary.b, 0.16)
                    opacity: area.arrivalGlow
                }
                // Where the query is found (the current one stronger).
                Repeater {
                    model: area.hitRects
                    Rectangle {
                        required property var modelData
                        objectName: "findHit"
                        x: modelData.x - 1
                        y: modelData.y
                        width: modelData.w + 2
                        height: modelData.h
                        radius: 3
                        color: Qt.rgba(Theme.warning.r, Theme.warning.g, Theme.warning.b, modelData.current ? (Theme.isLight ? 0.3 : 0.4) : (Theme.isLight ? 0.24 : 0.2))
                        border.width: modelData.current ? 1 : 0
                        border.color: Theme.warning
                    }
                }
                // The bracket by the cursor and its partner.
                Repeater {
                    model: area.bracketRects
                    Rectangle {
                        required property var modelData
                        objectName: "bracketMark"
                        x: modelData.x
                        y: modelData.y
                        width: modelData.w
                        height: modelData.h
                        radius: 2
                        color: Qt.rgba(Theme.primary.r, Theme.primary.g, Theme.primary.b, 0.14)
                        border.width: 1
                        border.color: Qt.rgba(Theme.primary.r, Theme.primary.g, Theme.primary.b, 0.6)
                    }
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
                    // (While a long file is still coming in.)
                    readOnly: !area.ready
                    tabStopDistance: 4 * metrics.averageCharacterWidth
                    FontMetrics { id: metrics; font: editor.font }
                    onCursorRectangleChanged: flick.ensureVisible(cursorRectangle)
                    onCursorPositionChanged: Qt.callLater(area.placeBrackets)
                    onSelectionEndChanged: Qt.callLater(area.placeBrackets)
                    Keys.onShortcutOverride: event => event.accepted = area.claims(event)
                    Keys.onPressed: event => area.press(event)
                    onTextChanged: {
                        Qt.callLater(area.placeBrackets)
                        if (root.findOpen) refind.restart()
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
