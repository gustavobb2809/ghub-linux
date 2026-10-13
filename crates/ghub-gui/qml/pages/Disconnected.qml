import QtQuick
import QtQuick.Layouts
import org.ghublinux.gui

Item {
    id: page
    required property var backend
    ColumnLayout {
        anchors.centerIn: parent
        spacing: 14
        width: Math.min(460, page.width - 2 * Theme.gap)
        Text {
            Layout.alignment: Qt.AlignHCenter
            text: "Serviço não está rodando"
            color: Theme.fg
            font.family: Theme.uiFonts[0]
            font.pixelSize: 24
            font.bold: true
        }
        Text {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            text: "O ghub precisa do serviço em segundo plano (ghubd) para falar com o mouse. Inicie-o para continuar."
            color: Theme.fgDim
            font.family: Theme.uiFonts[0]
            font.pixelSize: 14
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
            color: Theme.fgDim
            font.family: Theme.monoFonts[0]
            font.pixelSize: 12
        }
    }
}
