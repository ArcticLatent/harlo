import QtQuick
import QtQuick.Controls
import QtQuick.Shapes
import harlo

// Vertical icon bar at the window's left edge, the right rail's mirror:
// the sidebar's toggle at the top; at the bottom the plan meter, the model
// servers harlo set up, the phone, keyboard shortcuts, usage stats, settings
// and your avatar.
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
    /// Settings → Model servers.
    signal serversRequested()
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
        // pinned from its menu), its fullest limit window, as a ring. Of the
        // machine shown: another's are what its Claude sessions said.
        Item {
            id: planMeter
            objectName: "planMeter"
            readonly property string host: Workspace.instance
            readonly property var accounts: {
                try {
                    if (host === "") return JSON.parse(Workspace.quotaJson || "{}").accounts || []
                    return (JSON.parse(Workspace.hostLimitsJson || "{}")[host] || {}).accounts || []
                } catch (e) { return [] }
            }
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
            // Used last, with no limits to show (a model on your own server,
            // or a provider harlo reads no limits for): its logo, no ring.
            readonly property string lastOther: Workspace.meterPin === "" && Workspace.meterLast !== "" && !providers.some(p => p.provider === Workspace.meterLast) ? Workspace.meterLast : ""
            // (This computer's servers: another machine's aren't known here.)
            readonly property bool lastLocal: lastOther !== "" && host === "" && Accounts.revision >= 0
                && ["pi", "omp", "harlo", ""].some(a => JSON.parse(Accounts.servers(a)).some(s => s.name === lastOther && s.place !== "online"))
                || ["ollama", "llama.cpp", "lm-studio"].includes(lastOther)
            // Pinned, else the last used, else the fullest.
            readonly property var shown: {
                if (lastOther !== "") return null
                const pick = id => id !== "" ? providers.find(p => p.provider === id) : undefined
                return pick(Workspace.meterPin) || pick(Workspace.meterLast) || providers.reduce((w, p) => !w || p.f > w.f ? p : w, null)
            }
            readonly property real f: shown ? Math.min(1, shown.f) : 0
            readonly property color tint: !shown ? Theme.accent : shown.reached || shown.f >= 0.95 ? Theme.error : shown.f >= 0.75 ? Theme.warning : Theme.accent
            function percent(p) { return Math.round(p.f * 100) + "%" }
            // Another machine with nothing used there yet: an empty ring,
            // so the meter doesn't look gone.
            readonly property bool none: host !== "" && !shown && lastOther === ""
            visible: (shown !== null && shown !== undefined) || lastOther !== "" || none
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
                        visible: planMeter.lastOther !== "" || (!!planMeter.shown && planMeter.shown.provider !== "Claude")
                        provider: planMeter.lastOther !== "" ? planMeter.lastOther : planMeter.shown ? planMeter.shown.provider : ""
                        color: Theme.textMuted
                        implicitWidth: 16
                        implicitHeight: 16
                    }
                }
                Label {
                    objectName: "planMeterPercent"
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: planMeter.lastOther !== "" ? (planMeter.lastLocal ? "local" : "—") : planMeter.shown ? planMeter.percent(planMeter.shown) : planMeter.none ? "—" : ""
                    color: Theme.textMuted
                    font.pixelSize: Theme.px(10)
                }
            }
            HoverHandler { id: meterHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: meterMenu.popup(planMeter, planMeter.width + 8, 0) }
            ToolTip.visible: meterHover.hovered && !meterMenu.visible
            readonly property string where: host !== "" ? " on " + host : ""
            ToolTip.text: planMeter.none ? "Nothing used on " + planMeter.host + " yet: its limits show once a session there works (Claude's say how much is used)"
                : planMeter.lastOther !== "" ? planMeter.lastOther + " (used last" + planMeter.where + "): " + (planMeter.lastLocal ? "runs on your own server, no usage limits" : "no usage limits harlo can read") + "\nClick to switch"
                : planMeter.shown ? planMeter.shown.provider + planMeter.where + ": " + planMeter.percent(planMeter.shown) + " of the " + planMeter.shown.label + " limit" + (planMeter.shown.reached ? " (limit reached)" : "")
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

        // The model servers harlo set up on this computer: Start and Stop
        // (a green dot while one runs).
        RailButton {
            id: serversButton
            objectName: "railServers"
            readonly property var list: {
                const rows = []
                if (Accounts.revision < 0) return rows
                for (const a of ["pi", "omp", "harlo", ""])
                    for (const sv of JSON.parse(Accounts.servers(a)))
                        if (sv.managed && !rows.some(r => r.baseUrl === sv.baseUrl)) rows.push(sv)
                return rows
            }
            readonly property var states: Accounts.serverStates !== "" ? JSON.parse(Accounts.serverStates) : ({})
            readonly property int running: list.filter(r => (states[r.baseUrl] || {}).state === "running").length
            function check() { if (list.length > 0) Accounts.checkServers(JSON.stringify(list.map(r => r.baseUrl))) }
            visible: list.length > 0
            iconName: "server"
            tip: running === 0 ? "Model servers: none running" : running === 1 ? "Model servers: 1 running" : "Model servers: " + running + " running"
            active: serversPanel.visible
            onClicked: serversPanel.visible ? serversPanel.close() : serversPanel.open()
            Component.onCompleted: Accounts.loadServers()
            Connections { target: Accounts; function onRevisionChanged() { serversButton.check() } }
            // Now and then; often while the panel is open.
            Timer { interval: serversPanel.visible ? 2000 : 15000; repeat: true; running: serversButton.visible; triggeredOnStart: true; onTriggered: serversButton.check() }
            Rectangle {
                objectName: "railServersRunning"
                visible: serversButton.running > 0
                x: serversButton.width - width - 7
                y: serversButton.height - height - 7
                width: 8
                height: 8
                radius: 4
                color: Theme.success
                border.width: 1.5
                border.color: Theme.chrome
            }
            Popup {
                id: serversPanel
                objectName: "railServersPanel"
                x: serversButton.width + 8
                y: serversButton.height - height
                width: 300
                padding: 6
                background: GlassSurface { radius: 10 }
                contentItem: Column {
                    spacing: 2
                    Label {
                        leftPadding: 8
                        topPadding: 4
                        bottomPadding: 4
                        text: "MODEL SERVERS"
                        color: Theme.label
                        font.pixelSize: Theme.px(11)
                        font.weight: Font.Medium
                        font.letterSpacing: Theme.px(11) * 0.07
                    }
                    Repeater {
                        model: serversButton.list
                        Item {
                            id: sRow
                            required property var modelData
                            readonly property var st: serversButton.states[modelData.baseUrl] || ({ state: "" })
                            objectName: "railServer-" + modelData.name
                            width: serversPanel.availableWidth
                            height: 44
                            Rectangle { anchors.fill: parent; radius: 6; color: rowHover.hovered ? Theme.rowHover : "transparent" }
                            HoverHandler { id: rowHover }
                            Rectangle {
                                id: dot
                                x: 10
                                anchors.verticalCenter: parent.verticalCenter
                                width: 8
                                height: 8
                                radius: 4
                                color: sRow.st.state === "running" ? Theme.success : sRow.st.state === "starting" ? Theme.warning : sRow.st.state === "failed" ? Theme.error : Theme.muted
                            }
                            Column {
                                anchors.left: dot.right
                                anchors.leftMargin: 10
                                anchors.right: rowButton.left
                                anchors.rightMargin: 8
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 1
                                // The engine (port when there are two), then its models.
                                Label {
                                    width: parent.width
                                    readonly property bool twin: serversButton.list.filter(r => r.engine === sRow.modelData.engine).length > 1
                                    text: !sRow.modelData.engine ? sRow.modelData.name
                                        : sRow.modelData.engine + (twin ? " · " + sRow.modelData.baseUrl.replace(/^https?:\/\/[^:\/]+:?/, "").replace(/\/.*$/, "") : "")
                                    color: Theme.text
                                    font.pixelSize: Theme.px(13)
                                    elide: Text.ElideRight
                                }
                                Label {
                                    width: parent.width
                                    readonly property var names: (sRow.modelData.models || []).map(Theme.modelLabel)
                                    text: (({ running: "Running", starting: sRow.st.message || "Starting…", failed: "Didn't start", stopped: "Stopped" })[sRow.st.state] || "…")
                                        + (names.length > 0 ? " · " + names[0] + (names.length > 1 ? " +" + (names.length - 1) : "") : "")
                                    color: sRow.st.state === "failed" ? Theme.error : Theme.textMuted
                                    font.pixelSize: Theme.px(11.5)
                                    elide: Text.ElideRight
                                }
                            }
                            FlatButton {
                                id: rowButton
                                objectName: "railServerToggle-" + sRow.modelData.name
                                anchors.right: parent.right
                                anchors.rightMargin: 6
                                anchors.verticalCenter: parent.verticalCenter
                                visible: sRow.st.state !== "" && sRow.st.state !== "starting"
                                text: sRow.st.state === "running" ? "Stop" : "Start"
                                accent: sRow.st.state === "running" ? Theme.foreground : Theme.primary
                                font.pixelSize: Theme.px(12)
                                onClicked: sRow.st.state === "running" ? Accounts.stopServer(sRow.modelData.baseUrl) : Accounts.startServer(sRow.modelData.baseUrl)
                            }
                        }
                    }
                    AppMenuSeparator { width: serversPanel.availableWidth }
                    AppMenuItem {
                        width: serversPanel.availableWidth
                        text: "Model servers…"
                        onTriggered: { serversPanel.close(); root.serversRequested() }
                    }
                }
            }
        }

        // With a phone paired: pauses or resumes its notifications (the
        // pairing stays); right-click pairs another. Else: pairs one.
        RailButton {
            id: phoneButton
            objectName: "pairDeviceButton"
            readonly property bool paired: Phone.revision >= 0 && Phone.hasDevices()
            readonly property bool paused: paired && Phone.paused
            iconName: "smartphone"
            tip: !paired ? "Pair a device: your sessions on your phone"
                 : (paused ? "Phone notifications paused. Click to resume." : "Phone notifications on. Click to pause.")
                   + "\nRight-click to pair a device."
            active: Phone.running && !paused
            onClicked: paired ? Phone.setNotificationsPaused(!Phone.paused) : root.pairDeviceRequested()
            TapHandler { acceptedButtons: Qt.RightButton; onTapped: root.pairDeviceRequested() }
            // Paused: a pause mark on its corner.
            Rectangle {
                objectName: "phonePausedBadge"
                visible: phoneButton.paused
                x: phoneButton.width - width - 6
                y: phoneButton.height - height - 6
                width: 12
                height: 12
                radius: 6
                color: Theme.warning
                Row {
                    anchors.centerIn: parent
                    spacing: 2
                    Repeater { model: 2; Rectangle { width: 2; height: 6; radius: 1; color: Theme.background } }
                }
            }
        }
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
