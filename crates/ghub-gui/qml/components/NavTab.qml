import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic
import org.ghublinux.gui

// Aba superior do mock: texto #a89984, ativa #fabd2f com borda inferior de 2px.
AbstractButton {
    id: tab
    property string badge: ""
    leftPadding: 16; rightPadding: 16; topPadding: 14; bottomPadding: 14
    implicitHeight: row.implicitHeight + 28 + 2
    implicitWidth: row.implicitWidth + 32
    opacity: enabled ? 1 : 0.5
    background: Item {
        Rectangle {
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
            height: 2
            color: tab.checked ? Theme.yellow : "transparent"
        }
    }
    contentItem: RowLayout {
        id: row
        spacing: 8
        Text {
            text: tab.text
            color: tab.checked ? Theme.yellow : Theme.fgDim
            font.family: Theme.ui
            font.pixelSize: 14
        }
        Tag {
            visible: tab.badge !== ""
            text: tab.badge
            tint: Theme.yellow
            edge: Theme.yellowDark
        }
    }
}
