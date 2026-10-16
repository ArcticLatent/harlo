import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// Starts an omp goal: an objective omp keeps working toward across turns,
// with an optional token budget.
Popup {
    id: root
    property ChatSession session
    /// Text to start from (the composer's draft).
    property string draft: ""
    /// The draft became the goal; the composer clears it.
    signal draftUsed()
    readonly property bool hasGoal: session && session.goalJson !== "" && JSON.parse(session.goalJson).status !== "complete"

    width: 460
    padding: 14
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
    background: GlassSurface { radius: 10 }
    onAboutToShow: {
        objective.text = draft
        budget.text = ""
        Qt.callLater(() => objective.forceActiveFocus())
    }

    /// "200k", "1.5m", "50000" → tokens; 0 for none, -1 when unreadable.
    function parseBudget(t) {
        const m = t.trim().toLowerCase().replace(/[, _]/g, "").match(/^(\d+(?:\.\d+)?)([km]?)$/)
        if (t.trim() === "") return 0
        if (!m) return -1
        return Math.round(parseFloat(m[1]) * (m[2] === "k" ? 1e3 : m[2] === "m" ? 1e6 : 1))
    }
    readonly property int budgetValue: parseBudget(budget.text)
    function start() {
        if (objective.text.trim() === "" || budgetValue < 0 || hasGoal) return
        session.goalCommand("create", objective.text.trim(), budgetValue)
        if (objective.text === draft && draft !== "") draftUsed()
        close()
    }

    contentItem: ColumnLayout {
        spacing: 10
        RowLayout {
            spacing: 8
            Icon { name: "compass-3"; color: Theme.primary; implicitWidth: 15; implicitHeight: 15 }
            Label { text: "Set a goal"; color: Theme.foreground; font.pixelSize: Theme.px(13); font.bold: true }
        }
        Label {
            Layout.fillWidth: true
            text: "omp keeps working toward it, turn after turn, until it's done, you pause it, or the token budget runs out."
            color: Theme.muted
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
        }
        Label {
            Layout.fillWidth: true
            visible: root.hasGoal
            text: "This session already has a goal. Drop it (above the composer) to set another."
            color: Theme.warning
            font.pixelSize: Theme.px(12)
            wrapMode: Text.Wrap
        }
        ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(160, Math.max(70, objective.implicitHeight))
            TextArea {
                id: objective
                enabled: !root.hasGoal
                placeholderText: "What should be true when it's done? E.g. \"All tests pass and the parser handles nested lists\""
                wrapMode: TextEdit.Wrap
                color: Theme.foreground
                placeholderTextColor: Theme.muted
                selectionColor: Theme.selection
                font.pixelSize: Theme.px(13)
                background: Rectangle { radius: 6; color: Theme.background; border.color: objective.activeFocus ? Theme.primary : Theme.border }
                Keys.onReturnPressed: event => { if (event.modifiers & Qt.ShiftModifier) event.accepted = false; else root.start() }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Label { text: "Token budget"; color: Theme.foreground; font.pixelSize: Theme.px(12) }
            AppTextField {
                id: budget
                Layout.preferredWidth: 110
                enabled: !root.hasGoal
                placeholderText: "none"
                onAccepted: root.start()
            }
            Label {
                Layout.fillWidth: true
                text: root.budgetValue < 0 ? "Use a number like 200k or 1.5m"
                    : root.budgetValue > 0 ? root.budgetValue.toLocaleString(Qt.locale("en_US"), "f", 0) + " tokens" + (root.budgetValue < 10000 ? ": too small for one turn" : "")
                    : "optional, e.g. 200k"
                color: root.budgetValue < 0 ? Theme.error : root.budgetValue > 0 && root.budgetValue < 10000 ? Theme.warning : Theme.muted
                font.pixelSize: Theme.px(11)
                elide: Text.ElideRight
            }
            DialogButton {
                text: "Start goal"
                accent: true
                enabled: !root.hasGoal && objective.text.trim() !== "" && root.budgetValue >= 0
                onClicked: root.start()
            }
        }
    }
}
