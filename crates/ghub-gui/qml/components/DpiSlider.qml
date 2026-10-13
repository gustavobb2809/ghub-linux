import QtQuick
import org.ghublinux.gui

// Slider do mock: trilho de 6px, parte preenchida, manopla redonda de 18px.
Item {
    id: s
    property real from: 0
    property real to: 1
    property real stepSize: 1
    property real value: 0
    property bool active: false
    readonly property bool pressed: area.pressed
    readonly property real position: to > from ? Math.max(0, Math.min(1, (value - from) / (to - from))) : 0
    signal moved()
    implicitHeight: 18
    implicitWidth: 120

    function _setFromX(x) {
        const usable = Math.max(1, s.width - 18)
        const f = Math.max(0, Math.min(1, (x - 9) / usable))
        let v = s.from + f * (s.to - s.from)
        if (s.stepSize > 0) v = s.from + Math.round((v - s.from) / s.stepSize) * s.stepSize
        v = Math.max(s.from, Math.min(s.to, v))
        if (v !== s.value) { s.value = v; s.moved() }
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
            color: s.active ? Theme.yellow : Theme.border
        }
    }
    Rectangle {
        id: handle
        width: 18; height: 18; radius: 9
        anchors.verticalCenter: parent.verticalCenter
        x: s.position * (s.width - 18)
        color: s.active ? Theme.yellow : Theme.fg
        border.color: Theme.bg
        border.width: 3
    }
    MouseArea {
        id: area
        anchors.fill: parent
        enabled: s.enabled
        onPressed: mouse => s._setFromX(mouse.x)
        onPositionChanged: mouse => { if (pressed) s._setFromX(mouse.x) }
    }
}
