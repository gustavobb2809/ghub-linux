import QtQuick
import org.ghublinux.gui

// Etiqueta com borda (.tag/.ok/.badge do mock).
Rectangle {
    id: tag
    property string text: ""
    property color tint: Theme.aqua
    property color edge: Theme.aquaDark
    property int fontSize: 11
    implicitWidth: label.implicitWidth + 16
    implicitHeight: label.implicitHeight + 6
    radius: 6
    color: "transparent"
    border.color: tag.edge
    border.width: 1
    Text {
        id: label
        anchors.centerIn: parent
        text: tag.text
        color: tag.tint
        font.family: Theme.ui
        font.pixelSize: tag.fontSize
    }
}
