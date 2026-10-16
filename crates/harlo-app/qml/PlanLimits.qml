import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Provider plan limits, from Claude Code's sign-in and from omp (`omp
// usage`): per account its plan and a bar per limit window, with when it
// resets.
ColumnLayout {
    id: root
    readonly property var report: { try { return JSON.parse(Workspace.quotaJson || "{}") } catch (e) { return {} } }
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
        Label { text: "Plan limits"; color: Theme.foreground; font.bold: true; font.pixelSize: Theme.px(14) }
        Label {
            Layout.fillWidth: true
            text: root.report.loading ? "checking…" : root.updated(root.report.updatedAt)
            color: Theme.muted
            font.pixelSize: Theme.px(12)
            elide: Text.ElideRight
        }
        FlatButton { text: "Refresh"; font.pixelSize: Theme.px(12); enabled: !root.report.loading; onClicked: Workspace.refreshQuota() }
    }
    Label {
        Layout.fillWidth: true
        visible: !!root.report.error
        text: root.report.error || ""
        color: Theme.warning
        font.pixelSize: Theme.px(12)
        wrapMode: Text.Wrap
    }
    Label {
        Layout.fillWidth: true
        visible: root.accounts.length === 0 && !root.report.error && !root.report.loading
        text: "No limits to show. They come from Claude Code (signed in with a Claude plan) and from omp, for the providers it's signed in to (omp usage)."
        color: Theme.muted
        font.pixelSize: Theme.px(12)
        wrapMode: Text.Wrap
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
                    // Claude Code's sign-in: its mark; omp's accounts: the model provider's logo.
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
