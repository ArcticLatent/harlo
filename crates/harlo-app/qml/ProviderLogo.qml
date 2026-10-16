import QtQuick
import harlo

// A provider's logo (icons/providers) in the text
// colour. Providers without one: a server icon for local engines, else
// the provider's initial.
Item {
    id: root
    /// "openai-codex", "anthropic/claude-…", "llama.cpp", …
    property string provider: ""
    property color color: Theme.foreground
    implicitWidth: 17
    implicitHeight: 17

    readonly property string url: provider !== "" ? Workspace.providerLogo(provider, Theme.rgb(color)) : ""
    readonly property bool local: /llama|local|vllm|lm-?studio|koboldcpp|tabby|mlx/.test(provider.toLowerCase())

    Image {
        anchors.fill: parent
        visible: root.url !== ""
        source: root.url
        sourceSize: Qt.size(root.width * 2, root.height * 2)
        fillMode: Image.PreserveAspectFit
        smooth: true
    }
    Icon {
        anchors.fill: parent
        visible: root.url === "" && root.local
        name: "server"
        color: root.color
    }
    Rectangle {
        anchors.fill: parent
        visible: root.url === "" && !root.local && root.provider !== ""
        radius: width / 2
        color: "transparent"
        border.color: root.color
        Text {
            anchors.centerIn: parent
            text: root.provider.charAt(0).toUpperCase()
            color: root.color
            font.pixelSize: parent.height * 0.6
            font.bold: true
        }
    }
}
