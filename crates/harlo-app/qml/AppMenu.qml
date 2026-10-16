import QtQuick
import QtQuick.Controls
import harlo

// Context menu in the app's style: a glass card with a soft shadow, quick fade.
Menu {
    id: menu
    padding: 5
    margins: 8
    overlap: 0
    delegate: AppMenuItem { }
    // As wide as its widest item (Menu itself doesn't look), within reason.
    width: {
        let w = 210
        for (let i = 0; i < count; ++i) {
            const it = itemAt(i)
            if (it && it.visible) w = Math.max(w, it.implicitWidth + leftPadding + rightPadding)
        }
        return Math.min(w, 420)
    }

    background: GlassSurface {
        implicitWidth: 210
        radius: 10
    }

    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Math.round(Theme.motion * 0.56) }
        NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: Math.round(Theme.motion * 0.56); easing.type: Easing.OutCubic }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; from: 1; to: 0; duration: Math.round(Theme.motion * 0.44) }
    }
}
