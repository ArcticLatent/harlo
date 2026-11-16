pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// A paired phone tries every address harlo has (at home, over Tailscale);
// when this computer's firewall blocks one, say so in the corner and open
// it with one click (the system asks for the password).
ToastStack {
    id: root
    /// Re-read now and then: Tailscale may come up after harlo.
    property int tick: 0
    Timer { interval: 60000; running: Phone.running; repeat: true; onTriggered: root.tick++ }
    entries: Phone.running && Phone.firewallRevision >= 0 && Phone.revision >= 0 && root.tick >= 0 ? JSON.parse(Phone.blockedJson()) : []
    keyOf: b => b.address

    card: ToastCard {
        id: toast
        stack: root
        namePrefix: "phoneFirewall-"
        iconName: "alert"
        iconColor: Theme.warning
        border.color: Theme.warning
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: toast.entry.tailscale ? "Your phone can't reach harlo over Tailscale" : "Your phone can't reach harlo on this network"
            color: Theme.foreground
            font.pixelSize: Theme.px(13)
            font.weight: Font.DemiBold
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: "This computer's firewall (" + toast.entry.firewall + ") blocks it" + (toast.entry.tailscale ? ", so the harlo app works only at home." : ".")
                + " Allow opens port " + Phone.port + " to " + toast.entry.network + " only, not the internet."
            color: Theme.muted
            font.pixelSize: Theme.px(12)
        }
        // No way to ask for the password here: the command.
        RowLayout {
            visible: !toast.entry.canAsk
            Layout.fillWidth: true
            spacing: 6
            AppTextField { Layout.fillWidth: true; readOnly: true; text: toast.entry.command; font.family: Theme.mono; font.pixelSize: Theme.px(11) }
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
                visible: toast.entry.canAsk
                enabled: !Phone.firewallBusy
                text: Phone.firewallBusy ? "Waiting for your password…" : "Allow"
                accent: Theme.primary
                font.pixelSize: Theme.px(12)
                onClicked: Phone.allowFirewall(toast.entry.address)
            }
            FlatButton {
                text: "Not now"
                outlined: false
                font.pixelSize: Theme.px(12)
                onClicked: Phone.hideFirewallNote(toast.entry.address)
            }
        }
    }
}
