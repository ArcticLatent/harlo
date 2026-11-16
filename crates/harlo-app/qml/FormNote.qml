import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// A form's explanation: muted, wrapping to the form's width.
Label {
    color: Theme.muted
    font.pixelSize: Theme.px(12)
    wrapMode: Text.Wrap
    Layout.fillWidth: true
}
