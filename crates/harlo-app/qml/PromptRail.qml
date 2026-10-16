import QtQuick
import QtQuick.Controls
import harlo

// The prompt navigator: one tick per message you sent,
// at the transcript's right edge. Hovering scrubs through a list of the
// prompts and clicking jumps there; the wheel steps through them. Ctrl+J
// opens it for the keyboard: ↑ ↓ Home End move, Enter jumps, Esc closes.
Item {
    id: rail
    /// [{i: row index, text: preview}]
    property var prompts: []
    /// The prompt (index into `prompts`) whose turn is in view.
    property int activeIndex: -1
    property real maxPanelHeight: 440
    property real maxPanelWidth: 320
    signal picked(int row)
    /// Keyboard mode ended (jumped or Esc): focus can go back.
    signal finished()

    // At most 30 ticks show; hovering the ends carousels through the rest.
    readonly property int maxTicks: 30
    readonly property real pitch: 12
    readonly property real edgeZone: 18
    readonly property int count: prompts.length
    readonly property int visibleCount: Math.min(count, maxTicks)
    readonly property int maxStart: Math.max(0, count - visibleCount)
    property int windowStart: 0
    readonly property int start: Math.min(windowStart, maxStart)
    readonly property bool moreAbove: start > 0
    readonly property bool moreBelow: start + visibleCount < count
    /// The prompt under the pointer or keyboard; -1 while the rail is idle.
    property int highlighted: -1
    property bool keyboard: false
    property real pointerY: -1

    implicitWidth: 28
    implicitHeight: visibleCount * pitch
    visible: count >= 2

    function clampStart(s) { return Math.max(0, Math.min(maxStart, s)) }
    // While idle the tape glides so the prompt in view stays centred.
    function recenter() {
        if (highlighted >= 0) return
        const target = activeIndex >= 0 ? activeIndex : count - 1
        windowStart = clampStart(target - Math.floor(visibleCount / 2))
    }
    onActiveIndexChanged: recenter()
    onCountChanged: recenter()

    function ensureVisible(index) {
        if (index < start) windowStart = index
        else if (index >= start + visibleCount) windowStart = clampStart(index - visibleCount + 1)
        else windowStart = start
    }
    function moveTo(index) {
        if (count === 0) return
        const next = Math.max(0, Math.min(count - 1, index))
        ensureVisible(next)
        highlighted = next
    }
    function slotAt(y) { return Math.max(0, Math.min(visibleCount - 1, Math.floor(y / pitch))) }
    function select(index) {
        if (index < 0 || index >= count) return
        const row = prompts[index].i
        carousel.dir = 0
        highlighted = -1
        if (keyboard) { keyboard = false; finished() }
        recenter()
        picked(row)
    }
    function close() {
        highlighted = -1
        if (keyboard) { keyboard = false; finished() }
        recenter()
    }
    /// Keyboard mode, starting at the prompt in view.
    function openKeyboard() {
        if (count < 2) return
        keyboard = true
        forceActiveFocus()
        moveTo(activeIndex >= 0 ? activeIndex : count - 1)
    }

    // Tick lengths: a wave around the highlighted tick.
    readonly property var falloff: [1, 0.6, 0.35, 0.15]
    function tickWidth(index) {
        const base = index === activeIndex ? 14 : 10
        if (highlighted < 0) return base
        const f = falloff[Math.abs(index - highlighted)] || 0
        return Math.round(base + (20 - base) * f)
    }
    // Ticks fade at an end that has more beyond it.
    function edgeFade(index) {
        const rel = index - start
        if (rel < 0 || rel >= visibleCount) return 0
        if (moreAbove && rel < 2) return (rel + 1) / 3
        if (moreBelow && rel > visibleCount - 3) return (visibleCount - rel) / 3
        return 1
    }

    Timer {
        id: carousel
        property int dir: 0
        interval: 80
        repeat: true
        running: dir !== 0
        onTriggered: {
            const next = rail.clampStart(rail.start + dir)
            if (next === rail.start) { dir = 0; return }
            rail.windowStart = next
            if (rail.pointerY >= 0) rail.highlighted = Math.min(rail.count - 1, next + rail.slotAt(rail.pointerY))
        }
    }
    // Leaving the rail closes the list after a moment, so the pointer can
    // travel into it.
    Timer { id: hideTimer; interval: 160; onTriggered: if (!rail.keyboard) rail.close() }

    property real wheelRest: 0
    function wheelStep(dy) {
        wheelRest += dy
        const steps = Math.trunc(wheelRest / 120)
        if (steps === 0) return
        wheelRest -= steps * 120
        moveTo((highlighted >= 0 ? highlighted : (activeIndex >= 0 ? activeIndex : count - 1)) - steps)
    }

    Keys.onPressed: event => {
        if (!keyboard) return
        const cur = highlighted >= 0 ? highlighted : (activeIndex >= 0 ? activeIndex : count - 1)
        switch (event.key) {
        case Qt.Key_Up: moveTo(cur - 1); break
        case Qt.Key_Down: moveTo(cur + 1); break
        case Qt.Key_Home: moveTo(0); break
        case Qt.Key_End: moveTo(count - 1); break
        case Qt.Key_PageUp: moveTo(cur - 8); break
        case Qt.Key_PageDown: moveTo(cur + 8); break
        case Qt.Key_Return: case Qt.Key_Enter: case Qt.Key_Space: select(cur); break
        case Qt.Key_Escape: close(); break
        default: return
        }
        event.accepted = true
    }
    onActiveFocusChanged: if (!activeFocus && keyboard) { keyboard = false; highlighted = -1; recenter() }

    // The tape: ticks keep their place on the strip and the strip glides.
    // As wide as the longest tick, even when the rail itself is narrow.
    Item {
        anchors.right: parent.right
        width: 28
        height: parent.height
        clip: true
        Item {
            width: parent.width
            y: -rail.start * rail.pitch
            Behavior on y { NumberAnimation { duration: 300; easing.type: Easing.OutCubic } }
            Repeater {
                model: rail.count
                Item {
                    required property int index
                    y: index * rail.pitch
                    width: 28
                    height: rail.pitch
                    Rectangle {
                        anchors.right: parent.right
                        anchors.rightMargin: 4
                        anchors.verticalCenter: parent.verticalCenter
                        height: 2
                        radius: 1
                        width: rail.tickWidth(index)
                        color: Theme.foreground
                        opacity: (index === rail.activeIndex ? 1 : index === rail.highlighted ? 0.8 : 0.3) * rail.edgeFade(index)
                        Behavior on width { NumberAnimation { duration: 200; easing.type: Easing.OutCubic } }
                        Behavior on opacity { NumberAnimation { duration: 200 } }
                    }
                }
            }
        }
    }

    // The whole strip is one target: the pointer's height picks the tick.
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onPositionChanged: mouse => {
            hideTimer.stop()
            rail.pointerY = mouse.y
            rail.highlighted = Math.min(rail.count - 1, rail.start + rail.slotAt(mouse.y))
            carousel.dir = mouse.y <= rail.edgeZone && rail.moreAbove ? -1
                         : mouse.y >= rail.height - rail.edgeZone && rail.moreBelow ? 1 : 0
        }
        onExited: { rail.pointerY = -1; carousel.dir = 0; hideTimer.restart() }
        onClicked: mouse => rail.select(Math.min(rail.count - 1, rail.start + rail.slotAt(mouse.y)))
        onWheel: wheel => rail.wheelStep(wheel.angleDelta.y)
    }

    // The list: a window onto all prompts that glides with the highlight,
    // with two rows of context kept above and below it.
    Rectangle {
        id: panel
        readonly property real rowH: Theme.px(54)
        readonly property int rows: Math.max(1, Math.min(rail.count, 8, Math.floor((rail.maxPanelHeight - 10) / rowH)))
        visible: rail.highlighted >= 0
        x: -width - 12
        y: (rail.height - height) / 2
        width: Math.min(320, rail.maxPanelWidth)
        height: rows * rowH + 8
        radius: 12
        color: Theme.elevated
        border.color: Theme.border

        property real offset: 0
        property bool snap: false
        readonly property real maxOffset: Math.max(0, rail.count * rowH - rows * rowH)
        // Opening centres the highlight; afterwards the list moves only
        // when the highlight nears an edge.
        property bool opened: false
        function follow() {
            const h = rail.highlighted
            if (h < 0) { opened = false; return }
            const fresh = !opened
            opened = true
            let off = offset
            if (fresh) {
                off = h * rowH - (rows - 1) * rowH / 2
            } else {
                const highest = (h - 2) * rowH, lowest = (h + 3) * rowH - rows * rowH
                if (off > highest) off = highest
                else if (off < lowest) off = lowest
            }
            off = Math.max(0, Math.min(maxOffset, off))
            if (fresh) { snap = true; offset = off; snap = false } else offset = off
        }
        Connections { target: rail; function onHighlightedChanged() { panel.follow() } }

        HoverHandler { onHoveredChanged: hovered ? hideTimer.stop() : hideTimer.restart() }

        Item {
            id: listClip
            anchors.fill: parent
            anchors.topMargin: 4
            anchors.bottomMargin: 4
            clip: true
            Item {
                width: parent.width
                y: -panel.offset
                Behavior on y { enabled: !panel.snap; NumberAnimation { duration: 200; easing.type: Easing.OutCubic } }
                Repeater {
                    model: panel.visible ? rail.count : 0
                    Item {
                        id: entry
                        required property int index
                        readonly property bool active: index === rail.activeIndex
                        readonly property bool lit: index === rail.highlighted
                        // Only rows near the window are drawn.
                        visible: (index + 1) * panel.rowH >= panel.offset - panel.rowH * 2 && index * panel.rowH <= panel.offset + panel.height + panel.rowH * 2
                        x: 6
                        y: index * panel.rowH + 4
                        width: panel.width - 12
                        height: panel.rowH - 8
                        Rectangle {
                            anchors.fill: parent
                            radius: 8
                            color: entry.active ? Qt.tint(Theme.elevated, Theme.selection) : entry.lit ? Theme.hover : "transparent"
                            border.color: entry.active ? "transparent" : Qt.rgba(Theme.border.r, Theme.border.g, Theme.border.b, entry.lit ? 1 : 0.6)
                        }
                        Label {
                            anchors.fill: parent
                            anchors.leftMargin: 10
                            anchors.rightMargin: 10
                            verticalAlignment: Text.AlignVCenter
                            text: rail.prompts[entry.index] && rail.prompts[entry.index].text !== "" ? rail.prompts[entry.index].text : "No text"
                            textFormat: Text.PlainText
                            wrapMode: Text.WrapAnywhere
                            maximumLineCount: 2
                            elide: Text.ElideRight
                            font.pixelSize: Theme.px(12)
                            color: entry.active || entry.lit ? Theme.foreground : Theme.muted
                        }
                        HoverHandler {
                            cursorShape: Qt.PointingHandCursor
                            onPointChanged: if (hovered && rail.highlighted !== entry.index) { hideTimer.stop(); rail.moveTo(entry.index) }
                        }
                        TapHandler { onTapped: rail.select(entry.index) }
                    }
                }
            }
        }
        // Fades where the list goes on.
        Rectangle {
            visible: panel.offset > 0
            anchors.top: listClip.top
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.margins: 1
            height: 18
            gradient: Gradient {
                GradientStop { position: 0; color: Theme.elevated }
                GradientStop { position: 1; color: Qt.rgba(Theme.elevated.r, Theme.elevated.g, Theme.elevated.b, 0) }
            }
        }
        Rectangle {
            visible: panel.offset < panel.maxOffset
            anchors.bottom: listClip.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.margins: 1
            height: 18
            gradient: Gradient {
                GradientStop { position: 0; color: Qt.rgba(Theme.elevated.r, Theme.elevated.g, Theme.elevated.b, 0) }
                GradientStop { position: 1; color: Theme.elevated }
            }
        }
        // The wheel steps through prompts instead of scrolling the chat.
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.NoButton
            onWheel: wheel => rail.wheelStep(wheel.angleDelta.y)
        }
    }
}
