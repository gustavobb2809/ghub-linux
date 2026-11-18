import QtQuick
import org.ghublinux.gui

// Anel de foco do teclado: 2 px no destaque, 3 px fora do controle. Só aparece quando o foco veio
// do teclado (Tab, atalho); um clique de mouse não deixa anel.
Rectangle {
    id: ring
    property Item target: parent
    property real baseRadius: 9
    readonly property bool keyboard: target && target.activeFocus && target.focusReason !== Qt.MouseFocusReason
    anchors.fill: parent
    anchors.margins: -3
    radius: baseRadius + 3
    color: "transparent"
    border.width: 2
    border.color: Theme.focusRing
    visible: keyboard
}
