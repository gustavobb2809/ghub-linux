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

    ScrollView {
        id: scroll
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true
        ColumnLayout {
            width: scroll.availableWidth - 2 * Theme.gap
            x: Theme.gap
            y: Theme.gap
            spacing: Theme.gap

            Card {
                Layout.fillWidth: true
                title: "Perfis"
                Repeater {
                    model: page.profiles
                    delegate: Rectangle {
                        id: prow
                        required property string modelData
                        readonly property bool active: modelData === page.backend.activeProfile
                        readonly property bool canDelete: page.profiles.length > 1 && !active
                        readonly property bool asking: canDelete && page.confirming === modelData
                        onActiveChanged: if (active) page.confirming = ""
                        Layout.fillWidth: true
                        implicitHeight: 40
                        radius: 8
                        color: active ? Theme.cardHi : (rowArea.containsMouse ? Theme.bgSoft : "transparent")
                        MouseArea {
                            id: rowArea
                            anchors.fill: parent
                            hoverEnabled: true
                            onClicked: { page.confirming = ""; page.backend.activateProfile(prow.modelData) }
                        }
                        RowLayout {
                            anchors { fill: parent; leftMargin: 12; rightMargin: 8 }
                            spacing: 10
                            Text {
                                Layout.fillWidth: true
                                text: prow.modelData
                                elide: Text.ElideRight
                                color: Theme.fg
                                font.family: Theme.uiFonts[0]
                                font.pixelSize: 14
                            }
                            Text {
                                visible: prow.active
                                text: "ativo"
                                color: Theme.accent
                                font.family: Theme.uiFonts[0]
                                font.pixelSize: 12
                            }
                            Text {
                                visible: prow.asking
                                text: "Apagar?"
                                color: Theme.bad
                                font.family: Theme.uiFonts[0]
                                font.pixelSize: 13
                            }
                            PillButton {
                                visible: prow.asking
                                text: "Sim"
                                onClicked: { page.confirming = ""; page.backend.deleteProfile(prow.modelData) }
                            }
                            PillButton {
                                visible: prow.asking
                                text: "Não"
                                onClicked: page.confirming = ""
                            }
                            PillButton {
                                visible: prow.canDelete && !prow.asking
                                text: "Apagar"
                                onClicked: page.confirming = prow.modelData
                            }
                        }
                    }
                }
                RowLayout {
                    spacing: 10
                    TextField {
                        id: nameField
                        Layout.fillWidth: true
                        placeholderText: "Nome do novo perfil"
                        color: Theme.fg
                        placeholderTextColor: Theme.fgDim
                        font.family: Theme.uiFonts[0]
                        font.pixelSize: 14
                        background: Rectangle {
                            radius: 6
                            color: Theme.bgSoft
                            border.color: nameField.activeFocus ? Theme.accent : Theme.border
                        }
                        onAccepted: create.clicked()
                    }
                    PillButton {
                        id: create
                        text: "Criar perfil"
                        onClicked: {
                            const err = page.backend.createProfile(nameField.text)
                            page.createError = err
                            if (err === "") nameField.text = ""
                        }
                    }
                }
                Text {
                    visible: page.createError !== ""
                    text: page.createError
                    color: Theme.bad
                    font.family: Theme.uiFonts[0]
                    font.pixelSize: 12
                }
            }

            Card {
                Layout.fillWidth: true
                title: "Sistema"
                Switch {
                    visible: page.backend.autostartKnown
                    text: "Iniciar com o sistema"
                    checked: page.backend.autostartEnabled
                    onToggled: page.backend.setAutostart(checked)
                    contentItem: Text {
                        leftPadding: parent.indicator.width + 10
                        verticalAlignment: Text.AlignVCenter
                        text: parent.text
                        color: Theme.fg
                        font.family: Theme.uiFonts[0]
                        font.pixelSize: 14
                    }
                }
                Text {
                    visible: !page.backend.autostartKnown
                    text: "Serviço do systemd não encontrado"
                    color: Theme.fgDim
                    font.family: Theme.uiFonts[0]
                    font.pixelSize: 14
                }
            }

            Card {
                Layout.fillWidth: true
                title: "Sobre"
                Text {
                    text: "ghub — versão 0.1.0"
                    color: Theme.fg
                    font.family: Theme.uiFonts[0]
                    font.pixelSize: 14
                }
                Text {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    text: "Projeto independente, não afiliado à Logitech."
                    color: Theme.fgDim
                    font.family: Theme.uiFonts[0]
                    font.pixelSize: 12
                }
            }
        }
    }
}
