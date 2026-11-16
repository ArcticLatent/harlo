pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Keyboard shortcuts grouped into sections, in one or two columns (the
// shortcuts sheet). `shortcuts`: [{ id, seq, label, group, icon, hidden }];
// rows with the same label in a group merge into one row with alternative
// keys. Keys are changed in Settings → Shortcuts (ShortcutSettings).
RowLayout {
    id: root
    property var shortcuts: []
    property bool twoColumns: width > 600
    // A row clicked: its action, to run.
    signal activated(var run)

    readonly property var sections: {
        const out = []
        for (const g of Keymap.groupOrder) {
            const rows = []
            for (const s of shortcuts) {
                if (s.hidden || s.group !== g)
                    continue
                const key = { seq: s.seq, id: s.id || "" }
                const same = rows.find(r => r.label === s.label)
                if (same) {
                    same.keys.push(key)
                    if (!same.run && s.run) same.run = s.run
                } else {
                    rows.push({ label: s.label, icon: s.icon || "", keys: [key], run: s.run || null })
                }
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

    spacing: 28

    component Section: ColumnLayout {
        id: section
        property var data_
        spacing: 2
        Layout.fillWidth: true
        Label {
            text: section.data_.title
            color: Theme.text
            font.pixelSize: Theme.px(13.5)
            font.weight: Font.DemiBold
            Layout.bottomMargin: 6
        }
        Repeater {
            model: section.data_.rows
            Rectangle {
                id: row
                required property var modelData
                // A row with an action runs it; the others don't light up.
                readonly property bool acts: !!modelData.run
                Layout.fillWidth: true
                implicitHeight: 34
                radius: 6
                color: acts && rowHover.hovered ? Theme.hover : "transparent"
                HoverHandler { id: rowHover; enabled: row.acts; cursorShape: Qt.PointingHandCursor }
                TapHandler { enabled: row.acts; onTapped: root.activated(row.modelData.run) }
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    spacing: 10
                    Icon {
                        name: row.modelData.icon
                        color: Theme.textMuted
                        implicitWidth: 16
                        implicitHeight: 16
                        Layout.preferredWidth: 18
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
                            id: keyRow
                            required property var modelData
                            required property int index
                            spacing: 3
                            Label { visible: keyRow.index > 0; text: "or"; color: Theme.muted; font.pixelSize: Theme.px(11); Layout.leftMargin: 2; Layout.rightMargin: 2 }
                            Repeater {
                                // "Ctrl+Shift+P" → Ctrl, Shift, P (a trailing "+" is a key).
                                model: keyRow.modelData.seq !== "" ? keyRow.modelData.seq.split(/\+(?!$)/) : ["—"]
                                KeyCap { required property string modelData; text: modelData }
                            }
                        }
                    }
                }
            }
        }
    }

    Repeater {
        model: root.twoColumns ? 2 : 1
        ColumnLayout {
            id: sectionColumn
            required property int index
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            Layout.alignment: Qt.AlignTop
            spacing: 18
            Repeater {
                model: root.columns[sectionColumn.index]
                Section { required property var modelData; data_: modelData }
            }
        }
    }
}
