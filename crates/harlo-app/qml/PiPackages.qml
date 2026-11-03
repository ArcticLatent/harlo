import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import harlo

// The extensions harlo offers for pi (Settings → Extensions): what each does
// and costs, turning it on or off, and updating it.
ColumnLayout {
    id: root
    // The machine whose pi it is ("" this one): the one shown.
    property string host: Workspace.instance
    // A file to open in harlo (another machine's rules).
    signal openFile(string path)
    readonly property var st: {
        const v = Accounts.revision >= 0 ? JSON.parse(Accounts.packages()) : null
        return v && (v.host || "") === host ? v : ({ state: "loading", list: [] })
    }
    readonly property var list: st.list || []
    readonly property var job: st.job || null
    readonly property bool busy: job !== null && job.state === "running"
    readonly property int updatable: list.filter(p => p.latest !== "" && p.newer).length
    // (Those that are offered to everyone.)
    readonly property int allTokens: list.filter(p => !p.optional).reduce((sum, p) => sum + p.tokens, 0)
    spacing: 10
    Component.onCompleted: Accounts.loadPackages(host)
    onHostChanged: Accounts.loadPackages(host)

    function thousands(n) {
        return String(n).replace(/\B(?=(\d{3})+$)/g, ",")
    }

    ColumnLayout {
        Layout.fillWidth: true
        Layout.bottomMargin: 6
        spacing: 8
        Repeater {
            model: [
                root.host !== "" ? "These are for pi on " + root.host + ": its own copy of each, on or off there." : "",
                "Bare pi is minimal: it doesn't ask before it runs anything, and it can't search the web, keep a task list or hand work to other agents.",
                "These extensions add that, adapted to harlo. They're installed but off; turning them on is up to you.",
                root.allTokens > 0 ? "The cost is context: bare pi sends about 1,500 tokens with every request, and all of these together add about " + root.thousands(root.allTokens) + " more before they're used. That matters most on small local models." : ""
            ]
            Label {
                required property string modelData
                Layout.fillWidth: true
                visible: modelData !== ""
                text: modelData
                color: Theme.textMuted
                font.pixelSize: Theme.px(12.5)
                wrapMode: Text.Wrap
            }
        }
    }

    Repeater {
        model: root.list
        RowLayout {
            id: pkg
            required property var modelData
            readonly property bool working: root.busy && (root.job.source === modelData.name || root.job.source === "all")
            Layout.fillWidth: true
            spacing: 8
            Icon { name: "code-box"; color: Theme.muted; implicitWidth: 13; implicitHeight: 13; Layout.alignment: Qt.AlignTop; Layout.topMargin: 2 }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 1
                Label {
                    Layout.fillWidth: true
                    text: pkg.modelData.title + "  " + (pkg.modelData.version !== "" ? pkg.modelData.version : "not installed")
                    color: Theme.foreground
                    font.pixelSize: Theme.px(12.5)
                    elide: Text.ElideRight
                }
                Label {
                    Layout.fillWidth: true
                    text: pkg.modelData.what
                    color: Theme.muted
                    font.pixelSize: Theme.px(11.5)
                    wrapMode: Text.Wrap
                }
                Label {
                    Layout.fillWidth: true
                    text: pkg.modelData.tokens > 0 ? "Adds about " + root.thousands(pkg.modelData.tokens) + " tokens to every request" : "Adds nothing to requests"
                    color: Theme.muted
                    font.pixelSize: Theme.px(11.5)
                    wrapMode: Text.Wrap
                }
                Label {
                    objectName: "piPackageNote-" + pkg.modelData.name
                    Layout.fillWidth: true
                    visible: pkg.modelData.note !== ""
                    text: pkg.modelData.note
                    textFormat: Text.StyledText
                    linkColor: Theme.primary
                    color: Theme.warning
                    font.pixelSize: Theme.px(11.5)
                    wrapMode: Text.Wrap
                    onLinkActivated: link => Qt.openUrlExternally(link)
                    HoverHandler { cursorShape: parent.hoveredLink !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor }
                }
            }
            Label {
                visible: pkg.working
                text: "Working…"
                color: Theme.muted
                font.pixelSize: Theme.px(12)
            }
            FlatButton {
                objectName: "piPackageUpdate-" + pkg.modelData.name
                visible: !pkg.working && pkg.modelData.newer
                enabled: !root.busy
                text: "Update to " + pkg.modelData.latest
                padding: 3
                font.pixelSize: Theme.px(11)
                onClicked: Accounts.updatePackage(root.host, pkg.modelData.name)
            }
            AppSwitch {
                objectName: "piPackageSwitch-" + pkg.modelData.name
                Layout.alignment: Qt.AlignTop
                enabled: !root.busy
                checked: pkg.modelData.enabled
                onToggled: Accounts.setPackageEnabled(root.host, pkg.modelData.name, checked)
            }
        }
    }
    Label {
        visible: root.busy && root.job.message !== ""
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        text: root.job ? root.job.message : ""
        color: Theme.muted
        font.pixelSize: Theme.px(12)
    }
    Label {
        visible: (root.st.error || "") !== "" || (root.job !== null && root.job.state === "failed")
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        text: root.job !== null && root.job.state === "failed" ? root.job.message : (root.st.error || "")
        color: Theme.error
        font.pixelSize: Theme.px(12)
    }
    Label {
        visible: root.job !== null && root.job.state === "done"
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        text: root.job ? root.job.message : ""
        color: Theme.success
        font.pixelSize: Theme.px(12)
    }
    RowLayout {
        Layout.fillWidth: true
        spacing: 8
        FlatButton {
            objectName: "piPackageUpdateAll"
            visible: root.updatable > 1
            enabled: !root.busy
            text: "Update all"
            accent: Theme.primary
            font.pixelSize: Theme.px(12)
            onClicked: Accounts.updatePackage(root.host, "all")
        }
        FlatButton {
            objectName: "piPackageRules"
            visible: root.list.some(p => p.name === "@gotgenes/pi-permission-system" && p.enabled)
            text: "Permission rules"
            font.pixelSize: Theme.px(12)
            onClicked: {
                const file = Accounts.packageRulesFile(root.host)
                if (file === "") return
                // (Another machine's: in harlo's editor, over SSH.)
                if (file.startsWith("ssh://")) root.openFile(file)
                else Qt.openUrlExternally("file://" + file)
            }
        }
        Item { Layout.fillWidth: true }
    }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        text: "New pi sessions pick changes up, and open ones as soon as they're idle. These also apply to pi in your terminal.\n\nPackages you installed yourself with pi keep working and aren't listed here."
        color: Theme.muted
        font.pixelSize: Theme.px(11.5)
    }
}
