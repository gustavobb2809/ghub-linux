import QtQuick
import QtQuick.Controls.Basic
import org.ghublinux.gui

// Interruptor 40x22 do mock (verde ligado / cinza desligado).
Switch {
    id: tog
    implicitWidth: 40
    implicitHeight: 22
    padding: 0
    indicator: Rectangle {
        implicitWidth: 40
        implicitHeight: 22
        radius: 11
        color: tog.checked ? Theme.good : Theme.hi
        opacity: tog.enabled ? 1 : 0.5
        Rectangle {
            width: 16; height: 16; radius: 8
            y: 3
            x: tog.checked ? parent.width - width - 3 : 3
            color: tog.checked ? Theme.bg : Theme.fgDim
        }
    }
    contentItem: Item { }
}
