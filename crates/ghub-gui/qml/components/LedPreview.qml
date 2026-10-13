import QtQuick
import org.ghublinux.gui

// Brilho do logo do mouse: cor fixa, respiracao (pulsa) ou ciclo (gira o matiz).
Item {
    id: root
    property string effect: "fixed"
    property color color: Theme.accent
    property int periodMs: 4000
    implicitWidth: 120
    implicitHeight: 120

    property real phase: 0
    NumberAnimation on phase {
        running: root.effect === "breathing" || root.effect === "cycle"
        from: 0; to: 1; loops: Animation.Infinite
        duration: Math.max(500, root.periodMs)
    }

    readonly property color shown: {
        if (root.effect === "off") return "transparent"
        if (root.effect === "cycle") return Qt.hsla(root.phase, 0.9, 0.55, 1)
        return root.color
    }
    readonly property real level: root.effect === "breathing"
                                  ? 0.25 + 0.75 * (0.5 - 0.5 * Math.cos(2 * Math.PI * root.phase))
                                  : (root.effect === "off" ? 0 : 1)

    Rectangle {
        anchors.centerIn: parent
        width: parent.width; height: width; radius: width / 2
        color: root.shown
        opacity: 0.25 * root.level
    }
    Rectangle {
        anchors.centerIn: parent
        width: parent.width * 0.62; height: width; radius: width / 2
        color: root.shown
        opacity: 0.4 * root.level
    }
    Rectangle {
        anchors.centerIn: parent
        width: parent.width * 0.3; height: width; radius: width / 2
        color: root.effect === "off" ? Theme.hi : root.shown
        border.color: root.effect === "off" ? Theme.border : root.shown
        border.width: 2
        opacity: root.effect === "off" ? 1 : 0.4 + 0.6 * root.level
    }
}
