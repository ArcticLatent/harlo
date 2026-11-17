pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// Settings → Notifications: when harlo tells you, and how.
ColumnLayout {
    id: notify
    property bool compact
    spacing: 0
    property var cfg: JSON.parse(Workspace.notifySettingsJson())
    readonly property var info: JSON.parse(Workspace.notifyDefaultsJson())
    function save() { Workspace.setNotifySettings(JSON.stringify(cfg)) }
    function setTop(key, value) { const c = Object.assign({}, cfg); c[key] = value; cfg = c; save() }
    function setEvent(name, key, value) {
        const c = Object.assign({}, cfg)
        c.events = Object.assign({}, c.events)
        c.events[name] = Object.assign({}, c.events[name])
        c.events[name][key] = value
        cfg = c
        save()
    }
    readonly property var eventRows: [
        { name: "completion", title: "Agent completion", detail: "A run finishes.", icon: "checkbox-circle" },
        { name: "subagent", title: "Helper finished", detail: "A helper finishes.", icon: "node-tree" },
        { name: "error", title: "Agent errors", detail: "A reply fails, the agent stops unexpectedly, or a helper fails.", icon: "alert" },
        { name: "question", title: "Agent questions", detail: "The agent asks you something or wants a tool approved.", icon: "question" },
        { name: "limits", title: "Plan limits", detail: "A plan limit (Claude's, ChatGPT's) is used up, and when one you'd nearly or fully used resets, with a sound of its own.", icon: "speed", fixed: true },
    ]
    property string testNote: ""

    SectionHeader { title: "Notification delivery"; subtitle: "Choose when and how you get notified. Clicking a notification opens its session." }
    Group {
        SettingRow {
            title: "Enable notifications"
            icon: "notification-3"
            AppSwitch { objectName: "notifyEnabled"; checked: notify.cfg.enabled; onToggled: notify.setTop("enabled", checked) }
        }
        SettingRow {
            visible: notify.cfg.enabled
            divider: true
            title: "Notify while Harlo is focused"
            detail: "Off: only while Harlo is in the background. Never for the session you're looking at."
            icon: "window"
            AppSwitch { objectName: "notifyWhileFocused"; checked: notify.cfg.whileFocused; onToggled: notify.setTop("whileFocused", checked) }
        }
        SettingRow {
            visible: notify.cfg.enabled
            divider: true
            title: "Play a sound"
            detail: "Harlo's own, the same on any desktop."
            icon: "volume-up"
            AppSwitch { objectName: "notifySound"; checked: notify.cfg.sound; onToggled: notify.setTop("sound", checked) }
        }
        // (A row, as Devices' Test.)
        SettingRow {
            visible: notify.cfg.enabled
            divider: true
            title: "Test"
            detail: notify.testNote !== "" ? notify.testNote : "Shows one now, as the events below will."
            icon: "notification-3"
            FlatButton {
                objectName: "notifyTest"
                text: "Send a test notification"
                font.pixelSize: Theme.px(12)
                onClicked: { Workspace.sendTestNotification(); notify.testNote = "Sent. If nothing appeared, check that notify-send (libnotify) is installed." }
            }
        }
    }

    Rule {}
    SectionHeader {
        title: "Quiet hours"
        subtitle: "No notifications here, and silent ones on your iPhone."
        more: "At night, or whenever you want peace. The iPhone's make no sound and leave the screen dark; they wait in Notification Center."
    }
    Group {
        id: quietGroup
        readonly property var q: notify.cfg.quiet || ({ on: false, from: 1320, to: 480, asks: false })
        // Every half hour, 00:00 to 23:30.
        readonly property var times: Array.from({ length: 48 }, (_, i) => String(Math.floor(i / 2)).padStart(2, "0") + (i % 2 ? ":30" : ":00"))
        function setQuiet(key, value) { const q = Object.assign({}, quietGroup.q); q[key] = value; notify.setTop("quiet", q) }
        SettingRow {
            title: "Quiet hours"
            detail: quietGroup.q.on ? "From " + quietGroup.times[Math.floor(quietGroup.q.from / 30)] + " to " + quietGroup.times[Math.floor(quietGroup.q.to / 30)] + ", every day." : "Off: notifications come at any hour."
            icon: "moon"
            AppSwitch { objectName: "quietOn"; checked: quietGroup.q.on; onToggled: quietGroup.setQuiet("on", checked) }
        }
        SettingRow {
            visible: quietGroup.q.on
            divider: true
            title: "From"
            icon: "time"
            ComboField {
                objectName: "quietFrom"
                implicitWidth: 110
                model: quietGroup.times
                currentIndex: Math.floor(quietGroup.q.from / 30)
                onActivated: i => quietGroup.setQuiet("from", i * 30)
            }
        }
        SettingRow {
            visible: quietGroup.q.on
            divider: true
            title: "To"
            icon: "time"
            ComboField {
                objectName: "quietTo"
                implicitWidth: 110
                model: quietGroup.times
                currentIndex: Math.floor(quietGroup.q.to / 30)
                onActivated: i => quietGroup.setQuiet("to", i * 30)
            }
        }
        SettingRow {
            visible: quietGroup.q.on
            divider: true
            title: "Let questions through"
            detail: "An agent that asks you something or wants a tool approved still notifies, with sound, so it isn't stuck all night."
            icon: "question"
            AppSwitch { objectName: "quietAsks"; checked: quietGroup.q.asks; onToggled: quietGroup.setQuiet("asks", checked) }
        }
    }

    Rule { visible: notify.cfg.enabled }
    SectionHeader { visible: notify.cfg.enabled; title: "Notification events" }
    Group {
        visible: notify.cfg.enabled
        Repeater {
            model: notify.eventRows
            SettingRow {
                id: evRow
                required property var modelData
                required property int index
                divider: index > 0
                title: modelData.title
                detail: modelData.detail
                icon: modelData.icon
                AppSwitch {
                    objectName: "notifyEvent-" + evRow.modelData.name
                    checked: notify.cfg.events[evRow.modelData.name].on
                    onToggled: notify.setEvent(evRow.modelData.name, "on", checked)
                }
            }
        }
        // What a limit's reset plays (harlo plays it, whatever the
        // switch for sounds above says).
        SettingRow {
            id: resetSoundRow
            readonly property var sounds: notify.info.resetSounds || []
            readonly property string current: notify.cfg.resetSound !== undefined ? notify.cfg.resetSound : "desktop-login"
            visible: notify.cfg.events.limits && notify.cfg.events.limits.on
            divider: true
            title: "Sound when a limit resets"
            detail: "Plays even with the sound switch above off."
            icon: "volume-up"
            RowLayout {
                spacing: 8
                FlatButton {
                    objectName: "resetSoundPlay"
                    text: "Play"
                    enabled: resetSoundRow.current !== ""
                    font.pixelSize: Theme.px(12)
                    onClicked: Workspace.playSound(resetSoundRow.current)
                }
                ComboField {
                    objectName: "resetSound"
                    implicitWidth: 150
                    model: resetSoundRow.sounds.map(s => s.label)
                    currentIndex: Math.max(0, resetSoundRow.sounds.findIndex(s => s.id === resetSoundRow.current))
                    onActivated: i => { notify.setTop("resetSound", resetSoundRow.sounds[i].id); if (resetSoundRow.sounds[i].id !== "") Workspace.playSound(resetSoundRow.sounds[i].id) }
                }
            }
        }
    }

    Rule { visible: notify.cfg.enabled }
    SectionHeader {
        visible: notify.cfg.enabled
        title: "Notification templates"
        subtitle: "Leave a field empty for the default (shown faded). Variables:"
    }
    Flow {
        visible: notify.cfg.enabled
        Layout.fillWidth: true
        Layout.topMargin: 4
        spacing: 6
        Repeater {
            model: notify.info.variables
            Label {
                required property string modelData
                text: "{" + modelData + "}"
                color: Theme.info
                font.family: Theme.mono
                font.pixelSize: Theme.px(11.5)
            }
        }
    }
    GridLayout {
        visible: notify.cfg.enabled
        Layout.fillWidth: true
        Layout.topMargin: 10
        columns: notify.compact ? 1 : 2
        columnSpacing: 14
        rowSpacing: 14
        Repeater {
            model: notify.eventRows.filter(e => !e.fixed)
            Rectangle {
                id: eventCard
                required property var modelData
                readonly property var ev: notify.cfg.events[modelData.name]
                readonly property var def: notify.info.defaults[modelData.name]
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                implicitHeight: cardCol.implicitHeight + 24
                radius: 10
                color: Theme.elevated
                border.color: Theme.border
                ColumnLayout {
                    id: cardCol
                    x: 12
                    y: 12
                    width: parent.width - 24
                    spacing: 4
                    Label { text: eventCard.modelData.title.replace("Agent ", "").replace(/^./, c => c.toUpperCase()); color: Theme.foreground; font.bold: true; font.pixelSize: Theme.px(13) }
                    FormLabel { text: "Title"; Layout.topMargin: 4 }
                    AppTextField {
                        objectName: "notifyTitle-" + eventCard.modelData.name
                        Layout.fillWidth: true
                        text: eventCard.ev.title
                        placeholderText: eventCard.def.title
                        font.pixelSize: Theme.px(12)
                        onEditingFinished: if (text !== eventCard.ev.title) notify.setEvent(eventCard.modelData.name, "title", text)
                    }
                    FormLabel { Layout.topMargin: 8; text: "Message" }
                    AppTextField {
                        objectName: "notifyMessage-" + eventCard.modelData.name
                        Layout.fillWidth: true
                        text: eventCard.ev.message
                        placeholderText: eventCard.def.message
                        font.pixelSize: Theme.px(12)
                        onEditingFinished: if (text !== eventCard.ev.message) notify.setEvent(eventCard.modelData.name, "message", text)
                    }
                }
            }
        }
    }
}
