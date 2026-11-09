import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Every keyboard shortcut, grouped into sections over two columns.
// `shortcuts`: [{ seq, label, group, icon, hidden }]; rows with the same
// label in a group merge into one row with alternative keys.
Popup {
    id: root
    property var shortcuts: []
    // Test hook: number of rows shown.
    readonly property int rowCount: list.rowCount

    x: Math.round((parent.width - width) / 2)
    y: Math.round((parent.height - height) / 2)
    width: Math.min(860, parent.width - 40)
    height: Math.min(implicitHeight, parent.height - 40)
    padding: 0
    modal: true
    focus: true
    background: GlassSurface { radius: 14; kind: "dialog" }
    Overlay.modal: GlassScrim { }


    function grab(path) { card.grabToImage(r => r.saveToFile(path)) }

    // The card is inside contentItem so tests can grab it.
    contentItem: Item {
        implicitHeight: card.implicitHeight
        Rectangle {
            id: card
            anchors.fill: parent
            implicitHeight: cardBody.implicitHeight
            radius: 14
            // The glass behind it (the background) shows through.
            color: "transparent"
            ColumnLayout {
                id: cardBody
                anchors.fill: parent
                spacing: 0

                // Header
                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 20
                    Layout.bottomMargin: 16
                    spacing: 12
                    Rectangle {
                        implicitWidth: 36
                        implicitHeight: 36
                        radius: 9
                        color: Theme.selected
                        Icon { anchors.centerIn: parent; name: "keyboard"; color: Theme.primary; implicitWidth: 20; implicitHeight: 20 }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Label { Layout.fillWidth: true; text: "Keyboard shortcuts"; color: Theme.foreground; font.pixelSize: Theme.px(16); font.weight: Font.DemiBold }
                        Label { text: "Get around harlo without leaving the keyboard"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                    }
                    Button {
                        id: closeButton
                        flat: true
                        implicitWidth: 30
                        implicitHeight: 30
                        onClicked: root.close()
                        contentItem: Icon { name: "close"; color: closeButton.hovered ? Theme.foreground : Theme.muted }
                        background: Rectangle { radius: 6; color: closeButton.hovered ? Theme.hover : "transparent" }
                        ToolTip.visible: hovered
                        ToolTip.text: "Close (Esc)"
                    }
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.border }

                // Sections
                Flickable {
                    id: flick
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.preferredHeight: list.implicitHeight + 32
                    contentHeight: list.implicitHeight + 32
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar { }
                    ShortcutList {
                        id: list
                        x: 20
                        y: 16
                        width: flick.width - 40
                        shortcuts: root.shortcuts
                        twoColumns: root.width > 640
                    }
                }

                // Tips
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.border }
                Rectangle {
                    Layout.fillWidth: true
                    Layout.leftMargin: 1
                    Layout.rightMargin: 1
                    Layout.bottomMargin: 1
                    implicitHeight: tips.implicitHeight + 28
                    color: Theme.subtle
                    radius: 12
                    // Square off the top corners.
                    Rectangle { width: parent.width; height: 12; color: parent.color }
                    // The palette's two prefixes (the rest is in the list).
                    Flow {
                        id: tips
                        objectName: "shortcutTips"
                        x: 20
                        y: 14
                        width: parent.width - 40
                        spacing: 4
                        Repeater {
                            model: {
                                const out = []
                                const palette = Keymap.seq("palette")
                                if (palette !== "")
                                    for (const k of palette.split(/\+(?!$)/)) out.push({ key: k })
                                for (const w of "opens on sessions and actions. Start with".split(" ")) out.push({ word: w })
                                out.push({ key: ">" })
                                for (const w of "for actions only, or".split(" ")) out.push({ word: w })
                                out.push({ key: "@" })
                                for (const w of "for a file in the session's folder.".split(" ")) out.push({ word: w })
                                return out
                            }
                            Item {
                                required property var modelData
                                implicitWidth: modelData.key !== undefined ? cap.implicitWidth : word.implicitWidth
                                implicitHeight: 22
                                width: implicitWidth
                                height: implicitHeight
                                KeyCap { id: cap; visible: modelData.key !== undefined; text: modelData.key || ""; anchors.verticalCenter: parent.verticalCenter }
                                Label { id: word; visible: modelData.word !== undefined; text: modelData.word || ""; color: Theme.textMuted; font.pixelSize: Theme.px(12); anchors.verticalCenter: parent.verticalCenter }
                            }
                        }
                    }
                }
            }
        }
    }
}
