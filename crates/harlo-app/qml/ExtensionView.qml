import QtQuick
import QtQuick.Controls.Basic
import harlo

// One of a pi extension's views (`ChatSession.extViewsJson`): the lines
// its component draws, in the code font and the theme's colours; the
// Markdown and pictures among them (the view's `blocks`) drawn as a
// reply's. Its keys (once it has the focus) and clicks go back to the
// extension, which draws it again for the width it's given.
Item {
    id: root
    required property ChatSession session
    /// The view's last frame: `{ id, kind, lines, result, focus, … }`.
    required property var view
    /// "result": a tool's result lines (and its clicks), not its call's.
    property string part: ""
    property color background: Theme.codeBg
    property real pad: 8
    readonly property var lines: !view ? [] : (part === "result" ? view.result : view.lines) || []
    /// The lines in blocks, when they hold Markdown or a picture (none:
    /// one grid of them).
    readonly property var blocks: !view ? [] : (part === "result" ? view.resultBlocks : view.blocks) || []
    readonly property bool inBlocks: blocks.length > 0
    // Each block's first row among the lines.
    readonly property var blockRows: {
        const out = []
        let row = 0
        for (const b of blocks) {
            out.push(row)
            row += b.rows || 0
        }
        return out
    }
    readonly property real cellW: Math.max(1, screen.cellWidth)
    readonly property real cellH: Math.max(1, screen.cellHeight)
    readonly property int cols: Math.max(10, Math.floor((width - 2 * pad) / cellW))
    implicitHeight: lines.length === 0 ? 0 : inBlocks ? blockColumn.implicitHeight + 2 * pad : lines.length * cellH + 2 * pad
    activeFocusOnTab: true
    Accessible.role: Accessible.StaticText
    Accessible.name: root.lines.length > 0 ? qsTr("An extension's view, %n line(s)", "", root.lines.length) : ""

    /// The link it last opened (self-tests).
    property string opened: ""
    function send(op) { if (root.view) root.session.extInput(root.view.id, JSON.stringify(op)) }

    // (Its width, once it settles: the extension draws for it.)
    onColsChanged: if (part === "") sizeLater.restart()
    Component.onCompleted: if (part === "") sizeLater.restart()
    Timer { id: sizeLater; interval: 30; onTriggered: if (root.width > 0) root.send({ op: "size", width: root.cols }) }

    Rectangle {
        anchors.fill: parent
        radius: 2
        color: root.background
        border.width: root.activeFocus ? 1 : 0
        border.color: Theme.accent
    }
    ExtensionLines {
        id: screen
        objectName: "extensionLines"
        anchors.fill: parent
        anchors.margins: 1
        visible: !root.inBlocks
        lines: root.inBlocks ? [] : root.lines
        cols: root.cols
        fontFamily: Theme.mono
        fontPixelSize: Theme.mpx(13)
        padding: root.pad - 1
        readonly property var colors: Theme.terminalColors(root.background)
        onColorsChanged: setPalette(colors)
        Component.onCompleted: setPalette(colors)
    }
    Column {
        id: blockColumn
        visible: root.inBlocks
        x: root.pad
        y: root.pad
        width: root.cols * root.cellW
        Repeater {
            id: blockList
            model: root.inBlocks ? root.blocks : []
            ExtensionBlock {
                width: blockColumn.width
                lines: root.lines
                cellW: root.cellW
                cellH: root.cellH
                colors: screen.colors
            }
        }
    }
    // The link in its Markdown at `x`, `y` (the view's), or "".
    function linkAt(x, y) {
        if (!inBlocks) return ""
        for (let i = 0; i < blockList.count; i++) {
            const it = blockList.itemAt(i)
            if (!it) continue
            const p = it.mapFromItem(root, x, y)
            if (p.y >= 0 && p.y < it.height) return it.linkAt(p.x, p.y)
        }
        return ""
    }
    // The row among the lines at `y` (the view's): in a grid of lines, its
    // own; in Markdown or a picture, as far down its rows as `y` is down it.
    function rowAt(y) {
        if (!inBlocks) return Math.floor((y - pad) / cellH)
        for (let i = 0; i < blockList.count; i++) {
            const it = blockList.itemAt(i), b = blocks[i]
            if (!it || y - pad >= it.y + it.height) continue
            const into = Math.max(0, y - pad - it.y)
            return blockRows[i] + Math.min((b.rows || 1) - 1, b.kind === "lines" ? Math.floor(into / cellH) : Math.floor(into / Math.max(1, it.height) * (b.rows || 1)))
        }
        return lines.length - 1
    }

    // Keys: all of them to the extension but the app's own (Ctrl+Shift…,
    // tab switching).
    Keys.onShortcutOverride: event => {
        const ctrl = event.modifiers & Qt.ControlModifier, shift = event.modifiers & Qt.ShiftModifier
        event.accepted = !((ctrl && shift) || (ctrl && (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab || event.key === Qt.Key_PageUp || event.key === Qt.Key_PageDown)))
    }
    Keys.onPressed: event => {
        const data = screen.keyData(event.key, event.modifiers, event.text)
        if (data === "") return
        root.send({ op: "key", data: data })
        event.accepted = true
    }

    MouseArea {
        id: mouse
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
        hoverEnabled: true
        property var last: null
        property var pressedAt: null
        property int clicks: 0
        property real lastClick: 0
        function cell(m) {
            return { x: Math.max(0, Math.min(root.cols - 1, Math.floor((m.x - root.pad) / root.cellW))),
                     y: Math.max(0, Math.min(Math.max(0, root.lines.length - 1), root.rowAt(m.y))) }
        }
        function button(b) { return b === Qt.LeftButton ? "left" : b === Qt.MiddleButton ? "middle" : b === Qt.RightButton ? "right" : "none" }
        function event(type, m, extra) {
            const c = cell(m)
            const e = { type: type, button: "none", x: c.x, y: c.y, screenX: c.x, screenY: c.y, width: root.cols, height: root.lines.length,
                        shift: (m.modifiers & Qt.ShiftModifier) !== 0, alt: (m.modifiers & Qt.AltModifier) !== 0, ctrl: (m.modifiers & Qt.ControlModifier) !== 0 }
            Object.assign(e, extra)
            root.send({ op: "mouse", event: e, part: root.part })
            last = c
        }
        // (A link pressed: opened on release, not passed on.)
        property string pressedLink: ""
        onPressed: m => {
            root.forceActiveFocus()
            pressedLink = root.linkAt(m.x, m.y)
            if (pressedLink !== "") return
            const now = Date.now()
            const c = cell(m)
            clicks = now - lastClick < 400 && pressedAt && pressedAt.x === c.x && pressedAt.y === c.y ? clicks + 1 : 1
            lastClick = now
            pressedAt = c
            event("press", m, { button: button(m.button) })
        }
        cursorShape: overLink ? Qt.PointingHandCursor : Qt.ArrowCursor
        property bool overLink: false
        onReleased: m => {
            // (A link in its Markdown opens, as a reply's does: web links.)
            if (pressedLink !== "") {
                const link = pressedLink
                pressedLink = ""
                if (root.linkAt(m.x, m.y) === link && /^https?:\/\//.test(link)) {
                    root.opened = link
                    Qt.openUrlExternally(link)
                }
                return
            }
            const c = cell(m)
            event("release", m, { button: button(m.button) })
            if (pressedAt && pressedAt.x === c.x && pressedAt.y === c.y) event("click", m, { button: button(m.button), clickCount: clicks })
        }
        // (Moves only when they reach another cell.)
        onPositionChanged: m => {
            overLink = root.linkAt(m.x, m.y) !== ""
            if (pressed && pressedLink !== "") return
            const c = cell(m)
            if (last && last.x === c.x && last.y === c.y) return
            event(pressed ? "drag" : "move", m, { button: pressed ? button(pressedButtons & Qt.LeftButton ? Qt.LeftButton : pressedButtons & Qt.RightButton ? Qt.RightButton : Qt.MiddleButton) : "none" })
        }
        // (In the chat, the wheel scrolls the chat; a screen or an overlay
        // of its own gets it.)
        onWheel: w => {
            const lines = -Math.round(w.angleDelta.y / 40)
            if (lines !== 0 && root.view && (root.view.kind === "screen" || root.view.kind === "overlay")) event("wheel", w, { wheelDelta: lines })
            else w.accepted = false
        }
    }
}
