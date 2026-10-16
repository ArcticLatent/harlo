import QtQuick
import QtQuick.Controls
import harlo

// One terminal: a shell in `cwd`, drawn line by line in the code font and
// the theme's colours. Typing goes to the shell; Ctrl+Shift+C / Ctrl+Shift+V
// copy and paste; drag selects; the wheel scrolls the history.
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
    readonly property real cellW: Math.max(1, metrics.advanceWidth("M"))
    readonly property real cellH: Math.max(1, Math.ceil(metrics.height))

    // 16 ANSI colours from the theme, then foreground, background, cursor, selection.
    readonly property var termColors: {
        const light = Theme.isLight
        const bright = c => light ? Qt.darker(c, 1.15) : Qt.lighter(c, 1.25)
        const magenta = light ? "#a626a4" : "#c678dd"
        const cyan = light ? "#0184bc" : "#56b6c2"
        const base = [light ? "#24292f" : "#3b3b3b", Theme.error, Theme.success, Theme.warning, Theme.info, magenta, cyan, light ? "#6e7781" : "#c8c8c8"]
        const brights = [Theme.muted, bright(Theme.error), bright(Theme.success), bright(Theme.warning), bright(Theme.info), bright(magenta), bright(cyan), light ? "#8c959f" : "#ffffff"]
        return base.concat(brights).map(c => Theme.rgb(Qt.color(c)))
            .concat([Theme.rgb(Theme.foreground), Theme.rgb(root.background), Theme.rgb(Theme.primary), Theme.rgb(Qt.tint(root.background, Theme.selection))])
    }
    onTermColorsChanged: term.setPalette(termColors)

    TerminalSession { id: term }
    FontMetrics { id: metrics; font.family: Theme.mono; font.pixelSize: Theme.mpx(13) }

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
    TextEdit { id: clip; visible: false }
    function copySelection() {
        const t = term.selectedText()
        if (t === "") return
        clip.text = t; clip.selectAll(); clip.copy(); clip.text = ""
    }
    function pasteClipboard() {
        clip.text = ""; clip.paste()
        if (clip.text !== "") term.paste(clip.text)
        clip.text = ""
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

    // Screen
    Column {
        id: screen
        x: root.pad
        y: root.pad
        Repeater {
            model: term
            Item {
                required property string html
                width: root.cols * root.cellW
                height: root.cellH
                Text {
                    text: html
                    textFormat: Text.RichText
                    color: Theme.foreground
                    font.family: Theme.mono
                    font.pixelSize: Theme.mpx(13)
                    renderType: Text.NativeRendering
                }
            }
        }
    }

    // Cursor: a block when focused, an outline otherwise; blinks while idle.
    Rectangle {
        id: cursor
        visible: term.cursorVisible && term.running
        x: root.pad + term.cursorCol * root.cellW
        y: root.pad + term.cursorRow * root.cellH
        width: root.cellW
        height: root.cellH
        color: root.activeFocus ? Theme.primary : "transparent"
        border.color: Theme.primary
        opacity: root.activeFocus && blink.off ? 0.25 : (root.activeFocus ? 0.7 : 1)
        Timer {
            id: blink
            property bool off: false
            interval: 600
            repeat: true
            running: root.activeFocus && cursor.visible
            onTriggered: off = !off
            onRunningChanged: off = false
        }
        Connections { target: term; function onCursorColChanged() { blink.off = false; blink.restart() } }
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton
        cursorShape: Qt.IBeamCursor
        property int clicks: 0
        property real lastClick: 0
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
            if (clicks === 1) term.selectClear()
            term.selectStart(c.row, c.col, clicks)
            if (clicks > 1) term.selectUpdate(c.row, c.col, true)
        }
        onPositionChanged: mouse => {
            if (!(mouse.buttons & Qt.LeftButton)) return
            const c = cell(mouse)
            term.selectUpdate(c.row, c.col, c.right)
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
        radius: 8
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
