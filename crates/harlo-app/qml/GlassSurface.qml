import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import harlo

// Glass, as a popup's background: a blur of what's behind (a
// little more saturated), the surface colour over it at the glass opacity,
// a faint light edge, and a soft drop shadow. With glass off (Settings →
// Appearance) it's the solid surface with the same shadow; without
// graphics acceleration (Qt's software renderer runs no shader effects)
// it's solid with a simple outline shadow.
//
// What's behind: for a menu, the dialog it opened over (the top of
// Theme.glassStack) or the window; for a dialog, the dialog under it or
// the window. Only lower layers are captured, so nothing blurs itself.
Item {
    id: glass
    property real radius: 10
    /// "menu" (menus, pickers, popovers) or "dialog".
    property string kind: "menu"
    readonly property bool dialog: kind === "dialog"
    /// The surface colour.
    property color tint: dialog ? Theme.background : Theme.elevated

    /// Shader effects (blur, soft shadows) can run.
    readonly property bool gpu: GraphicsInfo.api !== GraphicsInfo.Software
    readonly property bool on: Theme.glass && gpu && visible && source !== null
    // Menus mix 18% solid into the glass colour, dialogs don't.
    readonly property real alpha: !(Theme.glass && gpu) ? 1 : dialog ? Workspace.glassOpacity : 0.18 + 0.82 * Workspace.glassOpacity
    /// Blur is sampled this far around the surface, so its edges blur into
    /// what's next to them rather than into nothing.
    readonly property real pad: 24

    /// The popup item this belongs to (the overlay's child), found when
    /// it's shown.
    property Item popupItem: null
    function findPopupItem() {
        const overlay = Overlay.overlay
        let it = glass
        while (it && it.parent && it.parent !== overlay) it = it.parent
        return overlay && it && it.parent === overlay ? it : null
    }
    readonly property Item source: {
        const stack = Theme.glassStack
        const own = stack.indexOf(popupItem)
        const below = dialog ? (own > 0 ? stack[own - 1] : (own < 0 && stack.length > 0 ? stack[stack.length - 1] : null))
                             : (stack.length > 0 ? stack[stack.length - 1] : null)
        return below && below !== popupItem ? below : Theme.backdrop
    }
    // Where this sits over the source; follows the popup as it moves.
    readonly property rect area: {
        const p = popupItem
        if (!on || !p) return Qt.rect(0, 0, 0, 0)
        void [p.x, p.y, p.scale, glass.width, glass.height, source.width, source.height]
        const o = glass.mapToItem(source, 0, 0)
        return Qt.rect(o.x - pad, o.y - pad, glass.width + 2 * pad, glass.height + 2 * pad)
    }

    // Dialogs join the stack while shown, so what opens over them blurs them.
    onVisibleChanged: {
        if (visible) popupItem = findPopupItem()
        if (dialog && popupItem)
            Theme.glassStack = visible ? Theme.glassPush(Theme.glassStack, popupItem) : Theme.glassPop(Theme.glassStack, popupItem)
    }
    Component.onCompleted: if (visible) popupItem = findPopupItem()
    Component.onDestruction: if (popupItem) Theme.glassStack = Theme.glassPop(Theme.glassStack, popupItem)

    RectangularShadow {
        visible: glass.gpu
        anchors.fill: parent
        radius: glass.radius
        offset.y: glass.dialog ? 24 : 18
        blur: glass.dialog ? 72 : 44
        spread: glass.dialog ? -20 : -18
        color: Qt.rgba(0, 0, 0, Theme.isLight ? (glass.dialog ? 0.65 : 0.55) : (glass.dialog ? 0.9 : 0.8))
    }

    // Without shaders: a few faint outlines below the card.
    Repeater {
        model: glass.gpu ? 0 : 3
        Rectangle {
            required property int index
            anchors.fill: parent
            anchors.margins: -(index + 1) * 2
            anchors.topMargin: -(index + 1)
            radius: glass.radius + (index + 1) * 2
            color: "transparent"
            border.width: 2
            border.color: Qt.rgba(0, 0, 0, 0.16 - index * 0.045)
        }
    }

    // The window's colour, under what's captured (its empty parts show it).
    Rectangle {
        anchors.fill: parent
        radius: glass.radius
        visible: glass.on
        color: Theme.background
    }
    Item {
        x: -glass.pad
        y: -glass.pad
        width: glass.width + 2 * glass.pad
        height: glass.height + 2 * glass.pad
        visible: glass.on
        ShaderEffectSource {
            id: behind
            anchors.fill: parent
            visible: false
            sourceItem: glass.on ? glass.source : null
            sourceRect: glass.area
            live: true
        }
        Item {
            id: shape
            anchors.fill: parent
            visible: false
            layer.enabled: true
            Rectangle { x: glass.pad; y: glass.pad; width: glass.width; height: glass.height; radius: glass.radius }
        }
        MultiEffect {
            anchors.fill: parent
            source: behind
            autoPaddingEnabled: false
            blurEnabled: true
            // A 16 px blur in dark themes, 12 px in light ones, as
            // MultiEffect amounts.
            blurMax: 64
            blur: Theme.isLight ? 0.75 : 1.0
            saturation: Theme.isLight ? 0.14 : 0.08
            maskEnabled: true
            maskSource: shape
        }
    }

    // The surface colour, and the edge: a faint line all round, and a
    // lighter one along the top in the dark.
    Rectangle {
        anchors.fill: parent
        radius: glass.radius
        color: Qt.rgba(glass.tint.r, glass.tint.g, glass.tint.b, glass.alpha)
        border.color: Theme.isLight ? Qt.rgba(0, 0, 0, 0.1) : Qt.rgba(1, 1, 1, glass.dialog ? 0.08 : 0.1)
    }
    Rectangle {
        visible: !Theme.isLight
        x: glass.radius
        y: 1
        width: glass.width - 2 * glass.radius
        height: 1
        color: Qt.rgba(1, 1, 1, glass.dialog ? 0.04 : 0.06)
    }
}
