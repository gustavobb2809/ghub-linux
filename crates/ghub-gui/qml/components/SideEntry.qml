import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic
import org.ghublinux.gui

// Entrada da barra lateral (.dev do mock): icone + nome + subtitulo opcional.
AbstractButton {
    id: ent
    property string subtitle: ""
    property color dotColor: "transparent"
    property bool showDot: false
    property string glyph1: ""
    property string glyph2: ""
    property color glyphColor: Theme.fgDim
    leftPadding: 10; rightPadding: 10; topPadding: 10; bottomPadding: 10
    implicitHeight: row.implicitHeight + 20
    implicitWidth: 200
    opacity: enabled ? 1 : 0.55
    background: Rectangle {
        radius: 9
        color: ent.checked ? Theme.raised : (ent.hovered && ent.enabled ? Qt.rgba(1, 1, 1, 0.04) : "transparent")
    }
    contentItem: RowLayout {
        id: row
        spacing: 10
        Glyph { d1: ent.glyph1; d2: ent.glyph2; color: ent.glyphColor; size: 22 }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 0
            Text {
                Layout.fillWidth: true
                text: ent.text
                elide: Text.ElideRight
                color: ent.checked ? Theme.fg : Theme.fg2
                font.family: Theme.ui
                font.pixelSize: 14
            }
            RowLayout {
                visible: ent.subtitle !== ""
                spacing: 6
                Rectangle { visible: ent.showDot; width: 8; height: 8; radius: 4; color: ent.dotColor }
                Text {
                    Layout.fillWidth: true
                    text: ent.subtitle
                    elide: Text.ElideRight
                    color: Theme.muted
                    font.family: Theme.ui
                    font.pixelSize: 12
                }
            }
        }
    }
}
