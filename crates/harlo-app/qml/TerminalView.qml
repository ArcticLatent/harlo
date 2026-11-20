import QtQuick
import QtQuick.Controls.Basic
import harlo

// One terminal: a shell in `cwd`, drawn by Rust (TerminalSession) in the
// code font and the theme's colours. Typing goes to the shell; Ctrl+Shift+C / Ctrl+Shift+V
// copy and paste; drag selects; the wheel scrolls the history; a click on a
// link opens it.
Rectangle {
    id: root
    property string cwd
    property alias session: term
    readonly property bool running: term.running
    /// The shell ended by itself (`exit`, a crash), not through restart().
    signal exited()
    property bool wasRunning: false
    property bool restarting: false
    onRunningChanged: {
        if (running) { wasRunning = true; restarting = false }
        else if (wasRunning && !restarting) exited()
    }
    color: background
    focus: true

    readonly property color background: Theme.codeBg
    readonly property real pad: 8
    readonly property int cols: Math.max(2, Math.floor((width - 2 * pad - 10) / cellW))
    readonly property int rows: Math.max(1, Math.floor((height - 2 * pad) / cellH))
    readonly property real cellW: Math.max(1, term.cellWidth)
    readonly property real cellH: Math.max(1, term.cellHeight)

    readonly property var termColors: Theme.terminalColors(root.background)
    onTermColorsChanged: term.setPalette(termColors)

    // The screen and its cursor (under the mouse area below).
    TerminalSession {
        id: term
        objectName: "terminalScreen"
        anchors.fill: parent
        fontFamily: Theme.mono
        fontPixelSize: Theme.mpx(13)
        padding: root.pad
        focused: root.activeFocus
    }

    /// Typed into the shell once it has started (then cleared).
    property string initialCommand: ""
    function start() {
        if (!term.running && width > 50 && height > 30 && term.start(cwd, cols, rows, termColors) && initialCommand !== "")
            typeLater.start()
    }
    Timer {
        id: typeLater
        interval: 500
        onTriggered: { term.paste(root.initialCommand); term.key(Qt.Key_Return, 0, "\r"); root.initialCommand = "" }
    }
    function restart() {
        restarting = true
        term.stop()
        Qt.callLater(start)
    }
    Component.onCompleted: Qt.callLater(start)
    onColsChanged: if (term.running) Qt.callLater(() => term.resize(cols, rows)); else Qt.callLater(start)
    onRowsChanged: if (term.running) Qt.callLater(() => term.resize(cols, rows)); else Qt.callLater(start)

    // Clipboard: QML has no clipboard API, so go through a hidden TextEdit.
    TextEdit { id: copier; visible: false }
    function copySelection() {
        const t = term.selectedText()
        if (t === "") return
        copier.text = t; copier.selectAll(); copier.copy(); copier.text = ""
    }
    function pasteClipboard() {
        copier.text = ""; copier.paste()
        if (copier.text !== "") term.paste(copier.text)
        copier.text = ""
    }

    // Keep most Ctrl/Alt keys for the shell (Ctrl+W, Ctrl+B, Ctrl+N…); app
    // shortcuts with Ctrl+Shift, tab switching and Ctrl+` still work.
    Keys.onShortcutOverride: event => {
        const ctrl = event.modifiers & Qt.ControlModifier, shift = event.modifiers & Qt.ShiftModifier
        const appKey = (ctrl && shift) || (ctrl && (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab || event.key === Qt.Key_PageUp || event.key === Qt.Key_PageDown))
            || event.key === Qt.Key_QuoteLeft
        event.accepted = !appKey
    }
    Keys.onPressed: event => {
        const ctrl = event.modifiers & Qt.ControlModifier, shift = event.modifiers & Qt.ShiftModifier
        if (ctrl && shift && event.key === Qt.Key_C) { copySelection(); event.accepted = true; return }
        if ((ctrl && shift && event.key === Qt.Key_V) || (shift && event.key === Qt.Key_Insert)) { pasteClipboard(); event.accepted = true; return }
        if (!term.running) return
        term.key(event.key, event.modifiers, event.text)
        event.accepted = true
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton
        hoverEnabled: true
        cursorShape: link !== "" ? Qt.PointingHandCursor : Qt.IBeamCursor
        property int clicks: 0
        property real lastClick: 0
        /// The link under the pointer; the one pressed on (opened if
        /// released there without a drag).
        property string link: ""
        property string pressedLink: ""
        property var pressedAt: null
        function linkUnder(mouse) {
            const c = cell(mouse)
            return c.row >= 0 && c.col >= 0 && c.col < root.cols ? term.linkAt(c.row, c.col) : ""
        }
        onExited: link = ""
        ToolTip.visible: link !== "" && containsMouse && !pressed
        ToolTip.delay: 600
        ToolTip.text: "Open " + link
        function cell(mouse) {
            const col = (mouse.x - root.pad) / root.cellW
            return { row: Math.floor((mouse.y - root.pad) / root.cellH), col: Math.floor(col), right: col - Math.floor(col) > 0.5 }
        }
        onPressed: mouse => {
            root.forceActiveFocus()
            if (mouse.button === Qt.MiddleButton) { root.pasteClipboard(); return }
            const now = Date.now()
            clicks = now - lastClick < 400 ? Math.min(3, clicks + 1) : 1
            lastClick = now
            const c = cell(mouse)
            pressedLink = clicks === 1 ? linkUnder(mouse) : ""
            pressedAt = c
            if (clicks === 1) term.selectClear()
            term.selectStart(c.row, c.col, clicks)
            if (clicks > 1) term.selectUpdate(c.row, c.col, true)
        }
        onPositionChanged: mouse => {
            if (!(mouse.buttons & Qt.LeftButton)) { link = linkUnder(mouse); return }
            const c = cell(mouse)
            // (A drag: a selection, not a click on the link.)
            if (pressedAt && (c.row !== pressedAt.row || c.col !== pressedAt.col)) pressedLink = ""
            term.selectUpdate(c.row, c.col, c.right)
        }
        onReleased: mouse => {
            if (mouse.button === Qt.LeftButton && pressedLink !== "") {
                term.selectClear()
                Qt.openUrlExternally(pressedLink)
            }
            pressedLink = ""
        }
        onWheel: wheel => term.wheel(Math.round(wheel.angleDelta.y / 40))
    }

    // Scrolled back: a way down.
    Rectangle {
        visible: term.scrolledBack > 0
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 8
        implicitWidth: backLabel.implicitWidth + 16
        implicitHeight: 24
        radius: 12
        color: Theme.elevated
        border.color: Theme.border
        Label { id: backLabel; anchors.centerIn: parent; text: "↓ " + term.scrolledBack + " lines up"; color: Theme.muted; font.pixelSize: Theme.px(11) }
        TapHandler { onTapped: term.wheel(-term.scrolledBack) }
    }

    // The shell ended (or could not start).
    Rectangle {
        visible: !term.running && term.exitText !== ""
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 10
        implicitWidth: exitRow.implicitWidth + 20
        implicitHeight: 34
        radius: 3
        color: Theme.elevated
        border.color: Theme.border
        Row {
            id: exitRow
            anchors.centerIn: parent
            spacing: 10
            Label { anchors.verticalCenter: parent.verticalCenter; text: term.exitText; color: Theme.muted; font.pixelSize: Theme.px(12) }
            FlatButton { text: "Restart"; font.pixelSize: Theme.px(12); onClicked: root.restart() }
        }
    }
}
