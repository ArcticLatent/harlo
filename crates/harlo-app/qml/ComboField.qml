import QtQuick
import QtQuick.Controls
import harlo

// ComboBox in the app's colours (the Basic style draws it light).
ComboBox {
    id: box
    // Font pickers: each item (and the current one) drawn in its own font;
    // item 0 is "system default" in `defaultFont`.
    property bool fontPreview: false
    property string defaultFont: ""
    // Providers: each item's provider id, for its mark.
    property var marks: []
    function fontOf(i) { return !fontPreview ? box.font.family : i <= 0 ? defaultFont : model[i] }
    implicitHeight: 30
    font.pixelSize: Theme.px(12)

    contentItem: Row {
        leftPadding: 10
        rightPadding: 4
        spacing: 7
        ProviderMark {
            anchors.verticalCenter: parent.verticalCenter
            visible: (box.marks[box.currentIndex] || "") !== ""
            provider: box.marks[box.currentIndex] || ""
            tip: ""
            color: Theme.foreground
        }
    Label {
        width: parent.width - x - parent.rightPadding
        height: parent.height
        text: box.displayText
        color: Theme.foreground
        font.pixelSize: box.font.pixelSize
        font.family: box.fontOf(box.currentIndex)
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    }
    indicator: Label {
        x: box.width - width - 10
        anchors.verticalCenter: parent.verticalCenter
        text: "▾"
        color: Theme.muted
        font.pixelSize: Theme.px(11)
    }
    rightPadding: 24
    // The app's button surface; the accent border while it has the focus.
    background: ControlSurface {
        control: box
        radius: 7
        Rectangle { anchors.fill: parent; radius: 7; color: "transparent"; border.color: Theme.primary; visible: box.activeFocus }
    }
    delegate: ItemDelegate {
        id: row
        required property int index
        required property var modelData
        width: ListView.view.width
        highlighted: box.highlightedIndex === index
        contentItem: Row {
            spacing: 7
            ProviderMark {
                anchors.verticalCenter: parent.verticalCenter
                visible: (box.marks[row.index] || "") !== ""
                provider: box.marks[row.index] || ""
                tip: ""
                color: Theme.foreground
            }
        Label {
            width: parent.width - x
            text: row.modelData
            color: row.index === box.currentIndex ? Theme.primary : Theme.foreground
            font.pixelSize: box.font.pixelSize
            font.family: box.fontOf(row.index)
            elide: Text.ElideRight
        }
        }
        background: Rectangle { radius: 4; color: row.highlighted ? Theme.hover : "transparent" }
    }
    popup: Popup {
        y: box.height + 2
        width: box.width
        implicitHeight: Math.min(contentItem.implicitHeight + 8, 320)
        padding: 4
        contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: box.popup.visible ? box.delegateModel : null
            currentIndex: box.highlightedIndex
            ScrollBar.vertical: ScrollBar { }
        }
        background: GlassSurface { radius: 8 }
    }
}
