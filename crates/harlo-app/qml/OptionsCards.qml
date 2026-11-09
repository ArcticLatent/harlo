import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// An ```options block's choices as cards side by side (one above another
// in a narrow column): each with what it is, its pros and cons, and the
// one the agent would pick marked.
GridLayout {
    id: cards
    /// Visuals.options's choices.
    property var choices: []
    columns: width >= 620 ? Math.min(choices.length, 3) : 1
    columnSpacing: 8
    rowSpacing: 8

    Repeater {
        model: cards.choices
        Rectangle {
            id: card
            objectName: "optionCard"
            required property var modelData
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            Layout.alignment: Qt.AlignTop
            implicitHeight: body.implicitHeight + 22
            radius: 6
            color: Theme.raised
            border.width: 1
            border.color: modelData.recommended ? Theme.primary : Theme.hairline
            ColumnLayout {
                id: body
                x: 12
                y: 11
                width: parent.width - 24
                spacing: 5
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Label {
                        Layout.fillWidth: true
                        text: card.modelData.name
                        color: Theme.text
                        font.pixelSize: Theme.px(14)
                        font.weight: Font.Medium
                        wrapMode: Text.Wrap
                    }
                    Label {
                        objectName: "optionRecommended"
                        visible: card.modelData.recommended
                        Layout.alignment: Qt.AlignTop
                        text: "Recommended"
                        color: Theme.primary
                        font.pixelSize: Theme.px(11.5)
                        font.weight: Font.Medium
                    }
                }
                Label {
                    Layout.fillWidth: true
                    visible: text !== ""
                    text: card.modelData.about
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(13)
                    wrapMode: Text.Wrap
                }
                Repeater {
                    model: card.modelData.pros
                    RowLayout {
                        required property string modelData
                        Layout.fillWidth: true
                        spacing: 7
                        Label { Layout.alignment: Qt.AlignTop; text: "+"; color: Theme.success; font.pixelSize: Theme.px(13); font.weight: Font.Medium }
                        Label { Layout.fillWidth: true; text: parent.modelData; color: Theme.text; font.pixelSize: Theme.px(13); wrapMode: Text.Wrap }
                    }
                }
                Repeater {
                    model: card.modelData.cons
                    RowLayout {
                        required property string modelData
                        Layout.fillWidth: true
                        spacing: 7
                        Label { Layout.alignment: Qt.AlignTop; text: "−"; color: Theme.error; font.pixelSize: Theme.px(13); font.weight: Font.Medium }
                        Label { Layout.fillWidth: true; text: parent.modelData; color: Theme.text; font.pixelSize: Theme.px(13); wrapMode: Text.Wrap }
                    }
                }
            }
        }
    }
}
