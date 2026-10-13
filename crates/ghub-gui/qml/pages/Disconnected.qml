import QtQuick
import QtQuick.Layouts
import org.ghublinux.gui

Item {
    id: page
    required property var backend
    Card {
        anchors.centerIn: parent
        width: Math.min(520, page.width - 2 * Theme.pad)
        padH: 28; padV: 28
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 14
            Text {
                Layout.alignment: Qt.AlignHCenter
                text: "Serviço não está rodando"
                color: Theme.fg
                font.family: Theme.ui
                font.pixelSize: 20
                font.weight: Font.DemiBold
            }
            Text {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                text: "O ghub precisa do serviço em segundo plano (ghubd) para falar com o mouse. Inicie-o para continuar."
                color: Theme.muted
                font.family: Theme.ui
                font.pixelSize: 13
            }
            PillButton {
                Layout.alignment: Qt.AlignHCenter
                primary: true
                text: "Iniciar serviço"
                onClicked: page.backend.startDaemon()
            }
            Text {
                Layout.alignment: Qt.AlignHCenter
                text: "systemctl --user start ghubd"
                color: Theme.muted
                font.family: Theme.mono
                font.pixelSize: 12
            }
        }
    }
}
