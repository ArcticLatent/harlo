import QtQuick
import QtQuick.Controls.Basic
import harlo

// One choice among a few, as a pill: outlined, filled when selected; a
// provider's pill has its mark before the name.
Button {
    id: pill
    property bool selected: false
    property string provider: ""
    flat: true
    padding: 5
    leftPadding: 11
    rightPadding: 11
    font.pixelSize: Theme.px(12)
    contentItem: Row {
        spacing: 6
        ProviderMark { anchors.verticalCenter: parent.verticalCenter; visible: pill.provider !== ""; provider: pill.provider; tip: ""; implicitWidth: 12; implicitHeight: 12; color: pill.selected ? Theme.foreground : Theme.muted }
        Label { anchors.verticalCenter: parent.verticalCenter; text: pill.text; color: pill.selected ? Theme.foreground : Theme.muted; font: pill.font }
    }
    background: ControlSurface { control: pill; variant: pill.selected ? "selected" : "outline"; radius: 3 }
}
