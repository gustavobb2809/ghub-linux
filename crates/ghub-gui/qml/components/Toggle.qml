import QtQuick
import QtQuick.Controls.Basic
import org.ghublinux.gui

// Interruptor 40x22: ligado no verde do tema; desligado com trilha de contraste 3:1.
Switch {
    id: tog
    implicitWidth: 40
    implicitHeight: 22
    padding: 0
    indicator: Rectangle {
        implicitWidth: 40
        implicitHeight: 22
        radius: 11
        color: tog.checked ? Theme.good : "transparent"
        border.color: tog.checked ? Theme.good : Theme.border
        border.width: 2
        opacity: tog.enabled ? 1 : 0.5
        FocusRing { target: tog; baseRadius: 11 }
        Rectangle {
            width: 16; height: 16; radius: 8
            y: 3
            x: tog.checked ? parent.width - width - 3 : 3
            color: tog.checked ? Theme.bg : Theme.border
        }
    }
    contentItem: Item { }
}
