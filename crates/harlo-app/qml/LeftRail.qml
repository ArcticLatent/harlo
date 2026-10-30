import QtQuick
import QtQuick.Controls
import QtQuick.Shapes
import harlo

// Vertical icon bar at the window's left edge, the right rail's mirror:
// the sidebar's toggle at the top; at the bottom the plan meter, the phone,
// keyboard shortcuts, usage stats, settings and your avatar.
Rectangle {
    id: root
    property bool sidebarOpen: true
    property bool settingsOpen: false
    signal sidebarToggled()
    signal settingsRequested()
    signal shortcutsRequested()
    signal pairDeviceRequested()
    /// The plan meter: open Settings → Usage.
    signal planRequested()
    implicitWidth: 52
    color: Theme.chrome
    Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Theme.hairline }

    RailButton {
        objectName: "sidebarToggle"
        anchors.top: parent.top
        anchors.topMargin: 10
        anchors.horizontalCenter: parent.horizontalCenter
        iconName: "layout-column"
        tip: (root.sidebarOpen ? "Hide" : "Show") + " the sidebar" + Keymap.hint("toggleSidebar")
        onClicked: root.sidebarToggled()
    }

    Column {
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 10
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: 6

        // Plan use at a glance: the provider you used last (or the one
        // pinned from its menu), its fullest limit window, as a ring.
        Item {
            id: planMeter
            objectName: "planMeter"
            readonly property var accounts: { try { return JSON.parse(Workspace.quotaJson || "{}").accounts || [] } catch (e) { return [] } }
            // Each provider with limits, at its fullest window (of any of
            // its accounts).
            readonly property var providers: {
                const out = []
                for (const a of accounts)
                    for (const x of a.windows) {
                        const p = out.find(o => o.provider === a.provider)
                        const v = { provider: a.provider, plan: a.plan, f: x.usedFraction, label: x.label, reached: a.limitReached }
                        if (!p) out.push(v)
                        else if (x.usedFraction > p.f) out[out.indexOf(p)] = v
                    }
                return out
            }
            // Pinned, else the last used, else the fullest.
            readonly property var shown: {
                const pick = id => id !== "" ? providers.find(p => p.provider === id) : undefined
                return pick(Workspace.meterPin) || pick(Workspace.meterLast) || providers.reduce((w, p) => !w || p.f > w.f ? p : w, null)
            }
            readonly property real f: shown ? Math.min(1, shown.f) : 0
            readonly property color tint: !shown ? Theme.accent : shown.reached || shown.f >= 0.95 ? Theme.error : shown.f >= 0.75 ? Theme.warning : Theme.accent
            function percent(p) { return Math.round(p.f * 100) + "%" }
            visible: shown !== null && shown !== undefined
            width: 38
            height: meterCol.implicitHeight + 10
            Rectangle { anchors.fill: parent; radius: 10; color: meterHover.hovered || meterMenu.visible ? Theme.rowHover : "transparent"; Behavior on color { ColorAnimation { duration: Theme.hoverMotion } } }
            Column {
                id: meterCol
                anchors.centerIn: parent
                spacing: 3
                Item {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 26
                    height: 26
                    // The ring: the track, then the part used.
                    Shape {
                        anchors.fill: parent
                        preferredRendererType: Shape.CurveRenderer
                        ShapePath {
                            strokeColor: Theme.controlFill
                            strokeWidth: 2.5
                            fillColor: "transparent"
                            PathAngleArc { centerX: 13; centerY: 13; radiusX: 11.5; radiusY: 11.5; startAngle: 0; sweepAngle: 360 }
                        }
                        ShapePath {
                            strokeColor: planMeter.tint
                            strokeWidth: 2.5
                            fillColor: "transparent"
                            capStyle: ShapePath.RoundCap
                            PathAngleArc { centerX: 13; centerY: 13; radiusX: 11.5; radiusY: 11.5; startAngle: -90; sweepAngle: 360 * planMeter.f }
                        }
                    }
                    // Whose limit it is: Claude's mark, else the provider's logo.
                    ProviderMark {
                        objectName: "planMeterClaude"
                        anchors.centerIn: parent
                        visible: !!planMeter.shown && planMeter.shown.provider === "Claude"
                        provider: "claude"
                        tip: ""
                        color: Theme.textMuted
                        implicitWidth: 12
                        implicitHeight: 12
                    }
                    // (The logos have a margin of their own, about an eighth
                    // on each side, which Claude's mark hasn't: bigger, so
                    // they look the same size.)
                    ProviderLogo {
                        objectName: "planMeterLogo"
                        anchors.centerIn: parent
                        visible: !!planMeter.shown && planMeter.shown.provider !== "Claude"
                        provider: planMeter.shown ? planMeter.shown.provider : ""
                        color: Theme.textMuted
                        implicitWidth: 16
                        implicitHeight: 16
                    }
                }
                Label {
                    objectName: "planMeterPercent"
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: planMeter.shown ? planMeter.percent(planMeter.shown) : ""
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(10)
                }
            }
            HoverHandler { id: meterHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: meterMenu.popup(planMeter, planMeter.width + 8, 0) }
            ToolTip.visible: meterHover.hovered && !meterMenu.visible
            ToolTip.text: planMeter.shown ? planMeter.shown.provider + ": " + planMeter.percent(planMeter.shown) + " of the " + planMeter.shown.label + " limit" + (planMeter.shown.reached ? " (limit reached)" : "")
                + (Workspace.meterPin === planMeter.shown.provider ? " (pinned)" : Workspace.meterLast === planMeter.shown.provider ? " (used last)" : "") + "\nClick to switch" : ""
            ToolTip.delay: 400
            // Whose limits to show.
            AppMenu {
                id: meterMenu
                AppMenuItem {
                    objectName: "meterFollow"
                    text: "Last used"
                    hint: Workspace.meterLast
                    marked: Workspace.meterPin === ""
                    onTriggered: Workspace.pinMeter("")
                }
                AppMenuSeparator { }
                Repeater {
                    model: planMeter.providers
                    AppMenuItem {
                        required property var modelData
                        objectName: "meterPick-" + modelData.provider
                        text: modelData.provider + (modelData.plan ? " · " + modelData.plan : "")
                        provider: modelData.provider === "Claude" ? "claude" : ""
                        logo: modelData.provider === "Claude" ? "" : modelData.provider
                        hint: planMeter.percent(modelData)
                        marked: Workspace.meterPin === modelData.provider
                        onTriggered: Workspace.pinMeter(modelData.provider)
                    }
                }
                AppMenuSeparator { }
                AppMenuItem { objectName: "meterAll"; text: "All limits…"; onTriggered: root.planRequested() }
            }
        }

        RailButton { objectName: "pairDeviceButton"; iconName: "smartphone"; tip: "Pair a device: your sessions on your phone"; active: Phone.running; onClicked: root.pairDeviceRequested() }
        RailButton { iconName: "keyboard"; tip: "Keyboard shortcuts" + Keymap.hint("shortcuts"); onClicked: root.shortcutsRequested() }
        RailButton { iconName: "stats"; tip: "Usage stats · " + Workspace.sessionCount + " sessions"; active: Workspace.activeKey === "view:stats"; onClicked: Workspace.open("view:stats") }
        RailButton { objectName: "settingsButton"; iconName: "settings"; tip: "Settings" + Keymap.hint("settings"); active: root.settingsOpen; onClicked: root.settingsRequested() }

        // You: your avatar; click to pick another.
        Item {
            id: avatarButton
            objectName: "avatarButton"
            anchors.horizontalCenter: parent.horizontalCenter
            width: 42
            height: 42
            Rectangle {
                anchors.fill: parent
                radius: width / 2
                color: "transparent"
                border.width: 2
                border.color: avatarPicker.visible ? Theme.accent : avatarHover.hovered ? Theme.hairline : "transparent"
            }
            UserAvatar { anchors.centerIn: parent; width: 34; height: 34 }
            HoverHandler { id: avatarHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: avatarPicker.visible ? avatarPicker.close() : avatarPicker.open() }
            ToolTip.visible: avatarHover.hovered && !avatarPicker.visible
            ToolTip.text: "Your avatar"
            ToolTip.delay: 400
            AvatarPicker {
                id: avatarPicker
                x: avatarButton.width + 8
                y: avatarButton.height - implicitHeight
            }
        }
    }

    component RailButton: Button {
        id: rb
        property string iconName
        property string tip
        property bool active: false
        anchors.horizontalCenter: parent ? parent.horizontalCenter : undefined
        flat: true
        implicitWidth: 38
        implicitHeight: 38
        padding: 0
        contentItem: Item {
            Icon {
                anchors.centerIn: parent
                name: rb.iconName
                implicitWidth: 19
                implicitHeight: 19
                color: rb.active ? Theme.accent : rb.hovered ? Theme.text : Theme.textMuted
            }
        }
        background: Rectangle { radius: 10; color: rb.active || rb.down ? Theme.rowSelected : rb.hovered ? Theme.rowHover : "transparent"; Behavior on color { ColorAnimation { duration: Theme.hoverMotion } } }
        ToolTip.visible: hovered
        ToolTip.text: tip
        ToolTip.delay: 400
    }
}
