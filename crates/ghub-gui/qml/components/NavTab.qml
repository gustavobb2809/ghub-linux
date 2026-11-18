import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic
import org.ghublinux.gui

// Aba superior: ativa no destaque do tema, com borda inferior de 2 px.
AbstractButton {
    id: tab
    property string badge: ""
    leftPadding: 16; rightPadding: 16; topPadding: 14; bottomPadding: 14
    implicitHeight: row.implicitHeight + 28 + 2
    implicitWidth: row.implicitWidth + 32
    opacity: enabled ? 1 : 0.5
    focusPolicy: Qt.TabFocus
    Accessible.role: Accessible.PageTab
    background: Item {
        FocusRing { target: tab; baseRadius: 8; anchors.margins: 2 }
        Rectangle {
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
            height: 2
            color: tab.checked ? Theme.accent : "transparent"
        }
    }
    contentItem: RowLayout {
        id: row
        spacing: 8
        Text {
            text: tab.text
            color: tab.checked ? Theme.accentText : (tab.hovered && tab.enabled ? Theme.fg2 : Theme.fgDim)
            font.family: Theme.ui
            font.pixelSize: 14
        }
        Tag {
            visible: tab.badge !== ""
            text: tab.badge
            tint: Theme.accentText
            edge: Theme.accentDark
        }
    }
}
