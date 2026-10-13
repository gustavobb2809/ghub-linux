import QtQuick
import QtQuick.Layouts
import org.ghublinux.gui

Item {
    id: page
    required property var backend
    readonly property string dpiShown: dpiValue.text
    readonly property bool asleep: backend.hasDevice && !backend.connected
    readonly property bool usable: backend.connected && backend.dpiCurrent > 0

    ColumnLayout {
        anchors { fill: parent; margins: Theme.gap }
        spacing: Theme.gap
        RowLayout {
            spacing: 10
            Rectangle {
                width: 10; height: 10; radius: 5
                color: page.backend.connected ? Theme.good : Theme.fgDim
            }
            Text {
                text: page.backend.hasDevice ? page.backend.deviceName : "Nenhum dispositivo"
                color: Theme.fg
                font.family: Theme.uiFonts[0]
                font.pixelSize: 24
                font.bold: true
            }
        }
        Text {
            visible: page.asleep
            text: "Mouse desconectado ou dormindo — mova o mouse"
            color: Theme.warn
            font.family: Theme.uiFonts[0]
            font.pixelSize: 14
        }
        Rectangle {
            Layout.fillWidth: true
            visible: page.backend.profileDiffers
            color: "#4a3a12"
            border.color: Theme.warn
            radius: Theme.radius
            implicitHeight: bannerRow.implicitHeight + 24
            RowLayout {
                id: bannerRow
                anchors { fill: parent; margins: 12 }
                spacing: 12
                Text {
                    Layout.fillWidth: true
                    text: page.backend.profileDiffText
                    color: Theme.fg
                    wrapMode: Text.WordWrap
                    font.family: Theme.uiFonts[0]
                    font.pixelSize: 13
                }
                PillButton {
                    text: "Aplicar perfil"
                    primary: true
                    enabled: page.backend.connected
                    onClicked: page.backend.applyProfileToDevice()
                }
            }
        }
        GridLayout {
            Layout.fillWidth: true
            columns: page.width < 1000 ? 2 : 4
            columnSpacing: Theme.gap
            rowSpacing: Theme.gap
            opacity: page.backend.connected ? 1 : 0.5

            Card {
                Layout.fillWidth: true
                Layout.preferredWidth: 100
                title: "Bateria"
                Text {
                    text: page.backend.batteryText
                    color: page.backend.batteryLow ? Theme.bad : Theme.fg
                    font.family: Theme.monoFonts[0]
                    font.pixelSize: 28
                }
            }
            Card {
                Layout.fillWidth: true
                Layout.preferredWidth: 100
                title: "DPI atual"
                Text {
                    id: dpiValue
                    text: page.backend.dpiText
                    color: Theme.accent
                    font.family: Theme.monoFonts[0]
                    font.pixelSize: 28
                }
            }
            Card {
                Layout.fillWidth: true
                Layout.preferredWidth: 100
                title: "Polling"
                Text {
                    text: page.backend.pollingText
                    color: Theme.fg
                    font.family: Theme.monoFonts[0]
                    font.pixelSize: 28
                }
            }
            Card {
                Layout.fillWidth: true
                Layout.preferredWidth: 100
                title: "Perfil"
                Text {
                    text: page.backend.activeProfile
                    color: Theme.fg
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                    font.family: Theme.uiFonts[0]
                    font.pixelSize: 22
                }
            }
        }
        Item { Layout.fillHeight: true }
    }
}
