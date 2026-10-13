import QtQuick
import QtQuick.Controls.Basic
import org.ghublinux.gui

// Botao do mock (.btn): raio 9, borda #504945; `primary` = laranja.
Button {
    id: btn
    property bool primary: false
    font.family: Theme.ui
    font.pixelSize: 13
    font.weight: Font.Medium
    implicitHeight: 36
    leftPadding: 16
    rightPadding: 16
    contentItem: Text {
        text: btn.text
        font: btn.font
        color: btn.primary ? Theme.bgHard : Theme.fg
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    background: Rectangle {
        radius: 9
        color: btn.primary ? (btn.hovered ? Qt.lighter(Theme.accent, 1.08) : Theme.accent)
                           : (btn.hovered ? Qt.lighter(Theme.raised, 1.15) : Theme.raised)
        border.color: btn.primary ? Theme.accent : Theme.hi
        border.width: 1
        opacity: btn.enabled ? 1 : 0.4
    }
}
