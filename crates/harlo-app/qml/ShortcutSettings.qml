pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Settings → Shortcuts: the actions whose keys can be changed, one card per
// group. Click a key (or Tab to it and press Enter) and press the new keys:
// Esc cancels, Backspace removes it, and a key another action has offers
// Replace. Fixed keys (the composer's, a file's, the image viewer's) are
// in the shortcuts sheet, one click away.
ColumnLayout {
    id: root
    /// The registry: [{ id, seq, label, group }]; rows without an id are fixed.
    property var shortcuts: []
    /// "Show every key": the shortcuts sheet.
    signal sheetRequested()
    spacing: 0

    readonly property var sections: {
        const out = []
        for (const g of Keymap.groupOrder) {
            const rows = []
            for (const s of shortcuts) {
                if (!s.id || s.hidden || s.group !== g)
                    continue
                const same = rows.find(r => r.label === s.label)
                if (same)
                    same.keys.push({ seq: s.seq, id: s.id })
                else
                    rows.push({ label: s.label, keys: [{ seq: s.seq, id: s.id }] })
            }
            if (rows.length)
                out.push({ title: g, rows: rows })
        }
        return out
    }

    // ---- recording ---------------------------------------------------------
    property string recordingId: ""
    /// A key another action has: { id, key, other }.
    property var pending: null
    /// A message under the action's row: { id, text, error }.
    property var note: null
    onRecordingIdChanged: Keymap.recording = recordingId !== ""
    Component.onDestruction: Keymap.recording = false

    function labelOf(id) { const s = shortcuts.find(x => x.id === id); return s ? s.label : id }
    function startRecording(chip) { pending = null; note = null; recordingId = chip.key.id; chip.forceActiveFocus() }
    function stopRecording() { recordingId = "" }
    readonly property var modifierKeys: [Qt.Key_Control, Qt.Key_Shift, Qt.Key_Alt, Qt.Key_Meta, Qt.Key_AltGr, Qt.Key_Super_L, Qt.Key_Super_R, Qt.Key_CapsLock]
    function captured(event) {
        const id = recordingId
        const plain = (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier | Qt.ShiftModifier)) === 0
        if (event.key === Qt.Key_Escape && plain) { stopRecording(); return }
        if ((event.key === Qt.Key_Backspace || event.key === Qt.Key_Delete) && plain) {
            Keymap.set(id, "")
            note = { id: id, text: "Removed; “" + labelOf(id) + "” has no shortcut now.", error: false }
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
        note = { id: p.id, text: p.key + " no longer does “" + labelOf(p.other) + "”.", error: false }
        pending = null
    }

    // One action's key: a solid chip that records the next keys when clicked.
    component KeyChip: Rectangle {
        id: chip
        required property var key   // { seq, id }
        readonly property bool recording: root.recordingId === key.id
        readonly property bool unset: key.seq === ""
        objectName: "keyChip-" + key.id
        implicitWidth: Math.max(56, chipLabel.implicitWidth + 20)
        implicitHeight: 26
        radius: 6
        color: recording ? Theme.accentSoft
             : chipHover.hovered ? Theme.controlFillHover
             : unset ? "transparent" : Theme.controlFill
        border.width: recording || unset || activeFocus ? 1 : 0
        border.color: recording ? Theme.primary : activeFocus ? Theme.textMuted : Theme.border
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: (unset ? "Not set" : key.seq) + ", change"
        Label {
            id: chipLabel
            anchors.centerIn: parent
            text: chip.recording ? "Press keys…" : chip.unset ? "Not set" : chip.key.seq.replace(/\bUp$/, "↑").replace(/\bDown$/, "↓").replace(/\bLeft$/, "←").replace(/\bRight$/, "→")
            color: chip.recording ? Theme.primary : chip.unset ? Theme.textFaint : Theme.text
            font.family: chip.recording || chip.unset ? Theme.uiFont : Theme.mono
            font.pixelSize: Theme.px(12)
        }
        HoverHandler { id: chipHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { enabled: !chip.recording; onTapped: root.startRecording(chip) }
        Keys.onShortcutOverride: event => event.accepted = chip.recording
        Keys.onPressed: event => {
            if (chip.recording) {
                root.captured(event)
                event.accepted = true
            } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                root.startRecording(chip)
                event.accepted = true
            }
        }
        onActiveFocusChanged: if (!activeFocus && recording) root.stopRecording()
    }

    component ActionRow: ColumnLayout {
        id: row
        required property var modelData
        required property int index
        readonly property var ids: modelData.keys.map(k => k.id)
        readonly property bool hasPending: root.pending !== null && ids.includes(root.pending.id)
        readonly property bool hasNote: root.note !== null && ids.includes(root.note.id)
        Layout.fillWidth: true
        spacing: 0
        Rectangle {
            visible: row.index > 0
            Layout.fillWidth: true
            Layout.leftMargin: 16
            Layout.rightMargin: 16
            implicitHeight: 1
            color: Theme.hairline
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 46
            Layout.leftMargin: 16
            Layout.rightMargin: 10
            spacing: 6
            Label {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                text: row.modelData.label
                color: Theme.text
                font.pixelSize: Theme.px(13.5)
                elide: Text.ElideRight
            }
            Repeater {
                model: row.modelData.keys
                RowLayout {
                    id: keySlot
                    required property var modelData
                    required property int index
                    spacing: 2
                    Label { visible: keySlot.index > 0; text: "or"; color: Theme.textFaint; font.pixelSize: Theme.px(12); Layout.leftMargin: 4; Layout.rightMargin: 4 }
                    KeyChip { key: keySlot.modelData }
                    // Back to the default; the slot keeps its room so the
                    // keys stay in one column.
                    Item {
                        implicitWidth: 24
                        implicitHeight: 24
                        Loader {
                            anchors.fill: parent
                            active: Keymap.isCustom(keySlot.modelData.id) && root.recordingId !== keySlot.modelData.id
                            sourceComponent: Button {
                                id: resetKey
                                flat: true
                                padding: 0
                                onClicked: { Keymap.reset(keySlot.modelData.id); root.note = null }
                                contentItem: Item { Icon { anchors.centerIn: parent; name: "arrow-go-back"; color: resetKey.hovered ? Theme.text : Theme.textMuted; implicitWidth: 13; implicitHeight: 13 } }
                                background: Rectangle { radius: 5; color: resetKey.hovered ? Theme.hover : "transparent" }
                                Accessible.name: "Reset to " + (Keymap.defaults[keySlot.modelData.id] || "none")
                                ToolTip.visible: hovered
                                ToolTip.text: Accessible.name
                            }
                        }
                    }
                }
            }
        }
        // A key another action has: replace it there?
        Loader {
            active: row.hasPending
            visible: active
            Layout.fillWidth: true
            sourceComponent: RowLayout {
                spacing: 8
                Label {
                    Layout.fillWidth: true
                    Layout.leftMargin: 16
                    text: root.pending ? root.pending.key + " is taken by “" + root.labelOf(root.pending.other) + "”." : ""
                    color: Theme.warning
                    font.pixelSize: Theme.px(12.5)
                    wrapMode: Text.Wrap
                }
                FlatButton { text: "Replace"; font.pixelSize: Theme.px(12); accent: Theme.primary; onClicked: root.replace() }
                FlatButton { text: "Cancel"; font.pixelSize: Theme.px(12); Layout.rightMargin: 10; onClicked: root.pending = null }
            }
        }
        Loader {
            active: row.hasNote && !row.hasPending
            visible: active
            Layout.fillWidth: true
            sourceComponent: Label {
                leftPadding: 16
                rightPadding: 16
                bottomPadding: 12
                text: root.note ? root.note.text : ""
                color: root.note && root.note.error ? Theme.error : Theme.textMuted
                font.pixelSize: Theme.px(12.5)
                wrapMode: Text.Wrap
            }
        }
    }

    SectionHeader {
        subtitle: "Click a key, then press the new keys. Esc cancels; Backspace leaves the action without one."
        FlatButton {
            visible: Object.keys(Keymap.overrides).length > 0
            text: "Reset all"
            font.pixelSize: Theme.px(12)
            onClicked: Keymap.resetAll()
        }
    }
    Repeater {
        model: root.sections
        ColumnLayout {
            id: section
            required property var modelData
            required property int index
            Layout.fillWidth: true
            Layout.topMargin: section.index > 0 ? 22 : 16
            spacing: 0
            Label {
                text: section.modelData.title
                color: Theme.textMuted
                font.pixelSize: Theme.px(12.5)
                font.weight: Font.Medium
                Layout.leftMargin: 2
            }
            Group {
                Layout.topMargin: 8
                Repeater {
                    model: section.modelData.rows
                    ActionRow { }
                }
            }
        }
    }
    RowLayout {
        Layout.fillWidth: true
        Layout.topMargin: 22
        spacing: 12
        FormNote {
            Layout.fillWidth: true
            color: Theme.textMuted
            text: "Keys that can't be changed, like Enter to send or the file editor's and image viewer's, are in the shortcuts sheet."
        }
        FlatButton {
            text: "Show every key"
            font.pixelSize: Theme.px(12)
            onClicked: root.sheetRequested()
        }
    }
}
