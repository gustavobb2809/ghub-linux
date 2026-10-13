import QtQuick
import QtQuick.Layouts
import org.ghublinux.gui

// Linha de configuracao (.row do mock): borda superior 1px, titulo + legenda e controle a direita.
Item {
    id: row
    property string text: ""
    property string note: ""
    default property alias trailing: right.data
    Layout.fillWidth: true
    implicitHeight: inner.implicitHeight + 24 + 1
    Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: 1; color: Theme.line }
    RowLayout {
        id: inner
        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; verticalCenterOffset: 0.5 }
        spacing: 16
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2
            Text {
                Layout.fillWidth: true
                text: row.text
                elide: Text.ElideRight
                color: Theme.fg
                font.family: Theme.ui
                font.pixelSize: 14
            }
            Text {
                visible: row.note !== ""
                Layout.fillWidth: true
                text: row.note
                elide: Text.ElideRight
                color: Theme.muted
                font.family: Theme.ui
                font.pixelSize: 12
            }
        }
        RowLayout { id: right; spacing: 8 }
    }
}
