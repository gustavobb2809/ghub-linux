import QtQuick
import QtQuick.Layouts
import org.ghublinux.gui

// Cartao do mock: fundo #32302f, borda #3c3836, raio 14.
// `title` = rotulo pequeno em maiusculas (Visao geral); `heading`/`subtitle` = h2 15/600 + p 13.
Rectangle {
    id: card
    default property alias content: col.data
    property alias headerExtra: extra.data
    property string title: ""
    property string heading: ""
    property string subtitle: ""
    property int padH: 20
    property int padV: 20
    color: Theme.card
    radius: Theme.cardRadius
    border.color: Theme.line
    border.width: 1
    implicitHeight: col.implicitHeight + 2 * padV
    implicitWidth: 200
    ColumnLayout {
        id: col
        anchors { fill: parent; leftMargin: card.padH; rightMargin: card.padH; topMargin: card.padV; bottomMargin: card.padV }
        spacing: 0
        Text {
            visible: card.title !== ""
            Layout.bottomMargin: 14
            text: card.title
            color: Theme.muted
            font.family: Theme.ui
            font.pixelSize: 12
            font.weight: Font.Medium
            font.letterSpacing: 1.2
            font.capitalization: Font.AllUppercase
        }
        RowLayout {
            visible: card.heading !== ""
            Layout.fillWidth: true
            Layout.bottomMargin: card.subtitle !== "" ? 18 : 12
            spacing: extra.children.length > 0 ? 16 : 0
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 4
                Text {
                    text: card.heading
                    color: Theme.fg
                    font.family: Theme.ui
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Text {
                    visible: card.subtitle !== ""
                    Layout.fillWidth: true
                    text: card.subtitle
                    color: Theme.muted
                    wrapMode: Text.WordWrap
                    font.family: Theme.ui
                    font.pixelSize: 13
                }
            }
            Row { id: extra; Layout.alignment: Qt.AlignTop; spacing: 8 }
        }
    }
}
