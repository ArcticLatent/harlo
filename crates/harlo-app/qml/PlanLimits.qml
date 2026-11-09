import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Provider plan limits, from Claude Code's sign-in and Harlo's ChatGPT plan:
// per account its plan and a bar per limit window, with when it resets.
// `host`: another machine's, from its own Claude Code and Harlo sessions.
ColumnLayout {
    id: root
    property string host: ""
    // "Plan limits", or "… on this PC" when other machines are listed too.
    property string title: host !== "" ? "Plan limits on " + host : "Plan limits"
    readonly property var report: {
        try {
            if (host === "") return JSON.parse(Workspace.quotaJson || "{}")
            return JSON.parse(Workspace.hostLimitsJson || "{}")[host] || {}
        } catch (e) { return {} }
    }
    readonly property var accounts: report.accounts || []
    spacing: 8

    function color(f, reached) { return reached || f >= 0.95 ? Theme.error : f >= 0.75 ? Theme.warning : Theme.primary }
    function resets(ms) {
        if (!ms) return ""
        const d = new Date(ms), now = new Date(), h = Math.round((d - now) / 3600000)
        return "resets " + (h < 1 ? "within the hour" : h < 24 ? "in " + h + " h" : d.toLocaleDateString(Qt.locale(), "ddd d MMM") + " " + d.toLocaleTimeString(Qt.locale(), "HH:mm"))
    }
    function updated(ms) {
        if (!ms) return ""
        const m = Math.round((Date.now() - ms) / 60000)
        return m < 1 ? "updated just now" : m < 60 ? "updated " + m + " min ago" : "updated " + Math.round(m / 60) + " h ago"
    }
    // Accounts per provider: show which account when there are several.
    function several(provider) { return accounts.filter(a => a.provider === provider).length > 1 }

    RowLayout {
        Layout.fillWidth: true
        spacing: 8
        // (A section of Settings → Usage: its heading, when it was read and
        // refresh at its right, as Activity has.)
        Label { text: root.title; color: Theme.text; font.pixelSize: Theme.px(16); font.weight: Font.DemiBold; font.letterSpacing: -0.1 }
        Label {
            Layout.fillWidth: true
            text: root.report.loading ? "checking…" : root.updated(root.report.updatedAt)
            color: Theme.textMuted
            font.pixelSize: Theme.px(12.5)
            elide: Text.ElideRight
        }
        RefreshButton {
            objectName: "limitsRefresh"
            tip: "Refresh the limits"
            enabled: !root.report.loading
            onClicked: root.host === "" ? Workspace.refreshQuota() : Workspace.refreshHostQuota(root.host)
        }
    }
    Label {
        Layout.fillWidth: true
        visible: !!root.report.error
        text: root.report.error || ""
        color: Theme.warning
        font.pixelSize: Theme.px(12)
        wrapMode: Text.Wrap
    }
    // (None yet: said inside a card, as Settings' empty sections.)
    Rectangle {
        Layout.fillWidth: true
        Layout.topMargin: 4
        visible: root.accounts.length === 0 && !root.report.error && !root.report.loading
        implicitHeight: noLimits.implicitHeight + 32
        radius: 12
        color: Theme.raised
        border.color: Theme.hairline
        Label {
            id: noLimits
            x: 16
            y: 16
            width: parent.width - 32
            text: root.host === ""
                ? "No limits to show. They come from Claude Code (signed in with a Claude plan) and from Harlo (signed in with a ChatGPT plan)."
                : "No limits from " + root.host + " yet. They come from its own Claude Code (signed in with a Claude plan) once it's connected, and from Harlo's sessions there."
            color: Theme.textMuted
            font.pixelSize: Theme.px(12.5)
            wrapMode: Text.Wrap
        }
    }
    Repeater {
        model: root.accounts
        Rectangle {
            id: card
            required property var modelData
            readonly property var acc: modelData
            // The label column: as wide as the longest label (within reason).
            readonly property real labelWidth: Math.min(150, Math.max(70, ...(card.acc.windows || []).map(w => labelMetrics.advanceWidth(w.label) + 4)))
            FontMetrics { id: labelMetrics; font.pixelSize: Theme.px(12) }
            Layout.fillWidth: true
            implicitHeight: accCol.implicitHeight + 20
            radius: 8
            color: Theme.elevated
            border.color: acc.limitReached ? Theme.error : Theme.border
            ColumnLayout {
                id: accCol
                x: 12
                y: 10
                width: parent.width - 24
                spacing: 6
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    // Claude Code's sign-in: its mark; Harlo's plan: the provider's logo.
                    ProviderMark { visible: card.acc.provider === "Claude"; provider: "claude"; tip: ""; color: Theme.foreground }
                    ProviderLogo { visible: card.acc.provider !== "Claude"; provider: card.acc.provider; implicitWidth: 14; implicitHeight: 14 }
                    Label { text: card.acc.provider; color: Theme.foreground; font.bold: true; font.pixelSize: Theme.px(13) }
                    Chip { visible: card.acc.plan !== ""; text: card.acc.plan }
                    Label {
                        Layout.fillWidth: true
                        visible: root.several(card.acc.provider)
                        text: card.acc.account
                        color: Theme.muted
                        font.pixelSize: Theme.px(11)
                        elide: Text.ElideRight
                    }
                    Item { Layout.fillWidth: true; visible: !root.several(card.acc.provider) }
                    Chip { visible: card.acc.limitReached; text: "limit reached"; fg: Theme.error }
                }
                Repeater {
                    model: card.acc.windows
                    RowLayout {
                        id: win_
                        required property var modelData
                        readonly property real f: Math.max(0, Math.min(1, modelData.usedFraction))
                        Layout.fillWidth: true
                        spacing: 10
                        Label { Layout.preferredWidth: card.labelWidth; text: win_.modelData.label; color: Theme.foreground; font.pixelSize: Theme.px(12); elide: Text.ElideRight }
                        Rectangle {
                            Layout.fillWidth: true
                            implicitHeight: 6
                            radius: 3
                            color: Theme.subtle
                            Rectangle { width: parent.width * win_.f; height: parent.height; radius: 3; color: root.color(win_.f, card.acc.limitReached && win_.f >= 0.99) }
                        }
                        Label {
                            text: Math.round(win_.f * 100) + "% used"
                            color: win_.f >= 0.75 ? root.color(win_.f, false) : Theme.foreground
                            font.pixelSize: Theme.px(12)
                        }
                        Label { text: root.resets(win_.modelData.resetsAt); color: Theme.muted; font.pixelSize: Theme.px(11) }
                    }
                }
            }
        }
    }
}
