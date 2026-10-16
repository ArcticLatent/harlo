import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Keyboard shortcuts grouped into sections, in one or two columns.
// `shortcuts`: [{ id, seq, label, group, glyph, hidden }]; rows with the
// same label in a group merge into one row with alternative keys.
// `editable` (Settings): click a key to record a new one; Esc cancels,
// Backspace removes it; a key taken by another action offers Replace.
RowLayout {
    id: root
    property var shortcuts: []
    property bool editable: false
    property bool twoColumns: width > 600
    readonly property var groupOrder: ["Navigation & Commands", "Session Management", "Interface", "Panels", "Composer"]

    readonly property var sections: {
        const out = []
        for (const g of groupOrder) {
            const rows = []
            for (const s of shortcuts) {
                if (s.hidden || s.group !== g)
                    continue
                const key = { seq: s.seq, id: s.id || "" }
                const same = rows.find(r => r.label === s.label)
                if (same)
                    same.keys.push(key)
                else
                    rows.push({ label: s.label, glyph: s.glyph || "•", keys: [key] })
            }
            if (rows.length)
                out.push({ title: g, rows: rows })
        }
        return out
    }
    // Left column takes sections until it holds about half the rows.
    readonly property var columns: {
        if (!twoColumns)
            return [sections, []]
        const total = sections.reduce((n, s) => n + s.rows.length + 2, 0)
        const left = [], right = []
        let n = 0
        for (const s of sections) {
            (n < total / 2 ? left : right).push(s)
            n += s.rows.length + 2
        }
        return [left, right]
    }
    // Test hook: number of rows shown.
    readonly property int rowCount: sections.reduce((n, s) => n + s.rows.length, 0)

    // ---- recording ---------------------------------------------------------
    property string recordingId: ""
    /// A key another action has: { id, key, other }.
    property var pending: null
    /// A message under the action's row: { id, text, error }.
    property var note: null
    onRecordingIdChanged: Keymap.recording = recordingId !== ""
    Component.onDestruction: Keymap.recording = false

    function labelOf(id) { const s = shortcuts.find(x => x.id === id); return s ? s.label : id }
    function startRecording(id) { pending = null; note = null; recordingId = id }
    function stopRecording() { recordingId = "" }
    readonly property var modifierKeys: [Qt.Key_Control, Qt.Key_Shift, Qt.Key_Alt, Qt.Key_Meta, Qt.Key_AltGr, Qt.Key_Super_L, Qt.Key_Super_R, Qt.Key_CapsLock]
    function captured(event) {
        const id = recordingId
        const plain = (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier | Qt.ShiftModifier)) === 0
        if (event.key === Qt.Key_Escape && plain) { stopRecording(); return }
        if ((event.key === Qt.Key_Backspace || event.key === Qt.Key_Delete) && plain) {
            Keymap.set(id, "")
            note = { id: id, text: "Removed; " + labelOf(id) + " has no shortcut now.", error: false }
            stopRecording()
            return
        }
        if (modifierKeys.includes(event.key)) return
        // Shift+Tab arrives as Backtab.
        const keyCode = event.key === Qt.Key_Backtab ? Qt.Key_Tab : event.key
        const key = Workspace.keySequenceText(keyCode, event.modifiers | (event.key === Qt.Key_Backtab ? Qt.ShiftModifier : 0))
        const fKey = keyCode >= Qt.Key_F1 && keyCode <= Qt.Key_F35
        if (!fKey && (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) === 0) {
            note = { id: id, text: key + " alone is for typing; add Ctrl, Alt or Meta.", error: true }
            return
        }
        if (Keymap.reserved[key]) {
            note = { id: id, text: key + " is taken by " + Keymap.reserved[key] + ".", error: true }
            return
        }
        const other = Keymap.owner(key, id)
        stopRecording()
        if (other !== "") {
            pending = { id: id, key: key, other: other }
            return
        }
        Keymap.set(id, key)
        note = null
    }
    function replace() {
        const p = pending
        Keymap.set(p.other, "")
        Keymap.set(p.id, p.key)
        note = { id: p.id, text: labelOf(p.other) + " no longer has a shortcut.", error: false }
        pending = null
    }

    spacing: 28

    component Keys_: RowLayout {
        id: keysRow
        property var key   // { seq, id }
        readonly property bool recording: root.recordingId !== "" && root.recordingId === key.id
        spacing: 3
        // Recording: a capsule that takes the next key press.
        Rectangle {
            visible: keysRow.recording
            implicitWidth: recLabel.implicitWidth + 16
            implicitHeight: 22
            radius: 5
            color: Qt.rgba(Theme.primary.r, Theme.primary.g, Theme.primary.b, 0.15)
            border.color: Theme.primary
            Label { id: recLabel; anchors.centerIn: parent; text: "Press keys…"; color: Theme.primary; font.pixelSize: Theme.px(11) }
            Item {
                id: recorder
                objectName: "keyRecorder"
                focus: keysRow.recording
                Keys.onShortcutOverride: event => event.accepted = true
                Keys.onPressed: event => { root.captured(event); event.accepted = true }
                onActiveFocusChanged: if (!activeFocus && keysRow.recording) root.stopRecording()
            }
            onVisibleChanged: if (visible) recorder.forceActiveFocus()
        }
        Repeater {
            // "Ctrl+Shift+P" → Ctrl, Shift, P (a trailing "+" is a key).
            model: keysRow.recording ? [] : keysRow.key.seq !== "" ? keysRow.key.seq.split(/\+(?!$)/) : [root.editable ? "Not set" : "—"]
            KeyCap { required property string modelData; text: modelData; opacity: keysRow.key.seq === "" ? 0.6 : 1 }
        }
        HoverHandler { id: keyHover; enabled: root.editable && keysRow.key.id !== ""; cursorShape: Qt.PointingHandCursor }
        TapHandler { enabled: root.editable && keysRow.key.id !== "" && !keysRow.recording; onTapped: root.startRecording(keysRow.key.id) }
        ToolTip.visible: keyHover.hovered && !keysRow.recording
        ToolTip.text: "Click to change"
        // Back to the default.
        Button {
            id: resetKey
            visible: root.editable && keysRow.key.id !== "" && Keymap.isCustom(keysRow.key.id) && !keysRow.recording
            flat: true
            padding: 0
            implicitWidth: 20
            implicitHeight: 20
            onClicked: { Keymap.reset(keysRow.key.id); root.note = null }
            contentItem: Label { text: "↺"; color: resetKey.hovered ? Theme.foreground : Theme.muted; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter; font.pixelSize: Theme.px(13) }
            background: Rectangle { radius: 4; color: resetKey.hovered ? Theme.hover : "transparent" }
            ToolTip.visible: hovered
            ToolTip.text: "Reset to " + (Keymap.defaults[keysRow.key.id] || "none")
        }
    }

    component Section: ColumnLayout {
        id: section
        property var data_
        spacing: 2
        Layout.fillWidth: true
        Label {
            text: section.data_.title.toUpperCase()
            color: Theme.muted
            font.pixelSize: Theme.px(11)
            font.bold: true
            font.letterSpacing: 0.8
            Layout.bottomMargin: 4
        }
        Repeater {
            model: section.data_.rows
            ColumnLayout {
                id: row
                required property var modelData
                readonly property var ids: modelData.keys.map(k => k.id)
                readonly property bool hasPending: root.pending !== null && ids.includes(root.pending.id)
                readonly property bool hasNote: root.note !== null && ids.includes(root.note.id)
                Layout.fillWidth: true
                spacing: 0
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: 34
                    radius: 6
                    color: rowHover.hovered ? Theme.hover : "transparent"
                    HoverHandler { id: rowHover }
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 8
                        anchors.rightMargin: 8
                        spacing: 10
                        Rectangle {
                            implicitWidth: 24
                            implicitHeight: 24
                            radius: 6
                            color: Theme.subtle
                            Label { anchors.centerIn: parent; text: row.modelData.glyph; color: Theme.primary; font.pixelSize: Theme.px(12) }
                        }
                        Label {
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            text: row.modelData.label
                            color: Theme.foreground
                            font.pixelSize: Theme.px(13)
                            elide: Text.ElideRight
                        }
                        Repeater {
                            model: row.modelData.keys
                            RowLayout {
                                required property var modelData
                                required property int index
                                spacing: 3
                                Label { visible: index > 0; text: "or"; color: Theme.muted; font.pixelSize: Theme.px(11); Layout.leftMargin: 2; Layout.rightMargin: 2 }
                                Keys_ { key: modelData }
                            }
                        }
                    }
                }
                // A key another action has: replace it there?
                RowLayout {
                    visible: row.hasPending
                    Layout.fillWidth: true
                    Layout.leftMargin: 42
                    Layout.bottomMargin: 4
                    spacing: 8
                    Label {
                        Layout.fillWidth: true
                        text: row.hasPending ? root.pending.key + " is " + root.labelOf(root.pending.other) + "." : ""
                        color: Theme.warning
                        font.pixelSize: Theme.px(12)
                        wrapMode: Text.Wrap
                    }
                    FlatButton { text: "Replace"; font.pixelSize: Theme.px(12); accent: Theme.primary; onClicked: root.replace() }
                    FlatButton { text: "Cancel"; font.pixelSize: Theme.px(12); onClicked: root.pending = null }
                }
                Label {
                    visible: row.hasNote && !row.hasPending
                    Layout.fillWidth: true
                    Layout.leftMargin: 42
                    Layout.bottomMargin: 4
                    text: row.hasNote ? root.note.text : ""
                    color: row.hasNote && root.note.error ? Theme.error : Theme.muted
                    font.pixelSize: Theme.px(12)
                    wrapMode: Text.Wrap
                }
            }
        }
    }

    Repeater {
        model: root.twoColumns ? 2 : 1
        ColumnLayout {
            required property int index
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            Layout.alignment: Qt.AlignTop
            spacing: 18
            Repeater {
                model: root.columns[index]
                Section { required property var modelData; data_: modelData }
            }
        }
    }
}
