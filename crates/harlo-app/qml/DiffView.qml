import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// A file's changes side by side (the diff tab): the
// old version on the left, the new one on the right, scrolling together.
// Removed lines are red, added ones green, the changed part of a line
// stronger; hatched rows stand in for lines the other side doesn't have.
// The strip on the right maps every change; the buttons between the sides
// stage, unstage or discard a hunk. It reloads as the file changes. When
// the two sides wouldn't fit ~50 characters each it shows one column
// instead (inline: a change's removed lines above its added ones).
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
    // `row`: the line's row in `rows`; `pane` and `shown`: where it shows.
    function openComment(isLeft, row, pane, shown) {
        const r = rows[row]
        if (!r || !review) return
        const kind = r[isLeft ? 0 : 5], line = r[isLeft ? 1 : 6]
        if (kind >= 2 || !(line > 0)) return
        const side = isLeft ? "old" : "new"
        const code = ((isLeft ? view.left : view.right).split("\n")[row] || "").trim()
        const i = commentAt(side, line)
        commentBox.edit(side, line, code, i >= 0 ? comments[i].text : "", shown, pane)
    }
    // A saved comment's card: open it again.
    function editComment(ci, pane, shown) {
        const c = comments[ci]
        if (c) commentBox.edit(c.side, c.line, c.code, c.text, shown, pane)
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
        function edit(s, l, c, text, row, pane) {
            side = s
            line = l
            code = c
            existing = text !== ""
            commentText.text = text
            x = Math.max(12, Math.min(pane.x + 40, root.width - width - 12))
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

    // ---- layout: side by side, or inline --------------------------------
    // "auto" (side by side when each side fits ~50 characters), "split" or
    // "unified"; remembered.
    property string layoutSetting: Workspace.setting("diff_layout") || "auto"
    function setLayout(v) {
        Workspace.setSetting("diff_layout", v)
        layoutSetting = v
        jumped = false
    }
    FontMetrics { id: codeMetrics; font.family: Theme.mono; font.pixelSize: Theme.mpx(13) }
    readonly property bool splitFits: (sides.width - sides.middleWidth - sides.rulerWidth) / 2 - 66 >= 50 * codeMetrics.averageCharacterWidth
    readonly property bool unified: !sides.added && (layoutSetting === "unified" || (layoutSetting !== "split" && !splitFits))

    onPathChanged: load()
    onStagedChanged: load()
    Component.onCompleted: load()
    function load() {
        jumped = false
        expanded = ({})
        layoutSetting = Workspace.setting("diff_layout") || "auto"
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
    // A saved comment shows under its line, on rows of its own (kind 4:
    // `[4, 0, offset, -1, -1, 4, 0, offset, -1, -1, -1, comment, k]`).
    readonly property int noteRows: 3
    readonly property var view: {
        const src = allRows, left = dg.splitLeft, right = dg.splitRight
        const mine = []
        comments.forEach((c, i) => { if (c.path === relPath) mine.push([c, i]) })
        if ((!collapse && mine.length === 0) || src.length === 0)
            return { rows: src, left: left, right: right, index: null }
        const notesAt = r => mine.filter(([c]) => c.side === "old" ? r[0] < 2 && r[1] === c.line : r[5] < 2 && r[6] === c.line).map(([, i]) => i)
        // Rows within `context` of a change stay.
        const keep = new Array(src.length).fill(!collapse)
        for (let i = 0; collapse && i < src.length; i++) {
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
                for (const ci of notesAt(src[i]))
                    for (let k = 0; k < noteRows; k++) add([4, 0, 0, -1, -1, 4, 0, 0, -1, -1, -1, ci, k], "", "")
            }
        }
        return { rows: rows, left: lOut.join("\n"), right: rOut.join("\n"), index: index }
    }
    readonly property var rows: view.rows
    // Inline: one column of `rows`, a change's removed lines above its added
    // ones. Same fields, plus [13] its row in `rows` and [14] 1 for an old line.
    readonly property var uni: {
        if (!unified) return null
        const src = view.rows, lL = view.left.split("\n"), rL = view.right.split("\n")
        const out = [], lines = [], index = new Array(src.length)
        let off = 0
        const add = (r, text) => { r[2] = off; r[7] = off; out.push(r); lines.push(text); off += text.length + 1 }
        const changed = r => r[0] === 1 || r[0] === 2 || r[5] === 1 || r[5] === 2
        for (let i = 0; i < src.length;) {
            const r = src[i]
            if (!changed(r)) {
                index[i] = out.length
                if (r[0] === 3 || r[0] === 4) { const c = r.slice(); c[13] = i; c[14] = 0; add(c, "") }
                else add([-1, r[1], 0, -1, -1, 0, r[6], 0, -1, -1, -1, -1, -1, i, 0], rL[i] || "")
                i++
                continue
            }
            let j = i
            while (j < src.length && changed(src[j])) j++
            for (let k = i; k < j; k++) if (src[k][0] === 1) { index[k] = out.length; add([1, src[k][1], 0, src[k][3], src[k][4], -1, 0, 0, -1, -1, -1, -1, -1, k, 1], lL[k] || "") }
            for (let k = i; k < j; k++) if (src[k][5] === 1) { if (index[k] === undefined) index[k] = out.length; add([-1, 0, 0, -1, -1, 1, src[k][6], 0, src[k][8], src[k][9], -1, -1, -1, k, 0], rL[k] || "") }
            i = j
        }
        return { rows: out, text: lines.join("\n"), index: index }
    }
    /// The rows the main (right, or only) pane shows.
    readonly property var shownRows: uni ? uni.rows : rows
    readonly property var hunks: {
        if (!info) return []
        const toView = view.index ? info.hunks.map(h => [h[0], view.index[h[1]]]) : info.hunks
        return uni ? toView.map(h => [h[0], uni.index[h[1]]]) : toView
    }
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
        const rows = root.shownRows
        for (let i = 0; i < rows.length; i++) {
            const r = rows[i], l = r[0] === 1 || r[0] === 2, rt = r[5] === 1 || r[5] === 2
            // (A comment under a changed line doesn't split its change.)
            if (r[0] === 4) continue
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
    onBlocksChanged: jumpWhenReady()
    // (A long file's text comes in a slice at a time: once it's all in.)
    function jumpWhenReady() {
        if (!jumped && blocks.length > 0 && leftPane.ready && rightPane.ready) { jumped = true; jumpTries = 0; Qt.callLater(jumpToFirst) }
    }
    // The first change, a third of the way down, once the view has its
    // height (a third of none put it at the very top).
    property int jumpTries: 0
    Timer { id: jumpLater; interval: 50; onTriggered: root.jumpToFirst() }
    function jumpToFirst() {
        if (blocks.length === 0) return
        if (rightPane.flick.height <= 0 && ++jumpTries < 40) { jumpLater.start(); return }
        showRow(blocks[0].start)
    }

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
        // (In an Item: however much it holds, it doesn't widen the view.
        // Narrow, the folder and counts go and the file's actions move into
        // a menu.)
        Item {
        Layout.fillWidth: true
        implicitHeight: toolbar.implicitHeight + 12
        clip: true
        readonly property bool narrow: width < 640
        // (Tighter still: the counts go, and hiding unchanged lines and the
        // layout join the menu.)
        readonly property bool tight: width < 360
        RowLayout {
            id: toolbar
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 8
            spacing: 6
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
                Label { id: sideLabel; anchors.centerIn: parent; text: root.staged ? "Staged" : "Working tree"; color: Theme.muted; font.pixelSize: Theme.px(10.5) }
                HoverHandler { id: sideHover }
                ToolTip.visible: sideHover.hovered
                ToolTip.text: root.staged ? "What's staged (right) against the last commit (left)" : "The file as it is (right) against what's staged or committed (left)"
            }
            Label {
                objectName: "diffStats"
                visible: !toolbar.parent.tight
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
            ToolIcon {
                objectName: "diffPrev"
                enabled: root.blocks.length > 0
                iconName: "arrow-up-s"
                tip: "Previous change (Alt+↑)"
                onClicked: root.nextChange(-1)
            }
            ToolIcon {
                objectName: "diffNext"
                enabled: root.blocks.length > 0
                iconName: "arrow-down-s"
                tip: "Next change (Alt+↓)"
                onClicked: root.nextChange(1)
            }
            ToolIcon {
                objectName: "diffCollapse"
                visible: !toolbar.parent.tight
                iconName: "expand-up-down"
                on: root.collapse
                tip: root.collapse ? "Show unchanged lines" : "Hide unchanged lines (keep the changes and 3 lines around each)"
                onClicked: root.setCollapse(!root.collapse)
            }
            ToolIcon {
                objectName: "diffLayout"
                visible: !sides.added && !toolbar.parent.tight
                iconName: root.unified ? "layout-column" : "layout-row"
                tip: root.unified ? "Show side by side" + (root.layoutSetting === "auto" ? " (shown inline: the sides don't fit)" : "")
                                  : "Show inline (one column)"
                onClicked: root.setLayout(root.unified ? "split" : "unified")
            }
            Rectangle { visible: !toolbar.parent.narrow; implicitWidth: 1; implicitHeight: 16; color: Theme.border; Layout.leftMargin: 2; Layout.rightMargin: 2 }
            FlatButton {
                objectName: "diffOpen"
                visible: !toolbar.parent.narrow
                text: "Open file"; font.pixelSize: Theme.px(11.5); padding: 4
                ToolTip.visible: hovered; ToolTip.text: "Open it in the editor"
                onClicked: root.openFile(root.path)
            }
            FlatButton {
                objectName: "diffDiscardFile"
                visible: !toolbar.parent.narrow && !root.staged && dg.selectedPath !== ""
                text: "Discard…"; accent: Theme.error; font.pixelSize: Theme.px(11.5); padding: 4
                ToolTip.visible: hovered; ToolTip.text: "Discard all the changes to this file"
                onClicked: { confirmDiscard.hunk = -1; confirmDiscard.open() }
            }
            FlatButton {
                objectName: "diffStageFile"
                visible: !toolbar.parent.narrow && dg.selectedPath !== ""
                text: root.staged ? "Unstage" : "Stage"; font.pixelSize: Theme.px(11.5); padding: 4
                ToolTip.visible: hovered; ToolTip.text: root.staged ? "Unstage the whole file" : "Stage the whole file"
                onClicked: root.staged ? dg.unstageFile(dg.selectedPath) : dg.stageFile(dg.selectedPath)
            }
            // Narrow: the file's actions and the layout choice in a menu.
            ToolIcon {
                id: moreButton
                objectName: "diffMore"
                iconName: "more"
                tip: "More"
                onClicked: moreMenu.popup(moreButton, 0, moreButton.height + 4)
            }
        }
        }
        AppMenu {
            id: moreMenu
            objectName: "diffMoreMenu"
            AppMenuItem { text: "Open file"; visible: toolbar.parent.narrow; onTriggered: root.openFile(root.path) }
            AppMenuItem { text: root.staged ? "Unstage the file" : "Stage the file"; visible: toolbar.parent.narrow && dg.selectedPath !== ""
                onTriggered: root.staged ? dg.unstageFile(dg.selectedPath) : dg.stageFile(dg.selectedPath) }
            AppMenuItem { text: "Discard the changes…"; danger: true; visible: toolbar.parent.narrow && !root.staged && dg.selectedPath !== ""
                onTriggered: { confirmDiscard.hunk = -1; confirmDiscard.open() } }
            AppMenuSeparator { visible: toolbar.parent.narrow; height: visible ? implicitHeight : 0 }
            AppMenuItem { objectName: "diffMenuCollapse"; text: "Hide unchanged lines"; visible: toolbar.parent.tight; marked: root.collapse; onTriggered: root.setCollapse(!root.collapse) }
            AppMenuSeparator { visible: toolbar.parent.tight; height: visible ? implicitHeight : 0 }
            AppMenuItem { objectName: "diffLayoutAuto"; text: "Side by side when it fits"; marked: root.layoutSetting === "auto"; onTriggered: root.setLayout("auto") }
            AppMenuItem { objectName: "diffLayoutSplit"; text: "Always side by side"; marked: root.layoutSetting === "split"; onTriggered: root.setLayout("split") }
            AppMenuItem { objectName: "diffLayoutUnified"; text: "Always inline"; marked: root.layoutSetting === "unified"; onTriggered: root.setLayout("unified") }
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
            readonly property int middleWidth: 30
            readonly property int rulerWidth: 14
            // A new file has no old side: its lines take the whole width, as
            // they do inline.
            readonly property bool added: dg.splitLeft.replace(/\n/g, "") === "" && dg.splitRight.trim() !== ""
            readonly property bool single: added || root.unified
            readonly property real paneWidth: (width - middleWidth - rulerWidth) / (single ? 1 : 2)

            DiffPane {
                id: leftPane
                isLeft: true
                rows: root.rows
                text: root.view.left
                x: 0
                visible: !sides.single
                width: sides.single ? 0 : sides.paneWidth
                height: sides.height
                onScrolled: y => { if (rightPane.flick.contentY !== y) rightPane.flick.contentY = y }
            }
            // Between the sides: each hunk's actions, by its first change.
            Item {
                id: middle
                x: leftPane.width
                width: sides.middleWidth
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
                        visible: rowY > -60 && rowY < middle.height
                        spacing: 4
                        Repeater {
                            model: root.staged ? [["subtract", "Unstage this change", "unstage"]] : [["add", "Stage this change", "stage"], ["arrow-go-back", "Discard this change…", "discard"]]
                            Button {
                                id: hunkButton
                                required property var modelData
                                objectName: "hunk-" + modelData[2] + "-" + hunkButtons.hunk
                                implicitWidth: 22
                                implicitHeight: 22
                                padding: 0
                                flat: true
                                contentItem: Item {
                                    Icon {
                                        anchors.centerIn: parent
                                        name: hunkButton.modelData[0]
                                        color: hunkButton.modelData[2] === "discard" ? Theme.error : Theme.foreground
                                        implicitWidth: 14
                                        implicitHeight: 14
                                    }
                                }
                                background: Rectangle {
                                    radius: 6
                                    color: hunkButton.down ? Theme.pressed : hunkButton.hovered ? Theme.hover : Theme.elevated
                                    border.color: hunkButton.visualFocus ? Theme.accent : Theme.border
                                }
                                ToolTip.visible: hovered || visualFocus
                                ToolTip.delay: 300
                                ToolTip.text: modelData[1]
                                onClicked: {
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
            DiffPane {
                id: rightPane
                isLeft: false
                unified: root.unified
                rows: root.shownRows
                text: root.uni ? root.uni.text : root.view.right
                x: leftPane.width + sides.middleWidth
                width: sides.paneWidth
                height: sides.height
                onScrolled: y => { if (leftPane.flick.contentY !== y) leftPane.flick.contentY = y }
            }
            // The map of changes: click or drag to go there.
            Item {
                id: ruler
                objectName: "diffRuler"
                x: sides.width - sides.rulerWidth
                width: sides.rulerWidth
                height: sides.height
                Rectangle { width: 1; height: parent.height; color: Theme.border }
                // All on screen: each change beside its rows; else the whole
                // file squeezed to the strip.
                readonly property bool fits: root.shownRows.length * root.lineHeight + 8 <= height
                readonly property real startY: fits ? 8 : 0
                readonly property real rowScale: fits ? root.lineHeight : root.shownRows.length > 0 ? height / root.shownRows.length : 0
                Repeater {
                    model: root.blocks
                    Item {
                        required property var modelData
                        x: 2
                        y: ruler.startY + modelData.start * ruler.rowScale
                        width: ruler.width - 3
                        height: Math.max(2, (modelData.end - modelData.start) * ruler.rowScale)
                        Rectangle { width: parent.width / 2; height: parent.height; color: Theme.error; opacity: 0.8; visible: parent.modelData.left }
                        Rectangle { x: parent.width / 2; width: parent.width / 2; height: parent.height; color: Theme.success; opacity: 0.8; visible: parent.modelData.right }
                    }
                }
                // Where the view is.
                Rectangle {
                    readonly property var f: rightPane.flick
                    visible: !ruler.fits
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
            readonly property bool narrow: root.width < 460
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
            // How many (always whole), then for whom (cut first).
            Label {
                objectName: "reviewSummary"
                text: (root.comments.length === 1 ? "1 comment" : root.comments.length + " comments") + (reviewBar.narrow ? "" : " on " + (reviewBar.files === 1 ? "1 file" : reviewBar.files + " files"))
                color: Theme.text
                font.pixelSize: Theme.px(13)
            }
            Label {
                id: reviewFor
                objectName: "reviewFor"
                // (Narrow: Send's tooltip says it.)
                opacity: reviewBar.narrow ? 0 : 1
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                elide: Text.ElideRight
                text: root.canSend ? "for " + root.sessionTitle : "open a session in this folder to send them"
                color: Theme.textMuted
                font.pixelSize: Theme.px(13)
                HoverHandler { id: forHover }
                ToolTip.visible: forHover.hovered && truncated
                ToolTip.text: text
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
                text: reviewBar.narrow ? "Send" : "Send to the agent"
                ToolTip.visible: hovered && reviewBar.narrow
                ToolTip.text: root.canSend ? "Send them to " + root.sessionTitle : "Open a session in this folder to send them"
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

    // A toolbar button: an icon, a tooltip, `on` when toggled.
    component ToolIcon: Button {
        id: ti
        property string iconName
        property string tip: ""
        property bool on: false
        flat: true
        padding: 0
        implicitWidth: 26
        implicitHeight: 26
        contentItem: Item {
            Icon { anchors.centerIn: parent; name: ti.iconName; color: ti.on ? Theme.accent : Theme.foreground; implicitWidth: 16; implicitHeight: 16; opacity: ti.enabled ? 1 : 0.35 }
        }
        background: Rectangle {
            radius: 6
            color: ti.down ? Theme.pressed : ti.on ? Theme.selected : ti.hovered ? Theme.hover : "transparent"
            border.color: ti.visualFocus ? Theme.accent : "transparent"
            Behavior on color { ColorAnimation { duration: Theme.hoverMotion } }
        }
        ToolTip.visible: (hovered || visualFocus) && tip !== ""
        ToolTip.delay: 400
        ToolTip.text: tip
    }

    // One side (or, inline, the one column): line numbers, row colours, the
    // text (read-only, selectable, syntax-highlighted), folds and comments.
    component DiffPane: Item {
        id: pane
        property bool isLeft: true
        /// Inline: each row says its side ([14] 1 for an old line).
        property bool unified: false
        property var rows: []
        property string text: ""
        signal scrolled(real y)
        property alias flick: flick
        property alias editor: editor
        readonly property real lineHeight: editor.lineCount > 0 ? editor.contentHeight / editor.lineCount : 16
        readonly property int base: isLeft ? 0 : 5
        readonly property var folds: rows.reduce((out, r, i) => { if (r[0] === 3) out.push({ row: i, count: r[1], start: r[10] }); return out }, [])
        // Saved comments' cards: on the side they're about (inline, all).
        readonly property var notes: rows.reduce((out, r, i) => {
            const c = r[0] === 4 && r[12] === 0 ? root.comments[r[11]] : null
            if (c && (pane.unified || (c.side === "old") === pane.isLeft)) out.push({ row: i, comment: r[11] })
            return out
        }, [])
        readonly property int gutterWidth: 58
        readonly property bool ready: editor.text === text
        // Test hook: how far the row colours are scrolled.
        function rowsAt() { return rowsView.contentY - rowsView.originY }
        clip: true

        onTextChanged: {
            // Same place after a reload (the file changed under the view).
            const x = flick.contentX, y = flick.contentY
            // (The highlighter first: it colours each slice as it comes in.)
            if (!highlight.active && text !== "") highlight.active = true
            fill.put(text)
            flick.contentX = x
            flick.contentY = Math.min(y, Math.max(0, flick.contentHeight - flick.height))
        }
        onReadyChanged: if (ready) root.jumpWhenReady()
        TextFill { id: fill; editor: editor }
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
            model: pane.rows.length
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
                readonly property var r: pane.rows[index] || [0, 0, 0, -1, -1, 0, 0, 0, -1, -1]
                // Which side's fields: the pane's, or inline the row's own.
                readonly property int base: pane.unified ? (r[14] === 1 ? 0 : 5) : pane.base
                readonly property color tint: base === 0 ? Theme.error : Theme.success
                readonly property int kind: r[base]
                width: rowsView.width
                height: pane.lineHeight
                Rectangle {
                    anchors.fill: parent
                    visible: rowItem.kind === 1
                    color: Qt.rgba(rowItem.tint.r, rowItem.tint.g, rowItem.tint.b, 0.14)
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
                    readonly property int from: rowItem.r[rowItem.base + 3]
                    readonly property int to: rowItem.r[rowItem.base + 4]
                    readonly property int offset: rowItem.r[rowItem.base + 2]
                    visible: rowItem.kind === 1 && from >= 0 && pane.ready
                    // (After the text is laid out: its size says when.)
                    readonly property real x0: visible && editor.contentWidth > 0 ? editor.positionToRectangle(Math.min(offset + from, editor.length)).x : 0
                    readonly property real x1: visible && editor.contentWidth > 0 ? editor.positionToRectangle(Math.min(offset + to, editor.length)).x : 0
                    x: pane.gutterWidth + 8 + x0 - flick.contentX
                    width: Math.max(2, x1 - x0)
                    height: parent.height
                    // (Light: red reads heavier than green at the same strength.)
                    color: Qt.rgba(rowItem.tint.r, rowItem.tint.g, rowItem.tint.b, Theme.isLight && rowItem.base === 0 ? 0.22 : 0.32)
                }
                Rectangle { width: pane.gutterWidth; height: parent.height; color: Theme.background; opacity: rowItem.kind === 1 ? 0.35 : 1 }
                // A review comment on this line: its mark; a click on the
                // number adds one (or opens it).
                readonly property int lineNo: rowItem.kind < 2 ? rowItem.r[rowItem.base + 1] : -1
                readonly property bool commented: lineNo > 0 && root.comments.length > 0 && root.commentAt(rowItem.base === 0 ? "old" : "new", lineNo) >= 0
                Rectangle {
                    x: 3
                    anchors.verticalCenter: parent.verticalCenter
                    width: 14
                    height: 14
                    radius: 7
                    visible: rowItem.commented || (gutterHover.hovered && rowItem.lineNo > 0 && root.review !== null)
                    color: rowItem.commented ? Theme.accent : Theme.controlFill
                    Icon {
                        anchors.centerIn: parent
                        name: "chat-thread"
                        color: rowItem.commented ? Theme.textOnPrimary : Theme.text
                        implicitWidth: 9
                        implicitHeight: 9
                    }
                }
                Item {
                    objectName: "gutter-" + (pane.unified ? "u" : pane.isLeft ? "l" : "r") + "-" + rowItem.index
                    width: pane.gutterWidth
                    height: parent.height
                    HoverHandler { id: gutterHover; cursorShape: rowItem.lineNo > 0 ? Qt.PointingHandCursor : Qt.ArrowCursor }
                    TapHandler { onTapped: root.openComment(rowItem.base === 0, pane.unified ? rowItem.r[13] : rowItem.index, pane, rowItem.index) }
                    ToolTip.visible: gutterHover.hovered && rowItem.lineNo > 0
                    ToolTip.delay: 600
                    ToolTip.text: rowItem.commented ? "Your comment (click to edit)" : "Comment on this line"
                }
                Label {
                    width: pane.gutterWidth - 18
                    height: parent.height
                    text: rowItem.kind >= 2 ? "" : rowItem.r[rowItem.base + 1]
                    horizontalAlignment: Text.AlignRight
                    verticalAlignment: Text.AlignVCenter
                    color: rowItem.kind === 1 ? rowItem.tint : Theme.muted
                    opacity: rowItem.kind === 1 ? 0.9 : 0.55
                    font.family: Theme.mono
                    font.pixelSize: Theme.mpx(12)
                }
                Label {
                    x: pane.gutterWidth - 14
                    width: 12
                    height: parent.height
                    visible: rowItem.kind === 1
                    text: rowItem.base === 0 ? "−" : "+"
                    verticalAlignment: Text.AlignVCenter
                    color: rowItem.tint
                    font.family: Theme.mono
                    font.pixelSize: Theme.mpx(12)
                }
            }
        }

        // Folded unchanged regions: a bar over the text, open on a click.
        Repeater {
            model: pane.folds
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
                Icon {
                    x: pane.gutterWidth - 24
                    anchors.verticalCenter: parent.verticalCenter
                    name: "expand-up-down"
                    color: Theme.muted
                    implicitWidth: 12
                    implicitHeight: 12
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

        // Saved comments: a card under the line, on rows kept for it.
        Repeater {
            model: pane.notes
            Rectangle {
                id: note
                required property var modelData
                readonly property var comment: root.comments[modelData.comment] || ({ text: "" })
                objectName: "diffNote-" + modelData.comment
                z: 2
                x: pane.gutterWidth + 4
                width: pane.width - x - 10
                height: root.noteRows * pane.lineHeight - 6
                y: 8 + modelData.row * pane.lineHeight - flick.contentY + 3
                visible: y > -height && y < pane.height
                radius: 8
                color: noteHover.hovered ? Theme.hover : Theme.raised
                border.color: Theme.hairline
                Rectangle { width: 3; height: parent.height - 12; y: 6; x: 0; radius: 1.5; color: Theme.accent }
                Icon { x: 12; y: 9; name: "chat-thread"; color: Theme.accent; implicitWidth: 13; implicitHeight: 13 }
                Label {
                    id: noteText
                    x: 32
                    y: 7
                    width: parent.width - x - (noteHover.hovered ? 44 : 12)
                    text: note.comment.text
                    color: Theme.text
                    font.pixelSize: Theme.px(12.5)
                    wrapMode: Text.Wrap
                    maximumLineCount: Math.max(1, Math.floor((note.height - 12) / (font.pixelSize * 1.35)))
                    elide: Text.ElideRight
                }
                Label {
                    visible: noteHover.hovered
                    anchors.right: parent.right
                    anchors.rightMargin: 10
                    y: 8
                    text: "Edit"
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(12)
                }
                HoverHandler { id: noteHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: root.editComment(note.modelData.comment, pane, note.modelData.row - 1) }
                ToolTip.visible: noteHover.hovered && noteText.truncated
                ToolTip.delay: 500
                ToolTip.text: note.comment.text
            }
        }

        Flickable {
            id: flick
            anchors.fill: parent
            anchors.leftMargin: pane.gutterWidth + 8
            anchors.topMargin: 8
            clip: true
            contentWidth: Math.max(width, editor.implicitWidth + 40)
            // (As tall as all its rows while the text is still coming in.)
            contentHeight: Math.max(height, editor.implicitHeight + 40, fill.running ? pane.rows.length * pane.lineHeight + 40 : 0)
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
