import QtQuick
import org.ghublinux.gui

// Slider de DPI: trilho de 6 px, parte preenchida e manopla de 18 px.
// Teclado (com foco): setas = 1 passo, Shift+setas e PageUp/PageDown = 10 passos, Home/End = pontas.
// A roda do mouse só ajusta com o slider focado, para não roubar a rolagem da página.
// `moved` acompanha o arrasto; `nudged` sai a cada ajuste por teclado ou roda (a página grava).
Item {
    id: s
    property real from: 0
    property real to: 1
    property real stepSize: 1
    property real value: 0
    property bool active: false
    property string accessibleName: ""
    readonly property bool pressed: area.pressed
    readonly property real position: to > from ? Math.max(0, Math.min(1, (value - from) / (to - from))) : 0
    signal moved()
    signal nudged()
    implicitHeight: 18
    implicitWidth: 120
    activeFocusOnTab: true

    Accessible.role: Accessible.Slider
    Accessible.name: s.accessibleName
    Accessible.description: Math.round(s.value) + " DPI"

    function _clamp(v) {
        if (s.stepSize > 0) v = s.from + Math.round((v - s.from) / s.stepSize) * s.stepSize
        return Math.max(s.from, Math.min(s.to, v))
    }
    function _setFromX(x) {
        const usable = Math.max(1, s.width - 18)
        const f = Math.max(0, Math.min(1, (x - 9) / usable))
        const v = s._clamp(s.from + f * (s.to - s.from))
        if (v !== s.value) { s.value = v; s.moved() }
    }
    function nudge(steps) {
        if (!s.enabled) return
        const v = s._clamp(s.value + steps * (s.stepSize > 0 ? s.stepSize : 1))
        if (v !== s.value) { s.value = v; s.moved(); s.nudged() }
    }
    function _jump(v) {
        if (!s.enabled) return
        v = s._clamp(v)
        if (v !== s.value) { s.value = v; s.moved(); s.nudged() }
    }

    Keys.onPressed: e => {
        const big = (e.modifiers & Qt.ShiftModifier) ? 10 : 1
        switch (e.key) {
        case Qt.Key_Left: case Qt.Key_Down: s.nudge(-big); break
        case Qt.Key_Right: case Qt.Key_Up: s.nudge(big); break
        case Qt.Key_PageDown: s.nudge(-10); break
        case Qt.Key_PageUp: s.nudge(10); break
        case Qt.Key_Home: s._jump(s.from); break
        case Qt.Key_End: s._jump(s.to); break
        default: return
        }
        e.accepted = true
    }

    Rectangle {
        id: track
        x: 9; width: s.width - 18
        anchors.verticalCenter: parent.verticalCenter
        height: 6; radius: 3
        color: Theme.hi
        opacity: s.enabled ? 1 : 0.6
        Rectangle {
            height: parent.height; radius: 3
            width: s.position * parent.width
            color: s.active ? Theme.accent : Theme.border
        }
    }
    Rectangle {
        id: handle
        width: 18; height: 18; radius: 9
        anchors.verticalCenter: parent.verticalCenter
        x: s.position * (s.width - 18)
        color: s.active ? Theme.accent : Theme.fg
        border.color: Theme.card
        border.width: 3
        FocusRing { target: s; baseRadius: 9 }
    }
    MouseArea {
        id: area
        anchors.fill: parent
        enabled: s.enabled
        onPressed: mouse => { s.forceActiveFocus(Qt.MouseFocusReason); s._setFromX(mouse.x) }
        onPositionChanged: mouse => { if (pressed) s._setFromX(mouse.x) }
        onWheel: wheel => {
            if (!s.activeFocus) { wheel.accepted = false; return }
            s.nudge(wheel.angleDelta.y > 0 ? 1 : -1)
        }
    }
}
