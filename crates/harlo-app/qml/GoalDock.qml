import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// omp goal mode: the objective, its status, tokens against the budget and
// time spent, with Pause/Resume and Drop. A completed goal stays until
// dismissed.
Rectangle {
    id: root
    property ChatSession session
    readonly property var goal: session && session.goalJson !== "" ? JSON.parse(session.goalJson) : null
    readonly property string status: goal ? goal.status : ""
    property string dismissed: ""
    property bool expanded: false

    visible: goal !== null && dismissed !== goal.id
    implicitHeight: col.implicitHeight + 16
    radius: 10
    color: Theme.elevated
    border.color: status === "budget-limited" ? Theme.warning : status === "complete" ? Theme.success : Theme.border

    function tokens(n) {
        return n >= 1e6 ? (n / 1e6).toFixed(n >= 1e7 ? 0 : 1) + "M" : n >= 1000 ? Math.round(n / 1000) + "k" : String(n)
    }
    function duration(s) {
        const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), sec = s % 60
        return h > 0 ? h + "h " + m + "m" : m > 0 ? m + "m " + String(sec).padStart(2, "0") + "s" : sec + "s"
    }
    readonly property var badge: ({
        "active": { text: "Working", color: Theme.primary },
        "paused": { text: "Paused", color: Theme.muted },
        "budget-limited": { text: "Budget reached", color: Theme.warning },
        "complete": { text: "Complete", color: Theme.success },
    })[status] || { text: status, color: Theme.muted }

    AppDialog {
        id: confirmDrop
        title: "Drop this goal?"
        standardButtons: Dialog.Discard | Dialog.Cancel
        Label {
            width: 380
            wrapMode: Text.Wrap
            color: Theme.muted
            text: "omp stops working toward it. The session and its changes stay."
        }
        onDiscarded: { root.session.goalCommand("drop", "", 0); close() }
    }

    ColumnLayout {
        id: col
        x: 12
        y: 8
        width: parent.width - 24
        spacing: 6

        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Icon { name: "compass-3"; color: root.badge.color; implicitWidth: 15; implicitHeight: 15 }
            Label { text: "Goal"; color: Theme.foreground; font.pixelSize: Theme.px(12); font.bold: true }
            Rectangle {
                implicitWidth: badgeLabel.implicitWidth + 12
                implicitHeight: 18
                radius: 9
                color: Qt.rgba(root.badge.color.r, root.badge.color.g, root.badge.color.b, 0.15)
                Label { id: badgeLabel; anchors.centerIn: parent; text: root.badge.text; color: root.badge.color; font.pixelSize: Theme.px(11); font.bold: true }
                SequentialAnimation on opacity {
                    running: root.status === "active" && root.session.status === "busy"
                    loops: Animation.Infinite
                    onRunningChanged: if (!running) parent.opacity = 1
                    NumberAnimation { to: 0.55; duration: 800 }
                    NumberAnimation { to: 1; duration: 800 }
                }
            }
            Label {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                text: root.goal ? root.goal.objective : ""
                color: Theme.foreground
                font.pixelSize: Theme.px(12)
                elide: Text.ElideRight
                visible: !root.expanded
                TapHandler { onTapped: root.expanded = true }
                HoverHandler { cursorShape: Qt.PointingHandCursor }
            }
            Item { Layout.fillWidth: true; visible: root.expanded }
            FlatButton {
                visible: root.status === "active" || root.status === "budget-limited"
                text: "Pause"
                font.pixelSize: Theme.px(11)
                padding: 4
                onClicked: root.session.goalCommand("pause", "", 0)
                ToolTip.visible: hovered
                ToolTip.text: "Stop after the current turn; Resume picks it up again"
            }
            FlatButton {
                visible: root.status === "paused"
                text: "Resume"
                accent: Theme.primary
                font.pixelSize: Theme.px(11)
                padding: 4
                onClicked: root.session.goalCommand("resume", "", 0)
            }
            FlatButton {
                visible: root.status !== "complete"
                text: "Drop"
                accent: Theme.error
                font.pixelSize: Theme.px(11)
                padding: 4
                onClicked: confirmDrop.open()
            }
            FlatButton {
                visible: root.status === "complete"
                text: "Dismiss"
                font.pixelSize: Theme.px(11)
                padding: 4
                onClicked: root.dismissed = root.goal.id
            }
        }
        // The whole objective, when opened.
        Label {
            Layout.fillWidth: true
            visible: root.expanded
            text: root.goal ? root.goal.objective : ""
            color: Theme.foreground
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            TapHandler { onTapped: root.expanded = false }
        }
        // Tokens (against the budget) and time.
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Rectangle {
                visible: root.goal !== null && root.goal.tokenBudget !== undefined
                Layout.preferredWidth: 120
                implicitHeight: 4
                radius: 2
                color: Theme.subtle
                Rectangle {
                    width: parent.width * Math.min(1, root.goal && root.goal.tokenBudget ? root.goal.tokensUsed / root.goal.tokenBudget : 0)
                    height: parent.height
                    radius: 2
                    color: root.status === "budget-limited" ? Theme.warning : Theme.primary
                    Behavior on width { NumberAnimation { duration: 300 } }
                }
            }
            Label {
                Layout.fillWidth: true
                text: root.goal === null ? "" : [
                    root.tokens(root.goal.tokensUsed) + (root.goal.tokenBudget !== undefined ? " / " + root.tokens(root.goal.tokenBudget) : "") + " tokens",
                    root.duration(root.goal.timeUsedSeconds),
                    root.status === "budget-limited" ? "drop it and set a new goal with a bigger budget" : root.status === "paused" ? "omp waits until you resume" : ""
                ].filter(x => x).join(" · ")
                color: Theme.muted
                font.pixelSize: Theme.px(11)
                elide: Text.ElideRight
            }
        }
        Label {
            Layout.fillWidth: true
            visible: root.session && root.session.goalError !== ""
            text: root.session ? root.session.goalError : ""
            color: Theme.error
            font.pixelSize: Theme.px(11)
            wrapMode: Text.Wrap
        }
    }
}
