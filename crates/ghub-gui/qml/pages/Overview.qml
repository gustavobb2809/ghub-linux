import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic
import QtQuick.Shapes
import org.ghublinux.gui

Item {
    id: page
    required property var backend
    readonly property bool asleep: backend.hasDevice && !backend.connected
    readonly property bool usable: backend.connected && backend.dpiCurrent > 0
    readonly property var levels: JSON.parse(backend.dpiLevelsJson)
    readonly property var profiles: JSON.parse(backend.profilesJson)
    // texto exibido no cartao de DPI ("1200 DPI" ou "lendo…"; nunca "0 DPI")
    readonly property string dpiShown: backend.dpiCurrent > 0 ? dpiValue.text + " " + dpiUnit.text : backend.dpiText
    readonly property string batteryTag: {
        const t = backend.batteryText
        if (t.indexOf("carregando") >= 0) return "Carregando"
        if (t.indexOf("cheia") >= 0) return "Cheia"
        return backend.batteryPercent >= 0 ? "Descarregando" : ""
    }

    Flickable {
        id: flick
        anchors.fill: parent
        contentWidth: width
        contentHeight: body.implicitHeight + 2 * Theme.pad
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { }

        ColumnLayout {
            id: body
            x: Theme.pad; y: Theme.pad
            width: flick.width - 2 * Theme.pad
            spacing: Theme.gap

            Rectangle {
                Layout.fillWidth: true
                visible: page.backend.profileDiffers
                color: Theme.warnBg
                border.color: Theme.warn
                radius: 10
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
                        font.family: Theme.ui
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

            // grade de 2 colunas 1.15fr / 1fr
            Item {
                id: grid
                Layout.fillWidth: true
                Layout.preferredHeight: Math.max(flick.height - 2 * Theme.pad
                                                 - (page.backend.profileDiffers ? bannerRow.implicitHeight + 24 + Theme.gap : 0),
                                                 right.implicitHeight)
                readonly property real colW: width - Theme.gap
                readonly property real leftW: colW * 1.15 / 2.15

                Item {
                    id: mouseCard
                    x: 0; y: 0
                    width: grid.leftW
                    height: grid.height
                    opacity: page.backend.connected ? 1 : 0.5
                    // fundo radial #3c3836 -> #2b2a29 (70%), centro em 50% 55%, raio 14
                    Shape {
                        anchors.fill: parent
                        ShapePath {
                            strokeColor: Theme.line
                            strokeWidth: 1
                            fillGradient: RadialGradient {
                                centerX: mouseCard.width * 0.5
                                centerY: mouseCard.height * 0.55
                                centerRadius: Math.sqrt(Math.pow(Math.max(centerX, mouseCard.width - centerX), 2)
                                                        + Math.pow(Math.max(centerY, mouseCard.height - centerY), 2)) * 0.7 + 1
                                focalX: centerX
                                focalY: centerY
                                GradientStop { position: 0.0; color: "#3c3836" }
                                GradientStop { position: 1.0; color: "#2b2a29" }
                            }
                            PathSvg {
                                path: {
                                    const w = mouseCard.width - 1, h = mouseCard.height - 1, r = Theme.cardRadius, o = 0.5
                                    return "M" + (o + r) + " " + o + " H" + (w - r) + " A" + r + " " + r + " 0 0 1 " + w + " " + (o + r)
                                         + " V" + (h - r) + " A" + r + " " + r + " 0 0 1 " + (w - r) + " " + h
                                         + " H" + (o + r) + " A" + r + " " + r + " 0 0 1 " + o + " " + (h - r)
                                         + " V" + (o + r) + " A" + r + " " + r + " 0 0 1 " + (o + r) + " " + o + " Z"
                                }
                            }
                        }
                    }
                    MouseArt {
                        anchors.centerIn: parent
                        maxWidth: Math.min(250, mouseCard.width - 40)
                        maxHeight: Math.min(386, mouseCard.height - 40)
                    }
                }

                ColumnLayout {
                    id: right
                    x: grid.leftW + Theme.gap
                    y: 0
                    width: grid.width - grid.leftW - Theme.gap
                    height: grid.height
                    spacing: Theme.gap

                    Card {
                        Layout.fillWidth: true
                        title: "Bateria"
                        opacity: page.backend.connected ? 1 : 0.5
                        RowLayout {
                            Layout.fillWidth: true
                            Layout.bottomMargin: 12
                            spacing: 6
                            Text {
                                text: page.backend.batteryPercent >= 0 ? String(page.backend.batteryPercent) : page.backend.batteryText
                                color: page.backend.batteryLow ? Theme.bad : Theme.fg
                                font.family: Theme.mono
                                font.pixelSize: 34
                                font.weight: Font.Medium
                            }
                            Text {
                                visible: page.backend.batteryPercent >= 0
                                Layout.alignment: Qt.AlignBaseline
                                text: "%"
                                color: Theme.muted
                                font.family: Theme.ui
                                font.pixelSize: 14
                            }
                            Item { Layout.fillWidth: true }
                            Tag {
                                visible: page.batteryTag !== ""
                                text: page.batteryTag
                                tint: page.backend.batteryLow ? Theme.bad : Theme.aqua
                                edge: page.backend.batteryLow ? Theme.bad : Theme.aquaDark
                            }
                        }
                        Rectangle {
                            Layout.fillWidth: true
                            height: 8; radius: 4
                            color: Theme.hi
                            clip: true
                            Rectangle {
                                height: parent.height; radius: 4
                                width: parent.width * Math.max(0, Math.min(100, page.backend.batteryPercent)) / 100
                                color: page.backend.batteryLow ? Theme.bad : Theme.good
                            }
                        }
                    }

                    Card {
                        Layout.fillWidth: true
                        title: "DPI ativo"
                        opacity: page.backend.connected ? 1 : 0.5
                        RowLayout {
                            Layout.fillWidth: true
                            Layout.bottomMargin: 14
                            spacing: 6
                            Text {
                                id: dpiValue
                                text: page.backend.dpiCurrent > 0 ? String(page.backend.dpiCurrent) : page.backend.dpiText
                                color: Theme.fg
                                font.family: Theme.mono
                                font.pixelSize: page.backend.dpiCurrent > 0 ? 34 : 22
                                font.weight: Font.Medium
                            }
                            Text {
                                id: dpiUnit
                                visible: page.backend.dpiCurrent > 0
                                Layout.alignment: Qt.AlignBaseline
                                text: "DPI"
                                color: Theme.muted
                                font.family: Theme.ui
                                font.pixelSize: 14
                            }
                            Item { Layout.fillWidth: true }
                            Text {
                                visible: page.usable && page.levels.length > 0
                                text: "Nível " + (page.backend.activeLevel + 1) + " de " + page.levels.length
                                color: Theme.muted
                                font.family: Theme.ui
                                font.pixelSize: 13
                            }
                        }
                        Flow {
                            Layout.fillWidth: true
                            spacing: 8
                            Repeater {
                                model: page.levels
                                delegate: Rectangle {
                                    id: chip
                                    required property int index
                                    required property int modelData
                                    readonly property bool on: index === page.backend.activeLevel
                                    implicitWidth: chipText.implicitWidth + 24
                                    implicitHeight: chipText.implicitHeight + 14 + 2
                                    radius: 8
                                    color: on ? Theme.yellow : Theme.raised
                                    border.color: on ? Theme.yellow : Theme.hi
                                    border.width: 1
                                    Text {
                                        id: chipText
                                        anchors.centerIn: parent
                                        text: String(chip.modelData)
                                        color: chip.on ? Theme.bg : Theme.fg2
                                        font.family: Theme.mono
                                        font.pixelSize: 13
                                        font.weight: chip.on ? Font.Medium : Font.Normal
                                    }
                                    MouseArea {
                                        anchors.fill: parent
                                        enabled: page.usable
                                        onClicked: page.backend.selectLevel(chip.index)
                                    }
                                }
                            }
                        }
                    }

                    Card {
                        Layout.fillWidth: true
                        title: "Taxa de polling"
                        opacity: page.backend.connected ? 1 : 0.5
                        RowLayout {
                            spacing: 6
                            Text {
                                text: page.backend.pollingHz > 0 ? String(page.backend.pollingHz) : "—"
                                color: Theme.fg
                                font.family: Theme.mono
                                font.pixelSize: 34
                                font.weight: Font.Medium
                            }
                            Text {
                                visible: page.backend.pollingHz > 0
                                Layout.alignment: Qt.AlignBaseline
                                text: "Hz"
                                color: Theme.muted
                                font.family: Theme.ui
                                font.pixelSize: 14
                            }
                        }
                    }

                    Card {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        title: "Perfis"
                        Repeater {
                            model: page.profiles
                            delegate: Rectangle {
                                id: prow
                                required property string modelData
                                readonly property bool active: modelData === page.backend.activeProfile
                                Layout.fillWidth: true
                                implicitHeight: 40
                                radius: 9
                                color: active ? Theme.raised : (area.containsMouse ? Qt.rgba(1, 1, 1, 0.04) : "transparent")
                                RowLayout {
                                    anchors { fill: parent; leftMargin: 12; rightMargin: 12 }
                                    spacing: 12
                                    Rectangle { width: 8; height: 8; radius: 4; color: prow.active ? Theme.yellow : Theme.hi }
                                    Text {
                                        Layout.fillWidth: true
                                        text: prow.modelData
                                        elide: Text.ElideRight
                                        color: prow.active ? Theme.fg : Theme.fg2
                                        font.family: Theme.ui
                                        font.pixelSize: 14
                                    }
                                    Text {
                                        visible: prow.active
                                        text: "ativo"
                                        color: Theme.muted
                                        font.family: Theme.ui
                                        font.pixelSize: 12
                                    }
                                }
                                MouseArea {
                                    id: area
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    onClicked: page.backend.activateProfile(prow.modelData)
                                }
                            }
                        }
                        Item { Layout.fillHeight: true }
                    }
                }
            }
        }
    }
}
