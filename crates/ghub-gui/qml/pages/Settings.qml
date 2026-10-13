import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic
import org.ghublinux.gui

Item {
    id: page
    required property var backend
    readonly property var profiles: JSON.parse(backend.profilesJson)
    property string confirming: ""
    property string createError: ""

    Flickable {
        id: flick
        anchors.fill: parent
        contentWidth: width
        contentHeight: grid.implicitHeight + 2 * Theme.pad
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { }

        GridLayout {
            id: grid
            x: Theme.pad; y: Theme.pad
            width: flick.width - 2 * Theme.pad
            columns: 2
            columnSpacing: Theme.gap
            rowSpacing: Theme.gap

            Card {
                Layout.fillWidth: true
                Layout.preferredWidth: 100
                Layout.alignment: Qt.AlignTop
                padH: 24; padV: 22
                heading: "Serviço em segundo plano"
                subtitle: "O ghubd aplica seus perfis assim que o mouse conecta, mesmo com a janela fechada."
                SettingRow {
                    text: "Estado"
                    note: "serviço de usuário do systemd"
                    Tag {
                        text: page.backend.daemonState === "running" ? "Ativo"
                              : (page.backend.daemonState === "stopped" ? "Parado" : "Conectando")
                        fontSize: 12
                        tint: page.backend.daemonState === "running" ? Theme.good
                              : (page.backend.daemonState === "stopped" ? Theme.bad : Theme.muted)
                        edge: page.backend.daemonState === "running" ? Theme.goodDark
                              : (page.backend.daemonState === "stopped" ? Theme.bad : Theme.hi)
                    }
                }
                SettingRow {
                    text: "Iniciar com a sessão"
                    note: page.backend.autostartKnown ? "ghubd.service" : "Serviço do systemd não encontrado"
                    Toggle {
                        visible: page.backend.autostartKnown
                        checked: page.backend.autostartEnabled
                        onToggled: {
                            page.backend.setAutostart(checked)
                            // o clique quebra o binding; reafirma para seguir o backend (falha => volta)
                            checked = Qt.binding(() => page.backend.autostartEnabled)
                        }
                    }
                }
                SettingRow {
                    visible: page.backend.daemonState === "stopped"
                    text: "Iniciar serviço"
                    note: "systemctl --user start ghubd"
                    PillButton { text: "Iniciar serviço"; onClicked: page.backend.startDaemon() }
                }
            }

            Card {
                Layout.fillWidth: true
                Layout.preferredWidth: 100
                Layout.alignment: Qt.AlignTop
                padH: 24; padV: 22
                heading: "Perfis"
                subtitle: "Toque em um perfil para ativá-lo. O ativo não pode ser apagado."
                Repeater {
                    model: page.profiles
                    delegate: SettingRow {
                        id: prow
                        required property string modelData
                        readonly property bool active: modelData === page.backend.activeProfile
                        readonly property bool canDelete: page.profiles.length > 1 && !active
                        readonly property bool asking: canDelete && page.confirming === modelData
                        onActiveChanged: if (active) page.confirming = ""
                        text: modelData
                        Tag {
                            visible: prow.active
                            text: "ativo"
                            fontSize: 12
                            tint: Theme.good
                            edge: Theme.goodDark
                        }
                        Text {
                            visible: prow.asking
                            text: "Apagar?"
                            color: Theme.bad
                            font.family: Theme.ui
                            font.pixelSize: 13
                        }
                        PillButton {
                            visible: prow.asking
                            implicitHeight: 32
                            text: "Sim"
                            onClicked: { page.confirming = ""; page.backend.deleteProfile(prow.modelData) }
                        }
                        PillButton {
                            visible: prow.asking
                            implicitHeight: 32
                            text: "Não"
                            onClicked: page.confirming = ""
                        }
                        PillButton {
                            visible: !prow.active && !prow.asking
                            implicitHeight: 32
                            text: "Ativar"
                            onClicked: { page.confirming = ""; page.backend.activateProfile(prow.modelData) }
                        }
                        PillButton {
                            visible: prow.canDelete && !prow.asking
                            implicitHeight: 32
                            text: "Apagar"
                            onClicked: page.confirming = prow.modelData
                        }
                    }
                }
                Item {
                    Layout.fillWidth: true
                    implicitHeight: createRow.implicitHeight + 25
                    Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: 1; color: Theme.line }
                    RowLayout {
                        id: createRow
                        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; verticalCenterOffset: 0.5 }
                        spacing: 10
                        TextField {
                            id: nameField
                            Layout.fillWidth: true
                            placeholderText: "Nome do novo perfil"
                            color: Theme.fg
                            placeholderTextColor: Theme.muted
                            font.family: Theme.ui
                            font.pixelSize: 13
                            leftPadding: 10; rightPadding: 10; topPadding: 7; bottomPadding: 7
                            background: Rectangle {
                                radius: 8
                                color: Theme.bg
                                border.color: nameField.activeFocus ? Theme.yellow : Theme.hi
                            }
                            onAccepted: create.clicked()
                        }
                        PillButton {
                            id: create
                            implicitHeight: 32
                            text: "Criar perfil"
                            onClicked: {
                                const err = page.backend.createProfile(nameField.text)
                                page.createError = err
                                if (err === "") nameField.text = ""
                            }
                        }
                    }
                }
                Text {
                    visible: page.createError !== ""
                    text: page.createError
                    color: Theme.bad
                    font.family: Theme.ui
                    font.pixelSize: 12
                }
            }

            Card {
                Layout.fillWidth: true
                Layout.preferredWidth: 100
                Layout.alignment: Qt.AlignTop
                padH: 24; padV: 22
                heading: "Sobre"
                subtitle: "Projeto independente, não afiliado à Logitech."
                SettingRow { text: "Nome"; Text { text: "G HUB Linux"; color: Theme.fg2; font.family: Theme.ui; font.pixelSize: 13 } }
                SettingRow {
                    text: "Versão"
                    Text { text: "0.1.0"; color: Theme.fg2; font.family: Theme.mono; font.pixelSize: 12 }
                }
            }
        }
    }
}
