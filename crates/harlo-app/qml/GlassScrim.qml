import QtQuick
import QtQuick.Effects
import harlo

// What a dialog lays over the window (its Overlay.modal):
// the window slightly blurred and dimmed toward the background colour. Only
// the lowest open scrim blurs; one over another dialog just dims, so the
// dialog underneath stays in view.
Rectangle {
    id: scrim
    readonly property bool lowest: Theme.scrims.length > 0 && Theme.scrims[0] === scrim
    readonly property bool gpu: GraphicsInfo.api !== GraphicsInfo.Software
    readonly property bool blurred: Theme.glass && gpu && visible && lowest
    color: Theme.glass && gpu ? Qt.rgba(Theme.background.r, Theme.background.g, Theme.background.b, Theme.isLight ? 0.6 : 0.64)
                              : "#66000000"

    onVisibleChanged: Theme.scrims = visible ? Theme.glassPush(Theme.scrims, scrim) : Theme.glassPop(Theme.scrims, scrim)
    // It may be made already shown.
    Component.onCompleted: if (visible) Theme.scrims = Theme.glassPush(Theme.scrims, scrim)
    Component.onDestruction: Theme.scrims = Theme.glassPop(Theme.scrims, scrim)

    // Under the dimming: the window, blurred (4 px).
    ShaderEffectSource {
        id: behind
        anchors.fill: parent
        visible: false
        sourceItem: scrim.blurred ? Theme.backdrop : null
        live: true
    }
    MultiEffect {
        z: -1
        anchors.fill: parent
        visible: scrim.blurred
        source: behind
        autoPaddingEnabled: false
        blurEnabled: true
        blurMax: 16
        blur: 0.25
    }
}
