import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic
import org.ghublinux.gui

AbstractButton {
    id: tab
    property string badge: ""
    implicitHeight: 40
    implicitWidth: 180
    opacity: enabled ? 1 : 0.45
    contentItem: RowLayout {
        spacing: 8
        Text {
            Layout.fillWidth: true
            text: tab.text
            color: tab.checked ? Theme.accent : Theme.fg
            font.family: Theme.uiFonts[0]
            font.pixelSize: 14
            elide: Text.ElideRight
        }
        Rectangle {
            visible: tab.badge !== ""
            implicitWidth: badgeText.implicitWidth + 12
            implicitHeight: 18
            radius: 9
            color: Theme.border
            Text {
                id: badgeText
                anchors.centerIn: parent
                text: tab.badge
                color: Theme.fg
                font.family: Theme.monoFonts[0]
                font.pixelSize: 10
            }
        }
    }
    background: Rectangle {
        radius: Theme.radius
        color: tab.checked ? Theme.cardHi : (tab.hovered && tab.enabled ? Theme.card : "transparent")
    }
    leftPadding: 12
    rightPadding: 12
}
