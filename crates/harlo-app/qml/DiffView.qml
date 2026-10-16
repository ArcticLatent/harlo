import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// A file's changes side by side (the diff tab): the
// old version on the left, the new one on the right, scrolling together.
// Removed lines are red, added ones green, the changed part of a line
// stronger; hatched rows stand in for lines the other side doesn't have.
// The strip on the right maps every change; the buttons between the sides
// stage, unstage or discard a hunk. It reloads as the file changes.
Rectangle {
    id: root
    required property string path      // absolute; "" when no diff tab is active
    property bool staged: false
    signal openFile(string path)
    /// Something was staged, unstaged or discarded here.
    signal repoChanged()
    /// Review comments: where they're kept (ChatView's, by repository), and
    /// who gets them (the session the diff is for).
    property QtObject review: null
    property string sessionTitle: ""
    property bool canSend: false
    signal sendReview(string message)
    color: Theme.background

    // ---- review comments ---------------------------------------------------
    // A comment on a line: click its number (the new side, or the old side
    // for a removed line); a mark shows on commented lines; the bar at the
    // bottom sends them all to the agent as one message.
    readonly property string relPath: dg.root !== "" && path.startsWith(dg.root + "/") ? path.substring(dg.root.length + 1) : fileName
    readonly property var comments: review && dg.root !== "" ? (review.all[dg.root] || []) : []
    function commentAt(side, line) { return comments.findIndex(c => c.path === relPath && c.side === side && c.line === line) }
    function openComment(isLeft, row) {
        const r = rows[row]
        if (!r || !review) return
        const kind = r[isLeft ? 0 : 5], line = r[isLeft ? 1 : 6]
        if (kind >= 2 || !(line > 0)) return
        const side = isLeft ? "old" : "new"
        const code = ((isLeft ? view.left : view.right).split("\n")[row] || "").trim()
        const i = commentAt(side, line)
        commentBox.edit(side, line, code, i >= 0 ? comments[i].text : "", row, isLeft)
    }
    function reviewMessage() {
        const lines = ["Review comments on your changes. Please address each one:", ""]
        for (const c of comments) {
            lines.push("- `" + c.path + ":" + c.line + "`" + (c.side === "old" ? " (a line you removed)" : "") + (c.code !== "" ? ": `" + c.code.replace(/`/g, "'") + "`" : ""))
            for (const t of c.text.split("\n")) lines.push("  " + t)
        }
        return lines.join("\n")
    }
    Popup {
        id: commentBox
        objectName: "commentBox"
        property string side: "new"
        property int line: 0
        property string code: ""
        property bool existing: false
        function edit(s, l, c, text, row, isLeft) {
            side = s
            line = l
            code = c
            existing = text !== ""
            commentText.text = text
            const pane = isLeft ? leftPane : rightPane
            x = Math.min(pane.x + 40, root.width - width - 12)
            y = Math.min(Math.max(8, sides.y + 8 + (row + 1) * root.lineHeight - pane.flick.contentY + 4), root.height - height - 12)
            open()
            commentText.forceActiveFocus()
        }
        function save() {
            if (!root.review) return
            const t = commentText.text.trim()
            const i = root.commentAt(side, line)
            if (t === "") { if (i >= 0) root.review.remove(dg.root, i) }
            else root.review.put(dg.root, i, { path: root.relPath, side: side, line: line, code: code, text: t })
            close()
        }
        width: Math.min(440, root.width - 24)
        padding: 12
        modal: false
        focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: GlassSurface { radius: 12 }
        contentItem: ColumnLayout {
            spacing: 8
            Label {
                Layout.fillWidth: true
                text: root.relPath + ":" + commentBox.line + (commentBox.side === "old" ? " (removed)" : "")
                color: Theme.textMuted
                elide: Text.ElideMiddle
                font.family: Theme.mono
                font.pixelSize: Theme.mpx(11.5)
            }
            Label {
                visible: commentBox.code !== ""
                Layout.fillWidth: true
                text: commentBox.code
                elide: Text.ElideRight
                color: Theme.text
                font.family: Theme.mono
                font.pixelSize: Theme.mpx(12)
            }
            ScrollView {
                Layout.fillWidth: true
                Layout.preferredHeight: 84
                TextArea {
                    id: commentText
                    objectName: "commentText"
                    wrapMode: TextEdit.Wrap
                    color: Theme.text
                    placeholderText: "What should the agent change here? (Ctrl+Enter)"
                    placeholderTextColor: Theme.textFaint
                    font.pixelSize: Theme.px(13)
                    background: Rectangle { radius: 8; color: Theme.background; border.color: commentText.activeFocus ? Theme.accent : Theme.border }
                    Keys.onPressed: event => {
                        if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && (event.modifiers & Qt.ControlModifier)) {
                            commentBox.save()
                            event.accepted = true
                        }
                    }
                }
            }
            RowLayout {
                spacing: 8
                FlatButton {
                    visible: commentBox.existing
                    text: "Delete"
                    accent: Theme.error
                    outlined: false
                    font.pixelSize: Theme.px(12)
                    onClicked: { commentText.text = ""; commentBox.save() }
                }
                Item { Layout.fillWidth: true }
                FlatButton { text: "Cancel"; outlined: false; font.pixelSize: Theme.px(12); onClicked: commentBox.close() }
                FlatButton {
                    objectName: "commentSave"
                    text: commentBox.existing ? "Save" : "Comment"
                    variant: "primary"
                    font.pixelSize: Theme.px(12)
                    enabled: commentText.text.trim() !== "" || commentBox.existing
                    onClicked: commentBox.save()
                }
            }
        }
    }

    // Its own: it follows the file's repository, not the session's.
    GitPanel { id: dg; splitMode: true; onFilesChanged: root.repoChanged() }
    // Test hooks.
    property alias git: dg
    property alias leftPane: leftPane
    property alias rightPane: rightPane
    property alias commentBox: commentBox

    onPathChanged: load()
    onStagedChanged: load()
    Component.onCompleted: load()
    function load() {
        jumped = false
        expanded = ({})
        if (path !== "") dg.showFile(path, staged)
    }
    readonly property bool remote: path.startsWith("ssh://")
    // Agents edit files under the view: look again now and then.
    Timer {
        interval: root.remote ? 6000 : 2500
        repeat: true
        running: root.visible && root.path !== "" && !dg.busy
        onTriggered: dg.refresh(true)
    }

    readonly property var info: dg.splitJson !== "" ? JSON.parse(dg.splitJson) : null
    // The file's rows as git gave them; `rows` / `hunks` are what's shown.
    readonly property var allRows: info ? info.rows : []

    // ---- collapsing unchanged regions -----------------------------------
    // Only the changes and a few lines around them; each longer unchanged
    // stretch is a fold row (kind 3: `[3, hidden, offset, -1, -1, 3, …]`)
    // that opens on a click. Remembered across files and runs.
    property bool collapse: Workspace.setting("diff_collapse") === "1"
    function setCollapse(on) {
        Workspace.setSetting("diff_collapse", on ? "1" : "0")
        collapse = on
        jumped = false
    }
    readonly property int context: 3
    // Folds opened by hand (by their first row; a fold row's [10]).
    property var expanded: ({})
    readonly property var view: {
        const src = allRows, left = dg.splitLeft, right = dg.splitRight
        if (!collapse || src.length === 0)
            return { rows: src, left: left, right: right, index: null }
        // Rows within `context` of a change stay.
        const keep = new Array(src.length).fill(false)
        for (let i = 0; i < src.length; i++) {
            if (src[i][0] === 0 && src[i][5] === 0) continue
            for (let j = Math.max(0, i - context); j <= Math.min(src.length - 1, i + context); j++) keep[j] = true
        }
        const lLines = left.split("\n"), rLines = right.split("\n")
        const rows = [], lOut = [], rOut = [], index = new Array(src.length)
        let lOff = 0, rOff = 0
        const add = (row, l, r) => {
            const out = row.slice()
            out[2] = lOff
            out[7] = rOff
            rows.push(out)
            lOut.push(l)
            rOut.push(r)
            lOff += l.length + 1
            rOff += r.length + 1
        }
        for (let i = 0; i < src.length;) {
            let j = i
            while (j < src.length && !keep[j]) j++
            // (A stretch too short to bother folding stays.)
            if (j - i >= 3 && !expanded[i]) {
                add([3, j - i, 0, -1, -1, 3, j - i, 0, -1, -1, i], "", "")
                for (let k = i; k < j; k++) index[k] = rows.length - 1
                i = j
                continue
            }
            const end = Math.max(j, i + 1)
            for (; i < end && i < src.length; i++) {
                index[i] = rows.length
                add(src[i], lLines[i] || "", rLines[i] || "")
            }
        }
        return { rows: rows, left: lOut.join("\n"), right: rOut.join("\n"), index: index }
    }
    readonly property var rows: view.rows
    readonly property var hunks: info ? (view.index ? info.hunks.map(h => [h[0], view.index[h[1]]]) : info.hunks) : []
    readonly property var folds: rows.reduce((out, r, i) => { if (r[0] === 3) out.push({ row: i, count: r[1], start: r[10] }); return out }, [])
    function openFold(start) {
        const e = Object.assign({}, expanded)
        e[start] = true
        expanded = e
    }
    readonly property string note: info ? info.note : ""
    readonly property string fileName: path.split("/").pop()
    readonly property string folder: dg.root !== "" && path.startsWith(dg.root + "/") ? path.substring(dg.root.length + 1, path.length - fileName.length - 1) : ""
    // Runs of changed rows: { start, end (exclusive), left, right }.
    readonly property var blocks: {
        const out = []
        let cur = null
        for (let i = 0; i < rows.length; i++) {
            const r = rows[i], l = r[0] === 1 || r[0] === 2, rt = r[5] === 1 || r[5] === 2
            if (l || rt) {
                if (!cur) { cur = { start: i, end: i + 1, left: false, right: false }; out.push(cur) }
                cur.end = i + 1
                cur.left = cur.left || r[0] === 1
                cur.right = cur.right || r[5] === 1
            } else {
                cur = null
            }
        }
        return out
    }
    readonly property real lineHeight: leftPane.lineHeight
    // The first time a file's changes show, they're scrolled into view.
    property bool jumped: false
    onBlocksChanged: if (!jumped && blocks.length > 0 && leftPane.ready && rightPane.ready) { jumped = true; Qt.callLater(() => showRow(blocks[0].start)) }

    function topRow() { return Math.floor(rightPane.flick.contentY / lineHeight) }
    function showRow(row) {
        const f = rightPane.flick
        f.contentY = Math.max(0, Math.min(f.contentHeight - f.height, row * lineHeight - f.height / 3))
    }
    function nextChange(dir) {
        if (blocks.length === 0) return
        // The change a third of the way down is the "current" one.
        const here = Math.floor((rightPane.flick.contentY + rightPane.flick.height / 3) / lineHeight)
        let target = null
        if (dir > 0) target = blocks.find(b => b.start > here) || blocks[0]
        else target = blocks.slice().reverse().find(b => b.start < here) || blocks[blocks.length - 1]
        showRow(target.start)
    }

    AppDialog {
        id: confirmDiscard
        property int hunk: -1
        title: hunk >= 0 ? "Discard this change?" : "Discard the changes to " + root.fileName + "?"
        modal: true
        anchors.centerIn: Overlay.overlay
        standardButtons: Dialog.Discard | Dialog.Cancel
        Label {
            width: 340
            wrapMode: Text.Wrap
            text: "This cannot be undone from harlo. Untracked files are moved to the trash."
            color: Theme.muted
        }
        onDiscarded: { hunk >= 0 ? dg.revertHunk(hunk) : dg.revertFile(dg.selectedPath); close() }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // ---- toolbar ------------------------------------------------------
        // (In an Item: however much it holds, it doesn't widen the view;
        // narrow, the less needed parts go.)
        Item {
        Layout.fillWidth: true
        implicitHeight: toolbar.implicitHeight + 12
        clip: true
        readonly property bool narrow: width < 640
        readonly property bool tight: width < 440
        RowLayout {
            id: toolbar
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 8
            spacing: 8
            FileIcon { path: root.fileName; Layout.preferredWidth: 16; Layout.preferredHeight: 16; sourceSize: Qt.size(16, 16) }
            Label { text: root.fileName; color: Theme.foreground; font.pixelSize: Theme.px(13); font.bold: true; elide: Text.ElideRight; Layout.minimumWidth: 0; Layout.maximumWidth: implicitWidth }
            Label {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                // (Narrow: empty, but still the spring.)
                text: toolbar.parent.narrow ? "" : root.folder
                color: Theme.muted
                font.pixelSize: Theme.px(12)
                elide: Text.ElideLeft
            }
            Rectangle {
                visible: !toolbar.parent.narrow
                implicitWidth: sideLabel.implicitWidth + 12
                implicitHeight: 18
                radius: 9
                color: Theme.subtle
                border.color: Theme.border
                Label { id: sideLabel; anchors.centerIn: parent; text: root.staged ? "Staged" : "Working Tree"; color: Theme.muted; font.pixelSize: Theme.px(10.5) }
                HoverHandler { id: sideHover }
                ToolTip.visible: sideHover.hovered
                ToolTip.text: root.staged ? "What's staged (right) against the last commit (left)" : "The file as it is (right) against what's staged or committed (left)"
            }
            Label {
                objectName: "diffStats"
                text: dg.diffStats
                color: Theme.muted
                font.family: Theme.mono
                font.pixelSize: Theme.mpx(11.5)
            }
            Label {
                visible: root.blocks.length > 0 && !toolbar.parent.narrow
                text: root.blocks.length === 1 ? "1 change" : root.blocks.length + " changes"
                color: Theme.muted
                font.pixelSize: Theme.px(11.5)
            }
            FlatButton {
                objectName: "diffPrev"
                enabled: root.blocks.length > 0
                text: "↑"; outlined: false; padding: 3; font.pixelSize: Theme.px(13)
                ToolTip.visible: hovered; ToolTip.text: "Previous change (Alt+↑)"
                onClicked: root.nextChange(-1)
            }
            FlatButton {
                objectName: "diffNext"
                enabled: root.blocks.length > 0
                text: "↓"; outlined: false; padding: 3; font.pixelSize: Theme.px(13)
                ToolTip.visible: hovered; ToolTip.text: "Next change (Alt+↓)"
                onClicked: root.nextChange(1)
            }
            FlatButton {
                objectName: "diffCollapse"
                text: "⇕"; outlined: root.collapse; padding: 3; font.pixelSize: Theme.px(13)
                accent: root.collapse ? Theme.primary : Theme.foreground
                ToolTip.visible: hovered
                ToolTip.text: root.collapse ? "Show unchanged regions" : "Collapse unchanged regions (show only the changes and the lines around them)"
                onClicked: root.setCollapse(!root.collapse)
            }
            Rectangle { implicitWidth: 1; implicitHeight: 16; color: Theme.border }
            FlatButton {
                text: toolbar.parent.tight ? "↗" : toolbar.parent.narrow ? "Open" : "Open file"; font.pixelSize: Theme.px(11.5); padding: 4
                ToolTip.visible: hovered; ToolTip.text: "Open it in the editor"
                onClicked: root.openFile(root.path)
            }
            FlatButton {
                objectName: "diffDiscardFile"
                visible: !root.staged && dg.selectedPath !== ""
                text: toolbar.parent.tight ? "↺" : "Discard"; accent: Theme.error; font.pixelSize: Theme.px(11.5); padding: 4
                ToolTip.visible: hovered; ToolTip.text: "Discard all the changes to this file"
                onClicked: { confirmDiscard.hunk = -1; confirmDiscard.open() }
            }
            FlatButton {
                objectName: "diffStageFile"
                visible: dg.selectedPath !== ""
                text: toolbar.parent.tight ? (root.staged ? "−" : "+") : root.staged ? "Unstage" : "Stage"; font.pixelSize: Theme.px(11.5); padding: 4
                ToolTip.visible: hovered; ToolTip.text: root.staged ? "Unstage the whole file" : "Stage the whole file"
                onClicked: root.staged ? dg.unstageFile(dg.selectedPath) : dg.stageFile(dg.selectedPath)
            }
        }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.border }

        // ---- nothing to compare ------------------------------------------
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.rows.length === 0
            spacing: 10
            Item { Layout.fillHeight: true }
            Label {
                objectName: "diffNote"
                Layout.alignment: Qt.AlignHCenter
                text: root.note !== "" ? root.note
                    : dg.busy || (root.info === null && dg.selectedPath !== "") ? "Loading…"
                    : root.staged ? "No staged changes in this file." : "No changes in this file (any left may be staged)."
                color: Theme.muted
                font.pixelSize: Theme.px(13)
            }
            FlatButton {
                Layout.alignment: Qt.AlignHCenter
                visible: root.info === null && !dg.busy && dg.selectedPath === ""
                text: root.staged ? "Show the working tree" : "Show what's staged"
                onClicked: Workspace.openDiffTab(root.path, !root.staged)
            }
            Item { Layout.fillHeight: true }
        }

        // ---- the two sides --------------------------------------------------
        Item {
            id: sides
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.rows.length > 0
            focus: true
            Keys.onPressed: event => {
                if ((event.modifiers & Qt.AltModifier) && (event.key === Qt.Key_Down || event.key === Qt.Key_Up)) {
                    root.nextChange(event.key === Qt.Key_Down ? 1 : -1)
                    event.accepted = true
                }
            }
            readonly property int middle: 30
            readonly property int ruler: 14

            DiffPane {
                id: leftPane
                isLeft: true
                text: root.view.left
                x: 0
                width: (sides.width - sides.middle - sides.ruler) / 2
                height: sides.height
                onScrolled: y => { if (rightPane.flick.contentY !== y) rightPane.flick.contentY = y }
            }
            // Between the sides: each hunk's actions, by its first change.
            Item {
                id: middle
                x: leftPane.width
                width: sides.middle
                height: sides.height
                clip: true
                Rectangle { anchors.horizontalCenter: parent.horizontalCenter; width: 1; height: parent.height; color: Theme.border }
                Repeater {
                    model: root.hunks
                    Column {
                        id: hunkButtons
                        required property var modelData
                        readonly property int hunk: modelData[0]
                        readonly property real rowY: 8 + modelData[1] * root.lineHeight - rightPane.flick.contentY
                        x: (middle.width - width) / 2
                        y: rowY
                        visible: rowY > -40 && rowY < middle.height
                        spacing: 2
                        Repeater {
                            model: root.staged ? [["−", "Unstage this change", "unstage"]] : [["+", "Stage this change", "stage"], ["↺", "Discard this change", "discard"]]
                            Rectangle {
                                id: hunkButton
                                required property var modelData
                                objectName: "hunk-" + modelData[2] + "-" + hunkButtons.hunk
                                width: 20
                                height: 18
                                radius: 4
                                color: hunkHover.hovered ? Theme.hover : Theme.elevated
                                border.color: Theme.border
                                Label {
                                    anchors.centerIn: parent
                                    text: hunkButton.modelData[0]
                                    color: hunkButton.modelData[2] === "discard" ? Theme.error : Theme.foreground
                                    font.pixelSize: Theme.px(12)
                                }
                                HoverHandler { id: hunkHover }
                                ToolTip.visible: hunkHover.hovered
                                ToolTip.text: hunkButton.modelData[1]
                                TapHandler {
                                    onTapped: {
                                        const what = hunkButton.modelData[2]
                                        if (what === "stage") dg.stageHunk(hunkButtons.hunk)
                                        else if (what === "unstage") dg.unstageHunk(hunkButtons.hunk)
                                        else { confirmDiscard.hunk = hunkButtons.hunk; confirmDiscard.open() }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            DiffPane {
                id: rightPane
                isLeft: false
                text: root.view.right
                x: leftPane.width + sides.middle
                width: leftPane.width
                height: sides.height
                onScrolled: y => { if (leftPane.flick.contentY !== y) leftPane.flick.contentY = y }
            }
            // The map of changes: click or drag to go there.
            Item {
                id: ruler
                objectName: "diffRuler"
                x: sides.width - sides.ruler
                width: sides.ruler
                height: sides.height
                Rectangle { width: 1; height: parent.height; color: Theme.border }
                readonly property real rowScale: root.rows.length > 0 ? height / root.rows.length : 0
                Repeater {
                    model: root.blocks
                    Item {
                        required property var modelData
                        x: 2
                        y: modelData.start * ruler.rowScale
                        width: ruler.width - 3
                        height: Math.max(2, (modelData.end - modelData.start) * ruler.rowScale)
                        Rectangle { width: parent.width / 2; height: parent.height; color: Theme.error; opacity: 0.8; visible: parent.modelData.left }
                        Rectangle { x: parent.width / 2; width: parent.width / 2; height: parent.height; color: Theme.success; opacity: 0.8; visible: parent.modelData.right }
                    }
                }
                // Where the view is.
                Rectangle {
                    readonly property var f: rightPane.flick
                    x: 1
                    width: parent.width - 1
                    y: f.contentHeight > 0 ? f.contentY / f.contentHeight * parent.height : 0
                    height: f.contentHeight > 0 ? Math.max(8, f.height / f.contentHeight * parent.height) : 0
                    color: Theme.foreground
                    opacity: rulerHover.hovered ? 0.14 : 0.08
                }
                HoverHandler { id: rulerHover }
                MouseArea {
                    anchors.fill: parent
                    function go(y) {
                        const f = rightPane.flick
                        f.contentY = Math.max(0, Math.min(f.contentHeight - f.height, y / height * f.contentHeight - f.height / 2))
                    }
                    onPressed: mouse => go(mouse.y)
                    onPositionChanged: mouse => { if (pressed) go(mouse.y) }
                }
            }
        }
        // ---- review comments: send them to the agent ------------------------
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.hairline; visible: reviewBar.visible }
        RowLayout {
            id: reviewBar
            objectName: "reviewBar"
            readonly property int files: [...new Set(root.comments.map(c => c.path))].length
            visible: root.comments.length > 0
            Layout.fillWidth: true
            Layout.margins: 10
            Layout.leftMargin: 14
            spacing: 10
            Rectangle {
                implicitWidth: 22
                implicitHeight: 22
                radius: 11
                color: Theme.accentSoft
                Label { anchors.centerIn: parent; text: root.comments.length; color: Theme.accent; font.pixelSize: Theme.px(11.5); font.weight: Font.DemiBold }
            }
            Label {
                objectName: "reviewSummary"
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                elide: Text.ElideRight
                text: (root.comments.length === 1 ? "1 comment" : root.comments.length + " comments") + " on " + (reviewBar.files === 1 ? "1 file" : reviewBar.files + " files")
                      + (root.canSend ? " · for " + root.sessionTitle : " · open a session in this folder to send them")
                color: Theme.text
                font.pixelSize: Theme.px(13)
            }
            FlatButton {
                text: "Clear"
                outlined: false
                accent: Theme.textMuted
                font.pixelSize: Theme.px(12)
                onClicked: root.review.clear(dg.root)
            }
            FlatButton {
                objectName: "reviewSend"
                text: "Send to the agent"
                variant: "primary"
                enabled: root.canSend
                font.pixelSize: Theme.px(12.5)
                // (The message first, then cleared: sending opens the session.)
                onClicked: {
                    const message = root.reviewMessage()
                    root.review.clear(dg.root)
                    root.sendReview(message)
                }
            }
        }
    }

    // One side: line numbers, row colours, the text (read-only, selectable,
    // syntax-highlighted).
    component DiffPane: Item {
        id: pane
        property bool isLeft: true
        property string text: ""
        signal scrolled(real y)
        property alias flick: flick
        property alias editor: editor
        readonly property real lineHeight: editor.lineCount > 0 ? editor.contentHeight / editor.lineCount : 16
        readonly property int base: isLeft ? 0 : 5
        readonly property color tint: isLeft ? Theme.error : Theme.success
        readonly property int gutterWidth: 58
        readonly property bool ready: editor.text === text
        // Test hook: how far the row colours are scrolled.
        function rowsAt() { return rowsView.contentY - rowsView.originY }
        clip: true

        onTextChanged: {
            // Same place after a reload (the file changed under the view).
            const x = flick.contentX, y = flick.contentY
            editor.text = text
            flick.contentX = x
            flick.contentY = Math.min(y, Math.max(0, flick.contentHeight - flick.height))
            if (!highlight.active && text !== "") highlight.active = true
        }
        Loader {
            id: highlight
            active: false
            source: "EditorHighlighter.qml"
            onLoaded: {
                item.fileName = root.fileName
                item.textEdit = editor
            }
        }

        // Row colours and line numbers, scrolled with the text.
        ListView {
            id: rowsView
            anchors.fill: parent
            anchors.topMargin: 8
            interactive: false
            model: root.rows.length
            // Kept in step by hand: a new model resets a ListView's
            // position, which would undo a binding.
            function follow() { contentY = flick.contentY + originY }
            onCountChanged: follow()
            onOriginYChanged: follow()
            // As far down as the text scrolls (its 40px past the end).
            footer: Item { width: 1; height: 40 }
            delegate: Item {
                id: rowItem
                required property int index
                readonly property var r: root.rows[index] || [0, 0, 0, -1, -1, 0, 0, 0, -1, -1]
                readonly property int kind: r[pane.base]
                width: rowsView.width
                height: pane.lineHeight
                Rectangle {
                    anchors.fill: parent
                    visible: rowItem.kind === 1
                    color: Qt.rgba(pane.tint.r, pane.tint.g, pane.tint.b, 0.14)
                }
                // Nothing on this side: hatched.
                Image {
                    anchors.fill: parent
                    anchors.leftMargin: pane.gutterWidth
                    visible: rowItem.kind === 2
                    fillMode: Image.Tile
                    horizontalAlignment: Image.AlignLeft
                    verticalAlignment: Image.AlignTop
                    source: rowItem.kind === 2 ? "data:image/svg+xml;utf8," + encodeURIComponent("<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><path d='M-2 2 L2 -2 M0 8 L8 0 M6 10 L10 6' stroke='" + Theme.border + "' stroke-width='1.2'/></svg>") : ""
                }
                // The part of the line that changed.
                Rectangle {
                    readonly property int from: rowItem.r[pane.base + 3]
                    readonly property int to: rowItem.r[pane.base + 4]
                    readonly property int offset: rowItem.r[pane.base + 2]
                    visible: rowItem.kind === 1 && from >= 0 && pane.ready
                    // (After the text is laid out: its size says when.)
                    readonly property real x0: visible && editor.contentWidth > 0 ? editor.positionToRectangle(offset + from).x : 0
                    readonly property real x1: visible && editor.contentWidth > 0 ? editor.positionToRectangle(offset + to).x : 0
                    x: pane.gutterWidth + 8 + x0 - flick.contentX
                    width: Math.max(2, x1 - x0)
                    height: parent.height
                    color: Qt.rgba(pane.tint.r, pane.tint.g, pane.tint.b, 0.32)
                }
                Rectangle { width: pane.gutterWidth; height: parent.height; color: Theme.background; opacity: rowItem.kind === 1 ? 0.35 : 1 }
                // A review comment on this line: its mark; a click on the
                // number adds one (or opens it).
                readonly property int lineNo: rowItem.kind < 2 ? rowItem.r[pane.base + 1] : -1
                readonly property bool commented: lineNo > 0 && root.comments.length > 0 && root.commentAt(pane.isLeft ? "old" : "new", lineNo) >= 0
                Rectangle {
                    x: 3
                    anchors.verticalCenter: parent.verticalCenter
                    width: 14
                    height: 14
                    radius: 7
                    visible: rowItem.commented || (gutterHover.hovered && rowItem.lineNo > 0 && root.review !== null)
                    color: rowItem.commented ? Theme.accent : Theme.controlFill
                    Label {
                        anchors.centerIn: parent
                        text: rowItem.commented ? "💬" : "+"
                        color: rowItem.commented ? Theme.onPrimary : Theme.text
                        font.pixelSize: rowItem.commented ? Theme.px(8) : Theme.px(11)
                    }
                }
                Item {
                    objectName: "gutter-" + (pane.isLeft ? "l" : "r") + "-" + rowItem.index
                    width: pane.gutterWidth
                    height: parent.height
                    HoverHandler { id: gutterHover; cursorShape: rowItem.lineNo > 0 ? Qt.PointingHandCursor : Qt.ArrowCursor }
                    TapHandler { onTapped: root.openComment(pane.isLeft, rowItem.index) }
                    ToolTip.visible: gutterHover.hovered && rowItem.lineNo > 0
                    ToolTip.delay: 600
                    ToolTip.text: rowItem.commented ? "Your comment (click to edit)" : "Comment on this line"
                }
                Label {
                    width: pane.gutterWidth - 18
                    height: parent.height
                    text: rowItem.kind >= 2 ? "" : rowItem.r[pane.base + 1]
                    horizontalAlignment: Text.AlignRight
                    verticalAlignment: Text.AlignVCenter
                    color: rowItem.kind === 1 ? pane.tint : Theme.muted
                    opacity: rowItem.kind === 1 ? 0.9 : 0.55
                    font.family: Theme.mono
                    font.pixelSize: Theme.mpx(12)
                }
                Label {
                    x: pane.gutterWidth - 14
                    width: 12
                    height: parent.height
                    visible: rowItem.kind === 1
                    text: pane.isLeft ? "−" : "+"
                    verticalAlignment: Text.AlignVCenter
                    color: pane.tint
                    font.family: Theme.mono
                    font.pixelSize: Theme.mpx(12)
                }
            }
        }

        // Folded unchanged regions: a bar over the text, open on a click.
        Repeater {
            model: root.folds
            Rectangle {
                id: fold
                required property var modelData
                objectName: "diffFold-" + (pane.isLeft ? "l" : "r") + "-" + modelData.start
                z: 2
                width: pane.width
                height: pane.lineHeight + 4
                y: 8 + modelData.row * pane.lineHeight - flick.contentY - 2
                visible: y > -height && y < pane.height
                color: foldHover.hovered ? Theme.hover : Theme.elevated
                Rectangle { width: parent.width; height: 1; color: Theme.border }
                Rectangle { y: parent.height - 1; width: parent.width; height: 1; color: Theme.border }
                Label {
                    x: pane.gutterWidth - 30
                    width: 24
                    height: parent.height
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    text: "⇕"
                    color: Theme.muted
                    font.pixelSize: Theme.px(12)
                }
                Label {
                    x: pane.gutterWidth + 8
                    height: parent.height
                    verticalAlignment: Text.AlignVCenter
                    text: fold.modelData.count + " hidden lines"
                    color: Theme.muted
                    font.pixelSize: Theme.px(12)
                }
                HoverHandler { id: foldHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: root.openFold(fold.modelData.start) }
                ToolTip.visible: foldHover.hovered
                ToolTip.text: "Show these lines"
            }
        }

        Flickable {
            id: flick
            anchors.fill: parent
            anchors.leftMargin: pane.gutterWidth + 8
            anchors.topMargin: 8
            clip: true
            contentWidth: Math.max(width, editor.implicitWidth + 40)
            contentHeight: Math.max(height, editor.implicitHeight + 40)
            boundsBehavior: Flickable.StopAtBounds
            pixelAligned: true
            ScrollBar.horizontal: ScrollBar { }
            onContentYChanged: { rowsView.follow(); pane.scrolled(contentY) }

            TextArea {
                id: editor
                objectName: pane.isLeft ? "diffLeftText" : "diffRightText"
                width: Math.max(flick.width, implicitWidth)
                padding: 0
                background: null
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.NoWrap
                font.family: Theme.mono
                font.pixelSize: Theme.mpx(13)
                color: Theme.foreground
                selectionColor: Theme.selection
                selectedTextColor: Theme.foreground
                tabStopDistance: 4 * metrics.averageCharacterWidth
                FontMetrics { id: metrics; font: editor.font }
            }
        }
    }
}
