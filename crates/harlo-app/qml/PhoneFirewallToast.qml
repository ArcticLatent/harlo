import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// A paired phone tries every address harlo has (at home, over Tailscale);
// when this computer's firewall blocks one, say so in the corner and open
// it with one click (the system asks for the password).
Column {
    id: root
    spacing: 8
    width: 380
    /// Re-read now and then: Tailscale may come up after harlo.
    property int tick: 0
    Timer { interval: 60000; running: Phone.running; repeat: true; onTriggered: root.tick++ }
    // A notice slides in from the side, and fades out as it goes.
    add: Transition {
        NumberAnimation { property: "x"; from: 48; duration: Theme.paneMotion; easing.type: Easing.OutCubic }
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.paneMotion }
    }
    move: Transition {
        NumberAnimation { property: "y"; duration: Theme.paneMotion; easing.type: Easing.OutCubic }
    }
    readonly property var blocked: Phone.running && Phone.firewallRevision >= 0 && Phone.revision >= 0 && root.tick >= 0 ? JSON.parse(Phone.blockedJson()) : []

    // The notices shown, by address: kept as they change, so one that's
    // gone (opened, or Not now) fades out first.
    ListModel { id: shown }
    function sync() {
        const keys = root.blocked.map(b => b.address)
        for (let i = 0; i < shown.count; i++)
            if (keys.indexOf(shown.get(i).key) < 0) shown.setProperty(i, "leaving", true)
        for (const b of root.blocked) {
            let i = 0
            while (i < shown.count && shown.get(i).key !== b.address) i++
            if (i < shown.count) shown.set(i, { json: JSON.stringify(b), leaving: false })
            else shown.append({ key: b.address, json: JSON.stringify(b), leaving: false })
        }
    }
    function remove(key) {
        for (let i = 0; i < shown.count; i++)
            if (shown.get(i).key === key && shown.get(i).leaving) { shown.remove(i); return }
    }
    onBlockedChanged: sync()
    Component.onCompleted: sync()

    Repeater {
        model: shown
        Rectangle {
            id: toast
            required property string json
            required property string key
            required property bool leaving
            readonly property var b: JSON.parse(json)
            objectName: leaving ? "" : "phoneFirewall-" + b.address
            opacity: leaving ? 0 : 1
            Behavior on opacity {
                enabled: Theme.paneMotion > 0
                NumberAnimation { duration: Theme.paneMotion; easing.type: Easing.InCubic; onRunningChanged: if (!running && toast.leaving) root.remove(toast.key) }
            }
            onLeavingChanged: if (leaving && Theme.paneMotion === 0) Qt.callLater(root.remove, key)
            transform: Translate { x: toast.leaving ? 32 : 0; Behavior on x { NumberAnimation { duration: Theme.paneMotion; easing.type: Easing.InCubic } } }
            width: root.width
            implicitHeight: body.implicitHeight + 24
            radius: 10
            color: Theme.elevated
            border.color: Theme.warning
            Rectangle { z: -1; anchors.fill: parent; anchors.topMargin: 3; anchors.leftMargin: -1; anchors.rightMargin: -1; anchors.bottomMargin: -4; radius: 12; color: "#000000"; opacity: 0.18 }

            RowLayout {
                id: body
                x: 14
                y: 12
                width: parent.width - 28
                spacing: 12
                Icon { Layout.alignment: Qt.AlignTop; Layout.topMargin: 2; name: "alert"; color: Theme.warning; implicitWidth: 18; implicitHeight: 18 }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 6
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: toast.b.tailscale ? "Your phone can't reach harlo over Tailscale" : "Your phone can't reach harlo on this network"
                        color: Theme.foreground
                        font.pixelSize: Theme.px(13)
                        font.weight: Font.DemiBold
                    }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: "This computer's firewall (" + toast.b.firewall + ") blocks it" + (toast.b.tailscale ? ", so the harlo app works only at home." : ".")
                            + " Allow opens port " + Phone.port + " to " + toast.b.network + " only, not the internet."
                        color: Theme.muted
                        font.pixelSize: Theme.px(12)
                    }
                    // No way to ask for the password here: the command.
                    RowLayout {
                        visible: !toast.b.canAsk
                        Layout.fillWidth: true
                        spacing: 6
                        AppTextField { Layout.fillWidth: true; readOnly: true; text: toast.b.command; font.family: Theme.mono; font.pixelSize: Theme.px(11) }
                    }
                    Label {
                        visible: Phone.firewallError !== ""
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        color: Theme.error
                        font.pixelSize: Theme.px(12)
                        text: Phone.firewallError
                    }
                    RowLayout {
                        spacing: 8
                        FlatButton {
                            objectName: "phoneFirewallAllow"
                            visible: toast.b.canAsk
                            enabled: !Phone.firewallBusy
                            text: Phone.firewallBusy ? "Waiting for your password…" : "Allow"
                            accent: Theme.primary
                            font.pixelSize: Theme.px(12)
                            onClicked: Phone.allowFirewall(toast.b.address)
                        }
                        FlatButton {
                            text: "Not now"
                            outlined: false
                            font.pixelSize: Theme.px(12)
                            onClicked: Phone.hideFirewallNote(toast.b.address)
                        }
                    }
                }
            }
        }
    }
}
