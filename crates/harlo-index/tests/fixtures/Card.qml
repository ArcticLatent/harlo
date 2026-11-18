import QtQuick

Item {
    id: root
    property var run: null
    readonly property list<string> keys: []
    signal opened(string key)

    function stageColor(stage) {
        return Theme.colour(stage);
    }

    Rectangle {
        width: 3
    }

    FlatButton {
        id: openAsking
        onClicked: root.opened("x")
        function press() {}
        Label { id: hint; text: "" }
    }

    component Badge: Text {
        id: badge
        property int count: 0
    }
}
