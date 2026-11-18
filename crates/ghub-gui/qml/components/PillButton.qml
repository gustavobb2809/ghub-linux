import QtQuick
import QtQuick.Controls.Basic
import org.ghublinux.gui

// Botão: raio 9; `primary` usa o destaque do tema. Desabilitado fica apagado (texto e borda), nunca
// um primário esmaecido que parece clicável.
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
        color: !btn.enabled ? Theme.muted : (btn.primary ? Theme.onAccent : Theme.fg)
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    background: Rectangle {
        radius: 9
        color: !btn.enabled ? "transparent"
               : btn.primary ? (btn.hovered ? Qt.lighter(Theme.accent, 1.08) : Theme.accent)
               : (btn.hovered ? Qt.lighter(Theme.raised, 1.15) : Theme.raised)
        border.color: !btn.enabled ? Theme.hi : (btn.primary ? Theme.accent : Theme.hi)
        border.width: 1
        FocusRing { target: btn; baseRadius: 9 }
    }
}
