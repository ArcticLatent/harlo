import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// A row of AppMenu. `danger` paints it red; `marked` shows a check;
// `hint` is right-aligned muted text (a shortcut, say); `provider` puts
// that agent's mark before the text, `logo` a model provider's logo.
MenuItem {
    id: item
    property bool danger: false
    property bool marked: false
    property string hint: ""
    property string provider: ""
    property string logo: ""
    readonly property color tint: danger ? Theme.error : Theme.foreground

    implicitWidth: Math.max(180, label.implicitWidth + hintLabel.implicitWidth + 56 + (item.provider !== "" || item.logo !== "" ? 22 : 0))
    implicitHeight: visible ? 30 : 0
    padding: 0
    leftPadding: 10
    rightPadding: 10
    font.pixelSize: Theme.px(13)

    indicator: null
    arrow: null
    contentItem: RowLayout {
        spacing: 8
        ProviderMark {
            objectName: "menuProvider"
            visible: item.provider !== ""
            provider: item.provider
            tip: ""
            color: item.tint
            opacity: item.enabled ? 1 : 0.4
        }
        ProviderLogo {
            visible: item.logo !== ""
            provider: item.logo
            color: item.tint
            implicitWidth: 14
            implicitHeight: 14
            opacity: item.enabled ? 1 : 0.4
        }
        Label {
            id: label
            Layout.fillWidth: true
            text: item.text
            font: item.font
            color: item.tint
            opacity: item.enabled ? 1 : 0.4
            elide: Text.ElideRight
        }
        Label {
            id: hintLabel
            visible: item.hint !== ""
            text: item.hint
            color: Theme.muted
            font.pixelSize: Theme.px(11)
            opacity: item.enabled ? 1 : 0.4
        }
        Label {
            visible: item.marked
            text: "✓"
            color: Theme.primary
            font.pixelSize: Theme.px(13)
        }
    }
    background: Rectangle {
        radius: 6
        color: !item.enabled || !item.highlighted ? "transparent"
            : item.danger ? Qt.rgba(Theme.error.r, Theme.error.g, Theme.error.b, 0.14) : Theme.selected
    }
}
