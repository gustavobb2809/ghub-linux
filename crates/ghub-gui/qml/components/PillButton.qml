import QtQuick
import QtQuick.Controls.Basic
import org.ghublinux.gui

Button {
    id: btn
    property bool primary: false
    font.pixelSize: 14
    implicitHeight: 36
    leftPadding: 18
    rightPadding: 18
    contentItem: Text {
        text: btn.text
        font: btn.font
        color: btn.primary ? Theme.bg : Theme.fg
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    background: Rectangle {
        radius: height / 2
        color: btn.primary ? (btn.hovered ? Qt.lighter(Theme.accent, 1.1) : Theme.accent)
                           : (btn.hovered ? Qt.lighter(Theme.cardHi, 1.2) : Theme.cardHi)
        border.color: btn.checked ? Theme.accent : "transparent"
        border.width: btn.checked ? 2 : 0
        opacity: btn.enabled ? 1 : 0.4
    }
}
