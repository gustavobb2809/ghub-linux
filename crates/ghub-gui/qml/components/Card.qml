import QtQuick
import QtQuick.Layouts
import org.ghublinux.gui

Rectangle {
    id: card
    default property alias content: col.data
    property string title: ""
    color: Theme.card
    radius: Theme.radius
    border.color: Theme.border
    border.width: 1
    implicitHeight: col.implicitHeight + 2 * Theme.gap
    implicitWidth: 200
    ColumnLayout {
        id: col
        anchors { fill: parent; margins: Theme.gap }
        spacing: 8
        Text {
            visible: card.title !== ""
            text: card.title
            color: Theme.fgDim
            font.family: Theme.uiFonts[0]
            font.pixelSize: 12
            font.capitalization: Font.AllUppercase
        }
    }
}
