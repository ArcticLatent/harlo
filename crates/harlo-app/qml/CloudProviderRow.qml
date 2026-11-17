pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import harlo

// One of an agent's cloud providers in Settings → Models: whether it has a
// key; opened, the key (saved, checked, removed), where to get one and
// see its prices, its models, and the API variants that share its key.
ColumnLayout {
    id: row
    required property var modelData
    required property int index
    /// "harlo" or "harlo@<host>".
    property string agent: "harlo"
    property bool expanded: false
    /// What came of the last change to its key: `{ state: "busy" | "ok" |
    /// "error", message }`, or null.
    property var result: null
    /// The providers that use this one's key over another API.
    property var variants: []
    signal toggled()
    signal signInRequested()

    readonly property string key: modelData.key || "none"
    readonly property bool replaced: modelData.replaced === true
    readonly property bool busy: result !== null && result.state === "busy"
    readonly property var models: modelData.models || []

    objectName: "cloudProvider-" + modelData.id
    Layout.fillWidth: true
    spacing: 0
    Rectangle { visible: row.index > 0; Layout.fillWidth: true; implicitHeight: 1; color: Theme.border }
    ItemDelegate {
        id: head
        Layout.fillWidth: true
        implicitHeight: 48
        hoverEnabled: true
        leftPadding: 14
        rightPadding: 14
        Accessible.name: row.modelData.name + ", " + status.text
        onClicked: row.toggled()
        background: Rectangle { color: head.hovered ? Theme.hover : "transparent"; radius: 10 }
        contentItem: RowLayout {
            spacing: 10
            Icon { name: row.expanded ? "arrow-down-s" : "arrow-right-s"; color: Theme.muted; implicitWidth: 14; implicitHeight: 14 }
            ProviderLogo { provider: row.modelData.maker || row.modelData.id; color: Theme.foreground; implicitWidth: 17; implicitHeight: 17 }
            Label { text: row.modelData.name; color: Theme.foreground; font.pixelSize: Theme.px(13.5); font.weight: Font.Medium }
            Rectangle {
                implicitWidth: 7
                implicitHeight: 7
                radius: 3.5
                color: row.replaced || row.key === "none" ? Theme.muted : Theme.success
            }
            Label {
                id: status
                objectName: "cloudStatus-" + row.modelData.id
                Layout.fillWidth: true
                elide: Text.ElideRight
                color: Theme.muted
                font.pixelSize: Theme.px(12.5)
                text: row.replaced ? qsTr("Set up in providers.json")
                    : row.key === "saved" ? qsTr("Key saved")
                    : row.key === "environment" ? qsTr("Key from $%1").arg(row.modelData.keyEnv)
                    : qsTr("No key")
            }
            Label {
                visible: row.models.length > 0
                text: row.models.length === 1 ? qsTr("1 model") : qsTr("%1 models").arg(row.models.length)
                color: Theme.muted
                font.pixelSize: Theme.px(12)
            }
        }
    }
    ColumnLayout {
        visible: row.expanded
        Layout.fillWidth: true
        Layout.leftMargin: 51
        Layout.rightMargin: 16
        Layout.bottomMargin: 14
        spacing: 8
        Label {
            visible: row.replaced
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.muted
            font.pixelSize: Theme.px(12)
            text: qsTr("Your providers.json sets up a provider called %1, so Harlo uses that one instead.").arg(row.modelData.id)
        }
        // The key: pasted and saved (then checked with the provider).
        RowLayout {
            visible: !row.replaced
            Layout.fillWidth: true
            spacing: 8
            AppTextField {
                id: keyField
                objectName: "cloudKey-" + row.modelData.id
                Layout.fillWidth: true
                echoMode: TextInput.Password
                enabled: !row.busy
                placeholderText: row.key === "saved" ? qsTr("Paste a new key to replace the saved one") : row.agent === "claude" ? qsTr("Paste your plan's key") : qsTr("Paste your API key")
                Accessible.name: qsTr("%1 API key").arg(row.modelData.name)
                font.pixelSize: Theme.px(12)
                onAccepted: saveButton.clicked()
            }
            FlatButton {
                id: saveButton
                objectName: "cloudSave-" + row.modelData.id
                text: qsTr("Save")
                accent: Theme.primary
                enabled: keyField.text.trim() !== "" && !row.busy
                font.pixelSize: Theme.px(12)
                onClicked: {
                    Accounts.saveKey(row.agent, row.modelData.id, keyField.text)
                    keyField.text = ""
                }
            }
        }
        Flow {
            visible: !row.replaced
            Layout.fillWidth: true
            spacing: 8
            FlatButton {
                objectName: "cloudTest-" + row.modelData.id
                visible: row.key !== "none"
                enabled: !row.busy
                text: qsTr("Test")
                font.pixelSize: Theme.px(12)
                onClicked: Accounts.testKey(row.agent, row.modelData.id)
            }
            FlatButton {
                objectName: "cloudRemove-" + row.modelData.id
                visible: row.key === "saved"
                enabled: !row.busy
                text: qsTr("Remove")
                font.pixelSize: Theme.px(12)
                onClicked: Accounts.removeKey(row.agent, row.modelData.id)
            }
            FlatButton {
                objectName: "cloudSignIn-" + row.modelData.id
                visible: row.modelData.id === "openrouter"
                enabled: !row.busy
                text: qsTr("Sign in with OpenRouter…")
                font.pixelSize: Theme.px(12)
                onClicked: row.signInRequested()
            }
            FlatButton {
                visible: (row.modelData.keyPage || "") !== ""
                text: qsTr("Get a key ↗")
                font.pixelSize: Theme.px(12)
                onClicked: Qt.openUrlExternally(row.modelData.keyPage)
            }
            FlatButton {
                visible: (row.modelData.planPage || "") !== ""
                text: qsTr("Plan ↗")
                font.pixelSize: Theme.px(12)
                onClicked: Qt.openUrlExternally(row.modelData.planPage)
            }
            FlatButton {
                visible: (row.modelData.pricePage || "") !== ""
                text: qsTr("Prices ↗")
                font.pixelSize: Theme.px(12)
                onClicked: Qt.openUrlExternally(row.modelData.pricePage)
            }
        }
        Label {
            objectName: "cloudResult-" + row.modelData.id
            visible: row.result !== null && (row.result.message || "") !== ""
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: row.result ? row.result.message : ""
            color: !row.result || row.result.state === "busy" ? Theme.muted : row.result.state === "ok" ? Theme.success : Theme.error
            font.pixelSize: Theme.px(12)
        }
        Label {
            visible: (row.modelData.note || "") !== ""
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: row.modelData.note || ""
            color: Theme.muted
            font.pixelSize: Theme.px(12)
        }
        Label {
            visible: row.key === "environment"
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("Read from $%1 in Harlo's environment. A key saved here is used instead.").arg(row.modelData.keyEnv)
            color: Theme.muted
            font.pixelSize: Theme.px(12)
        }
        Label {
            objectName: "cloudVariants-" + row.modelData.id
            visible: row.variants.length > 0
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("The same key works for %1: its models are in the model picker under that name.").arg(row.variants.map(v => v.name).join(", "))
            color: Theme.muted
            font.pixelSize: Theme.px(12)
        }
        // Its models (no prices: they change; its Prices page has them).
        Flow {
            visible: row.models.length > 0
            Layout.fillWidth: true
            Layout.topMargin: 2
            spacing: 5
            Repeater {
                model: row.models
                Rectangle {
                    id: chip
                    required property var modelData
                    implicitWidth: chipName.implicitWidth + 16
                    implicitHeight: 24
                    radius: 6
                    color: Theme.controlFill
                    border.color: Theme.controlBorder
                    Label {
                        id: chipName
                        anchors.centerIn: parent
                        text: chip.modelData.name || chip.modelData.id
                        color: Theme.foreground
                        font.pixelSize: Theme.px(11.5)
                    }
                    HoverHandler { id: chipHover }
                    ToolTip.visible: chipHover.hovered
                    ToolTip.delay: 500
                    ToolTip.text: chip.modelData.id
                        + (chip.modelData.contextWindow > 0 ? qsTr(" · %1k context").arg(Math.round(chip.modelData.contextWindow / 1000)) : "")
                }
            }
        }
        Label {
            visible: row.models.length === 0 && !row.replaced
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("Its models come from its own list: they show in the model picker once it has a key.")
            color: Theme.muted
            font.pixelSize: Theme.px(12)
        }
    }
}
