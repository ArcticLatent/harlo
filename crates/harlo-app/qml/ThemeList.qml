pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic
import harlo

// The themes to choose from: each a small harlo window in its colours, in
// the scheme chosen above (light or dark), with a radio and its name under
// it. The chosen one is ringed.
GridLayout {
    id: list
    // [{ name, light: [bg, fg, accent, elevated, muted text, sidebar], dark: […] }]
    property var families: []
    // Each theme's objectName is this plus its name.
    property string objectPrefix: "theme-"
    Layout.fillWidth: true
    columns: Math.max(2, Math.floor((width + columnSpacing) / 176))
    columnSpacing: 16
    rowSpacing: 16
    Repeater {
        model: list.families
        Item {
            id: themeItem
            required property var modelData
            objectName: list.objectPrefix + modelData.name
            readonly property bool current: Workspace.themeFamily === modelData.name
            // The scheme showing now, else the one the theme has.
            readonly property var cols: {
                const want = modelData[Theme.variant] || []
                return want[0] ? want : modelData[Theme.variant === "light" ? "dark" : "light"]
            }
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            implicitHeight: preview.height + 8 + nameRow.height
            activeFocusOnTab: true
            Accessible.role: Accessible.RadioButton
            Accessible.name: modelData.name
            Accessible.checked: current
            Keys.onSpacePressed: Workspace.chooseTheme(modelData.name)
            Keys.onReturnPressed: Workspace.chooseTheme(modelData.name)
            HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: Workspace.chooseTheme(themeItem.modelData.name) }

            // The ring: accent when chosen (or focused), a quiet one on hover.
            Rectangle {
                anchors.fill: preview
                anchors.margins: -3
                radius: preview.radius + 3
                color: "transparent"
                border.width: 2
                border.color: themeItem.current || themeItem.activeFocus ? Theme.accent : hover.hovered ? Theme.border : "transparent"
                Behavior on border.color { ColorAnimation { duration: Theme.hoverMotion } }
            }
            ThemePreview {
                id: preview
                x: 3
                y: 3
                width: parent.width - 6
                height: Math.round(width * 0.56)
                colors: themeItem.cols
            }
            Row {
                id: nameRow
                x: 2
                y: preview.y + preview.height + 8
                width: parent.width - 4
                spacing: 8
                // A radio: a ring, filled with a dot when chosen.
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.px(16)
                    height: width
                    radius: width / 2
                    color: "transparent"
                    border.width: themeItem.current ? 0 : 1.5
                    border.color: hover.hovered ? Theme.textMuted : Theme.textFaint
                    Rectangle {
                        visible: themeItem.current
                        anchors.fill: parent
                        radius: width / 2
                        color: Theme.accent
                        Rectangle {
                            anchors.centerIn: parent
                            width: parent.width * 0.4
                            height: width
                            radius: width / 2
                            color: Theme.background
                        }
                    }
                }
                Label {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - Theme.px(16) - parent.spacing
                    text: themeItem.modelData.name
                    color: themeItem.current ? Theme.text : Theme.textMuted
                    font.pixelSize: Theme.px(13)
                    font.weight: themeItem.current ? Font.Medium : Font.Normal
                    elide: Text.ElideRight
                }
            }
        }
    }

    // harlo's window, small, in one theme's colours: the sidebar with the
    // open session lit in the accent, a conversation, the composer.
    component ThemePreview: Rectangle {
        id: pv
        property var colors: []
        readonly property color bg: colors[0] || "#202020"
        readonly property color fg: colors[1] || "#e0e0e0"
        readonly property color acc: colors[2] || "#5b9bf0"
        readonly property color card: colors[3] || bg
        readonly property color mut: colors[4] || fg
        readonly property color side: colors[5] || card
        function a(c, alpha) { return Qt.rgba(c.r, c.g, c.b, alpha) }
        readonly property real u: width / 160
        radius: 3
        color: bg
        border.color: a(fg, 0.14)
        // The sidebar (square where it meets the conversation).
        Rectangle {
            x: 1
            y: 1
            width: Math.round(pv.width * 0.38)
            height: pv.height - 2
            radius: pv.radius - 1
            color: pv.side
            Rectangle { x: parent.width - width; width: pv.radius; height: parent.height; color: parent.color }
            Column {
                x: 7 * pv.u
                y: 8 * pv.u
                width: parent.width - 14 * pv.u
                spacing: 6 * pv.u
                Rectangle { width: parent.width * 0.5; height: 4 * pv.u; radius: height / 2; color: pv.a(pv.fg, 0.35) }
                // The open session.
                Rectangle {
                    width: parent.width
                    height: 11 * pv.u
                    radius: 3 * pv.u
                    color: pv.acc
                    Rectangle { x: 5 * pv.u; anchors.verticalCenter: parent.verticalCenter; width: parent.width * 0.55; height: 3.5 * pv.u; radius: height / 2; color: pv.a(pv.bg, 0.9) }
                }
                Repeater {
                    model: [0.75, 0.6, 0.7]
                    Row {
                        id: barRow
                        required property real modelData
                        spacing: 4 * pv.u
                        Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 3.5 * pv.u; height: width; radius: width / 2; color: pv.a(pv.mut, 0.7) }
                        Rectangle { width: (pv.width * 0.38 - 22 * pv.u) * barRow.modelData; height: 3.5 * pv.u; radius: height / 2; color: pv.a(pv.mut, 0.55) }
                    }
                }
            }
        }
        // The conversation: a reply's lines, then a message of yours.
        Column {
            x: pv.width * 0.38 + 9 * pv.u
            y: 9 * pv.u
            width: pv.width * 0.62 - 18 * pv.u
            spacing: 4.5 * pv.u
            Rectangle { width: parent.width * 0.85; height: 3.5 * pv.u; radius: height / 2; color: pv.a(pv.fg, 0.7) }
            Rectangle { width: parent.width * 0.65; height: 3.5 * pv.u; radius: height / 2; color: pv.a(pv.fg, 0.7) }
            Rectangle { width: parent.width * 0.75; height: 3.5 * pv.u; radius: height / 2; color: pv.a(pv.mut, 0.5) }
            Item { width: 1; height: 1 * pv.u }
            Rectangle {
                x: parent.width - width
                width: parent.width * 0.55
                height: 11 * pv.u
                radius: 4 * pv.u
                color: pv.card
                border.color: pv.a(pv.fg, 0.08)
                Rectangle { x: 5 * pv.u; anchors.verticalCenter: parent.verticalCenter; width: parent.width * 0.6; height: 3 * pv.u; radius: height / 2; color: pv.a(pv.fg, 0.6) }
            }
        }
        // The composer, its send button in the accent.
        Rectangle {
            x: pv.width * 0.38 + 7 * pv.u
            y: pv.height - height - 7 * pv.u
            width: pv.width * 0.62 - 14 * pv.u
            height: 15 * pv.u
            radius: 4 * pv.u
            color: pv.card
            border.color: pv.a(pv.fg, 0.14)
            Rectangle { x: 5 * pv.u; anchors.verticalCenter: parent.verticalCenter; width: parent.width * 0.4; height: 3 * pv.u; radius: height / 2; color: pv.a(pv.mut, 0.45) }
            Rectangle { x: parent.width - width - 3.5 * pv.u; anchors.verticalCenter: parent.verticalCenter; width: 8 * pv.u; height: width; radius: width / 2; color: pv.acc }
        }
    }
}
