import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// An ```options block's choices as cards side by side (one above another
// in a narrow column): each with what it is, its pros and cons, and the
// one the agent would pick marked. While the agent waits for your answer,
// each has a Choose button that sends its name; the one you chose (by
// button or in your own words, its name) stays marked.
GridLayout {
    id: cards
    /// Visuals.options's choices.
    property var choices: []
    /// Choose buttons shown (the latest reply, the agent waiting).
    property bool choosing: false
    /// What you wrote next, if anything: a choice's name marks that card.
    property string answer: ""
    signal chosen(string name)
    function norm(t) { return String(t).replace(/[\s.!]+$/, "").replace(/^\s+/, "").toLowerCase() }
    readonly property int chosenIndex: {
        const a = norm(answer)
        if (a === "") return -1
        for (let i = 0; i < choices.length; i++)
            if (norm(choices[i].name) === a) return i
        return -1
    }
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
            required property int index
            readonly property bool picked: cards.chosenIndex === index
            implicitHeight: body.implicitHeight + 22 + (choose.visible ? choose.height + 12 : 0)
            radius: 6
            color: Theme.raised
            border.width: picked ? 2 : 1
            border.color: picked || (modelData.recommended && cards.chosenIndex < 0) ? Theme.primary : Theme.hairline
            // (The ones not chosen step back.)
            opacity: cards.chosenIndex >= 0 && !picked ? 0.55 : 1
            Behavior on opacity { NumberAnimation { duration: Theme.hoverMotion } }
            // At the card's foot, so the buttons line up across the row.
            FlatButton {
                id: choose
                objectName: "optionChoose"
                visible: cards.choosing && cards.chosenIndex < 0
                x: 12
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 11
                text: "Choose"
                variant: card.modelData.recommended ? "primary" : "outline"
                Accessible.name: "Choose " + card.modelData.name
                onClicked: cards.chosen(card.modelData.name)
                Keys.onReturnPressed: cards.chosen(card.modelData.name)
                Keys.onEnterPressed: cards.chosen(card.modelData.name)
            }
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
                        objectName: "optionChosen"
                        visible: card.picked
                        Layout.alignment: Qt.AlignTop
                        text: "Chosen"
                        color: Theme.primary
                        font.pixelSize: Theme.px(11.5)
                        font.weight: Font.Medium
                    }
                    Label {
                        objectName: "optionRecommended"
                        visible: card.modelData.recommended && cards.chosenIndex < 0
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
