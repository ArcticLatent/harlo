import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Pair a device: turn phone access on (once), then show a QR code and link
// for one phone, reaching harlo on this network / Tailscale or, from
// anywhere, through a harlo relay (asked about once). The code works once, for a few minutes; the dialog says when a
// phone has paired.
AppDialog {
    id: dlg
    objectName: "pairDeviceDialog"
    title: "Pair a device"
    standardButtons: Dialog.Close
    width: Math.min(460, (Overlay.overlay ? Overlay.overlay.width : 500) - 40)

    /// This computer's addresses, then the ways in from anywhere: the relay,
    /// and Tailscale Funnel when Tailscale is installed.
    /// Read when the dialog opens (not when it has finished opening: the
    /// first code is made straight away).
    property var addresses: []
    function loadAddresses() {
        addresses = JSON.parse(Phone.addressesJson()).map(a => Object.assign({ kind: "direct" }, a))
            .concat([{ address: "relay", kind: "relay" }])
            .concat(Phone.funnelAvailable || Phone.funnelOn ? [{ address: "funnel", kind: "funnel" }] : [])
    }
    property int addressIndex: 0
    readonly property var address: addresses.length > 0 ? addresses[Math.min(addressIndex, addresses.length - 1)] : null
    readonly property string kind: address ? address.kind : ""
    readonly property bool viaRelay: kind === "relay"
    readonly property bool viaFunnel: kind === "funnel"
    readonly property bool relayReady: Phone.relayOn
    function indexOfKind(k) { return addresses.findIndex(a => a.kind === k) }
    /// Declined a way in from anywhere: back to this network, if there is one.
    function notNow() {
        if (indexOfKind("direct") < 0) { close(); return }
        addressIndex = 0
        refresh()
    }
    /// This computer's firewall and the chosen network (direct only):
    /// would it stop the phone?
    readonly property var fw: dlg.visible && kind === "direct" && Phone.running && Phone.port > 0 && Phone.firewallRevision >= 0 && address
        ? JSON.parse(Phone.firewallJson(address.address)) : ({ firewall: "" })
    readonly property bool fwBlocks: fw.firewall !== "" && fw.allowed !== true && !(fw.allowed === null && fw.hidden)
    /// Phones try every address (the app, away from home over Tailscale):
    /// the other kind of address (Tailscale or this network) the firewall
    /// blocks too.
    readonly property var otherAddress: {
        if (kind !== "direct" || !address) return null
        const tail = a => String(a.label).startsWith("Tailscale")
        return addresses.find(a => a.kind === "direct" && tail(a) !== tail(address)) || null
    }
    readonly property bool otherIsTailscale: otherAddress !== null && String(otherAddress.label).startsWith("Tailscale")
    readonly property var fwOther: dlg.visible && otherAddress && Phone.running && Phone.port > 0 && Phone.firewallRevision >= 0
        ? JSON.parse(Phone.firewallJson(otherAddress.address)) : ({ firewall: "" })
    readonly property bool fwOtherBlocks: fwOther.firewall !== "" && fwOther.allowed === false
    property var pairing: ({})
    /// Devices when the code was made (a new one means it paired).
    property int devicesBefore: 0
    property string pairedName: ""

    function refresh() {
        pairedName = ""
        const ready = viaRelay ? relayReady : viaFunnel ? Phone.funnelState === "online" : Phone.running
        if (!address || !Phone.enabled || !ready) { pairing = ({}); return }
        devicesBefore = JSON.parse(Phone.devicesJson()).length
        pairing = JSON.parse(Phone.pairingJson(address.address))
    }
    onAboutToShow: {
        loadAddresses()
        // Already reachable from anywhere: start there.
        addressIndex = relayReady ? indexOfKind("relay") : Phone.funnelOn && indexOfKind("funnel") >= 0 ? indexOfKind("funnel") : 0
        refresh()
    }
    Connections { target: Phone; function onRunningChanged() { if (dlg.visible) dlg.refresh() } }
    Connections { target: Phone; function onRelayOnChanged() { if (dlg.visible) dlg.refresh() } }
    Connections { target: Phone; function onFunnelStateChanged() { if (dlg.visible && dlg.viaFunnel) dlg.refresh() } }
    Connections {
        target: Phone
        function onRevisionChanged() {
            const list = JSON.parse(Phone.devicesJson())
            if (dlg.visible && list.length > dlg.devicesBefore) dlg.pairedName = list[list.length - 1].name
        }
    }

    // No taller than the window; the content scrolls when it doesn't fit.
    height: Math.min(implicitHeight, (Overlay.overlay ? Overlay.overlay.height : 600) - 24)

    Flickable {
        id: scroller
        objectName: "pairScroller"
        anchors.fill: parent
        implicitWidth: body.implicitWidth
        implicitHeight: body.implicitHeight
        contentHeight: body.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { }

    ColumnLayout {
        id: body
        objectName: "pairBody"
        width: scroller.width - (scroller.contentHeight > scroller.height ? 10 : 0)
        spacing: 10
        // For Copy. (In here: the dialog takes its height from its one
        // content item, so it can't have a second one beside it.)
        TextEdit { id: clip; visible: false; function copyText(t) { text = t; selectAll(); copy(); deselect(); text = "" } }

        // Phone access is off: ask first.
        ColumnLayout {
            visible: !Phone.enabled
            Layout.fillWidth: true
            spacing: 10
            Rectangle {
                implicitWidth: 44; implicitHeight: 44; radius: 22
                color: Theme.subtle
                Icon { anchors.centerIn: parent; name: "shield-check"; color: Theme.success; implicitWidth: 20; implicitHeight: 20 }
            }
            Label { text: "Turn on phone access?"; color: Theme.foreground; font.bold: true; font.pixelSize: Theme.px(15) }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: Theme.muted
                text: "harlo will accept connections from your phone on this computer's network and Tailscale (port " + Phone.port + "). Only phones you pair get in, and everything between them and harlo is end-to-end encrypted."
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 6
                spacing: 10
                DialogButton { Layout.fillWidth: true; text: "Not now"; onClicked: dlg.close() }
                DialogButton { objectName: "phoneTurnOn"; Layout.fillWidth: true; text: "Turn on"; accent: true; onClicked: Phone.setAccess(true) }
            }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.border; Layout.topMargin: 6 }
            RowLayout {
                spacing: 8
                Icon { name: "global"; color: Theme.muted; implicitWidth: 14; implicitHeight: 14 }
                Label { Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.muted; font.pixelSize: Theme.px(12); text: "Away from home? You can pick a harlo relay or Tailscale Funnel next." }
            }
        }

        // On: the QR code.
        ColumnLayout {
            visible: Phone.enabled
            Layout.fillWidth: true
            spacing: 10
            Label {
                visible: Phone.error !== ""
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: Theme.error
                text: Phone.error
            }
            Label {
                visible: dlg.kind === "direct" && !Phone.running && Phone.error === ""
                text: "Starting…"
                color: Theme.muted
            }
            RowLayout {
                visible: dlg.pairedName === ""
                Layout.fillWidth: true
                spacing: 8
                Label { text: "Reach harlo at"; color: Theme.muted; font.pixelSize: Theme.px(12) }
                ComboField {
                    objectName: "pairAddress"
                    Layout.fillWidth: true
                    model: dlg.addresses.map(a => a.kind === "relay" ? "Anywhere  ·  harlo relay" : a.kind === "funnel" ? "Anywhere  ·  Tailscale Funnel" : a.address + "  ·  " + a.label)
                    currentIndex: dlg.addressIndex
                    onActivated: i => { dlg.addressIndex = i; dlg.refresh() }
                }
            }
            Label {
                visible: dlg.kind === "direct" && Phone.running && dlg.indexOfKind("direct") < 0
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: Theme.warning
                text: "This computer has no network address a phone could reach. Use a harlo relay instead."
            }

            // The relay isn't on: ask.
            ColumnLayout {
                visible: dlg.viaRelay && !dlg.relayReady && dlg.pairedName === ""
                Layout.fillWidth: true
                Layout.topMargin: 4
                spacing: 10
                Rectangle {
                    implicitWidth: 44; implicitHeight: 44; radius: 22
                    color: Theme.subtle
                    Icon { anchors.centerIn: parent; name: "global"; color: Theme.primary; implicitWidth: 20; implicitHeight: 20 }
                }
                Label { text: "Use harlo's relay?"; color: Theme.foreground; font.bold: true; font.pixelSize: Theme.px(15) }
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    color: Theme.muted
                    text: "harlo and your phone meet at harlo's relay (" + Phone.relayAddress + "), so the phone reaches harlo from anywhere, with nothing to set up."
                          + " Everything stays end-to-end encrypted: the relay passes on messages it can't read, and sees only when they're sent, how big they are and the addresses they come from."
                }
                Label {
                    visible: Phone.relayError !== ""
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    color: Theme.error
                    font.pixelSize: Theme.px(12)
                    text: Phone.relayError
                }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.topMargin: 6
                    spacing: 10
                    DialogButton { Layout.fillWidth: true; text: "Not now"; onClicked: dlg.notNow() }
                    DialogButton { id: useRelay; objectName: "pairRelayOn"; Layout.fillWidth: true; text: "Use relay"; accent: true; onClicked: Phone.useRelay(true) }
                }
            }

            // Tailscale Funnel: ask first, then follow it starting up.
            ColumnLayout {
                visible: dlg.viaFunnel && dlg.pairedName === "" && Phone.funnelState !== "online"
                Layout.fillWidth: true
                Layout.topMargin: 4
                spacing: 10
                Rectangle {
                    implicitWidth: 44; implicitHeight: 44; radius: 22
                    color: Theme.subtle
                    Icon { anchors.centerIn: parent; name: "global"; color: Theme.primary; implicitWidth: 20; implicitHeight: 20 }
                }
                Label { text: "Use Tailscale Funnel?"; visible: !Phone.funnelOn; color: Theme.foreground; font.bold: true; font.pixelSize: Theme.px(15) }
                Label {
                    visible: !Phone.funnelOn
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    color: Theme.muted
                    text: "Tailscale gives this computer a public HTTPS address (port " + 8443 + ") that leads to harlo's phone access, so your phone reaches harlo from anywhere: no Tailscale app on the phone, nothing hosted. Only paired phones get in, and everything stays end-to-end encrypted. It's taken down when you turn it off or quit harlo."
                }
                RowLayout {
                    visible: !Phone.funnelOn
                    Layout.fillWidth: true
                    Layout.topMargin: 6
                    spacing: 10
                    DialogButton { Layout.fillWidth: true; text: "Not now"; onClicked: dlg.notNow() }
                    DialogButton { objectName: "pairFunnelOn"; Layout.fillWidth: true; text: "Use Funnel"; accent: true; onClicked: Phone.setFunnel(true) }
                }
                Label {
                    objectName: "pairFunnelState"
                    visible: Phone.funnelOn
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    color: Phone.funnelState === "failed" || Phone.funnelError !== "" ? Theme.error : Theme.muted
                    text: Phone.funnelError !== "" ? Phone.funnelError
                        : Phone.funnelState === "approval" ? "Tailscale needs you to allow Funnel for your tailnet (once). harlo carries on by itself when it's allowed."
                        : !Phone.running ? "Waiting for phone access to start…"
                        : "Starting Tailscale Funnel…"
                }
                FlatButton {
                    visible: Phone.funnelOn && Phone.funnelState === "approval"
                    text: "Allow Funnel in Tailscale…"
                    accent: Theme.primary
                    font.pixelSize: Theme.px(12)
                    onClicked: Qt.openUrlExternally(Phone.funnelUrl)
                }
            }

            ColumnLayout {
                visible: dlg.pairing.url !== undefined && dlg.pairedName === ""
                Layout.fillWidth: true
                spacing: 10
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    color: Theme.muted
                    text: dlg.viaRelay ? "Scan this with your phone's camera, or open the link on it. The phone reaches harlo through the relay, from anywhere."
                        : dlg.viaFunnel ? "Scan this with your phone's camera, or open the link on it. The phone reaches harlo through Tailscale Funnel, from anywhere."
                        : "Scan this with your phone's camera, or open the link on it. The phone must be on the same network or Tailscale."
                }
                // The firewall would stop the phone: say so, and open it.
                Rectangle {
                    objectName: "firewallNote"
                    visible: dlg.fwBlocks
                    Layout.fillWidth: true
                    radius: 8
                    color: Theme.subtle
                    border.color: Theme.warning
                    implicitHeight: fwCol.implicitHeight + 20
                    ColumnLayout {
                        id: fwCol
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 10
                        spacing: 8
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            Icon { Layout.alignment: Qt.AlignTop; name: "alert"; color: Theme.warning; implicitWidth: 16; implicitHeight: 16 }
                            Label {
                                Layout.fillWidth: true
                                wrapMode: Text.Wrap
                                color: Theme.foreground
                                text: dlg.fw.allowed === false
                                    ? "This computer's firewall (" + dlg.fw.firewall + ") blocks phones, so the link won't open on yours yet."
                                    : "This computer's firewall (" + dlg.fw.firewall + ") may block phones. If the link doesn't open on yours, let harlo through."
                            }
                        }
                        RowLayout {
                            visible: dlg.fw.canAsk === true || dlg.fw.allowed === null
                            spacing: 8
                            FlatButton {
                                objectName: "allowFirewall"
                                visible: dlg.fw.canAsk === true
                                enabled: !Phone.firewallBusy
                                text: Phone.firewallBusy ? "Waiting for your password…" : "Allow phones on this network"
                                accent: Theme.primary
                                font.pixelSize: Theme.px(12)
                                onClicked: Phone.allowFirewall(dlg.address.address)
                            }
                            FlatButton {
                                visible: dlg.fw.allowed === null
                                text: "It connects already"
                                outlined: false
                                font.pixelSize: Theme.px(12)
                                onClicked: Phone.hideFirewallNote(dlg.address.address)
                            }
                        }
                        // No way to ask for the password: the command to type.
                        RowLayout {
                            visible: dlg.fw.canAsk !== true
                            Layout.fillWidth: true
                            spacing: 6
                            AppTextField { Layout.fillWidth: true; readOnly: true; text: dlg.fw.command || ""; font.family: Theme.mono; font.pixelSize: Theme.px(11) }
                            FlatButton { text: "Copy"; font.pixelSize: Theme.px(12); onClicked: clip.copyText(dlg.fw.command) }
                        }
                        Label {
                            objectName: "firewallError"
                            visible: Phone.firewallError !== ""
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            color: Theme.error
                            font.pixelSize: Theme.px(12)
                            text: Phone.firewallError
                        }
                        Label {
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            color: Theme.muted
                            font.pixelSize: Theme.px(12)
                            text: "Opens port " + Phone.port + " to " + dlg.fw.network + " (your network) only, not the internet."
                        }
                    }
                }
                // The other address is blocked too (phones try them all).
                Rectangle {
                    objectName: "firewallOtherNote"
                    visible: dlg.fwOtherBlocks
                    Layout.fillWidth: true
                    radius: 8
                    color: Theme.subtle
                    border.color: Theme.warning
                    implicitHeight: fwOtherCol.implicitHeight + 20
                    ColumnLayout {
                        id: fwOtherCol
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 10
                        spacing: 8
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            Icon { Layout.alignment: Qt.AlignTop; name: "alert"; color: Theme.warning; implicitWidth: 16; implicitHeight: 16 }
                            Label {
                                Layout.fillWidth: true
                                wrapMode: Text.Wrap
                                color: Theme.foreground
                                text: dlg.otherIsTailscale
                                    ? "The harlo app also tries Tailscale (away from home), but the firewall (" + dlg.fwOther.firewall + ") blocks phones there."
                                    : "The harlo app also tries this computer's local address (at home), but the firewall (" + dlg.fwOther.firewall + ") blocks phones there."
                            }
                        }
                        FlatButton {
                            objectName: "allowFirewallOther"
                            visible: dlg.fwOther.canAsk === true
                            enabled: !Phone.firewallBusy
                            text: Phone.firewallBusy ? "Waiting for your password…" : dlg.otherIsTailscale ? "Allow phones over Tailscale" : "Allow phones on this network"
                            accent: Theme.primary
                            font.pixelSize: Theme.px(12)
                            onClicked: Phone.allowFirewall(dlg.otherAddress.address)
                        }
                        RowLayout {
                            visible: dlg.fwOther.canAsk !== true
                            Layout.fillWidth: true
                            spacing: 6
                            AppTextField { Layout.fillWidth: true; readOnly: true; text: dlg.fwOther.command || ""; font.family: Theme.mono; font.pixelSize: Theme.px(11) }
                            FlatButton { text: "Copy"; font.pixelSize: Theme.px(12); onClicked: clip.copyText(dlg.fwOther.command) }
                        }
                        Label {
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            color: Theme.muted
                            font.pixelSize: Theme.px(12)
                            text: "Opens port " + Phone.port + " to " + (dlg.fwOther.network || "") + " only, not the internet."
                        }
                    }
                }
                Label {
                    objectName: "pairRelayState"
                    visible: dlg.viaRelay && Phone.relayState !== "online"
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    color: Phone.relayState === "failed" ? Theme.error : Theme.muted
                    font.pixelSize: Theme.px(12)
                    text: Phone.relayState === "failed" ? Phone.relayError + " The phone will connect once harlo reaches the relay."
                                                         : "Connecting to the relay…"
                }
                Rectangle {
                    Layout.alignment: Qt.AlignHCenter
                    implicitWidth: 236
                    implicitHeight: 236
                    radius: 10
                    color: "#ffffff"
                    Image {
                        objectName: "pairQr"
                        anchors.fill: parent
                        anchors.margins: 6
                        source: dlg.pairing.qr || ""
                        sourceSize: Qt.size(448, 448)
                        smooth: false
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 6
                    AppTextField {
                        objectName: "pairLink"
                        Layout.fillWidth: true
                        readOnly: true
                        text: dlg.pairing.url || ""
                        font.family: Theme.mono
                        font.pixelSize: Theme.px(11)
                    }
                    FlatButton {
                        id: copyButton
                        property bool done: false
                        text: done ? "Copied" : "Copy"
                        font.pixelSize: Theme.px(12)
                        onClicked: { clip.copyText(dlg.pairing.url); done = true; copyTimer.restart() }
                        Timer { id: copyTimer; interval: 1500; onTriggered: copyButton.done = false }
                    }
                }
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    color: Theme.muted
                    font.pixelSize: Theme.px(12)
                    text: "Works once, for " + (dlg.pairing.minutes || 10) + " minutes. Until then treat the link like a password. harlo's key: " + (dlg.pairing.fingerprint || "")
                }
                FlatButton { text: "New code"; font.pixelSize: Theme.px(12); outlined: false; onClicked: dlg.refresh() }
            }
            // Paired.
            ColumnLayout {
                visible: dlg.pairedName !== ""
                Layout.fillWidth: true
                spacing: 8
                Rectangle {
                    implicitWidth: 44; implicitHeight: 44; radius: 22
                    color: Theme.subtle
                    Icon { anchors.centerIn: parent; name: "check"; color: Theme.success; implicitWidth: 22; implicitHeight: 22 }
                }
                Label { objectName: "pairedLabel"; text: dlg.pairedName + " is paired"; color: Theme.foreground; font.bold: true; font.pixelSize: Theme.px(15) }
                Label { Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.muted; text: "It can open your sessions, send messages and answer questions. Remove it any time in Settings → Devices." }
                FlatButton { text: "Pair another"; font.pixelSize: Theme.px(12); onClicked: dlg.refresh() }
            }
        }
    }
    }
}
