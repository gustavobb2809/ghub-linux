import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic
import org.ghublinux.gui

Item {
    id: page
    required property var backend
    readonly property alias levelCount: levelRepeater.count
    readonly property var levels: JSON.parse(backend.dpiLevelsJson)
    readonly property var pollingOptions: JSON.parse(backend.pollingOptionsJson)
    // "usable": mouse acordado e DPI ja lido (nunca mostrar "0 DPI")
    readonly property bool usable: backend.connected && backend.dpiCurrent > 0
    property string rowError: ""
    property int rowErrorIndex: -1
    property int commits: 0

    function commit() {
        page.commits++
        page.backend.commitDpi()
    }
    // usado pelo smoke: simula soltar o slider / terminar a edicao do campo
    function simulateRelease(i) { levelRepeater.itemAt(i).slider.pressedChanged() }
    function fieldText(i) { return levelRepeater.itemAt(i).field.text }
    function simulateEdit(i, t) {
        const it = levelRepeater.itemAt(i)
        it.field.text = t
        it.field.editingFinished()
    }

    function fail(index, err) {
        page.rowError = err
        page.rowErrorIndex = err === "" ? -1 : index
    }

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
            opacity: page.usable ? 1 : 0.5

            Text {
                text: page.backend.hasDevice && !page.backend.connected
                      ? "Mouse desconectado ou dormindo — mova o mouse"
                      : "DPI atual: " + page.backend.dpiText
                color: page.backend.hasDevice && !page.backend.connected ? Theme.warn : Theme.fgDim
                font.family: Theme.uiFonts[0]
                font.pixelSize: 14
            }

            Card {
                Layout.fillWidth: true
                title: "Níveis de DPI"
                Repeater {
                    id: levelRepeater
                    model: page.levels.length
                    delegate: ColumnLayout {
                        id: row
                        required property int index
                        readonly property int dpi: page.levels[index]
                        property alias field: field
                        property alias slider: slider
                        Layout.fillWidth: true
                        spacing: 2
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 10
                            RadioButton {
                                checked: row.index === page.backend.activeLevel
                                enabled: page.usable
                                text: ""
                                onClicked: page.backend.selectLevel(row.index)
                            }
                            TextField {
                                id: field
                                Layout.preferredWidth: 96
                                enabled: page.usable
                                text: String(row.dpi)
                                inputMethodHints: Qt.ImhDigitsOnly
                                color: Theme.fg
                                font.family: Theme.monoFonts[0]
                                font.pixelSize: 15
                                background: Rectangle {
                                    radius: 6
                                    color: Theme.bgSoft
                                    border.color: field.activeFocus ? Theme.accent : Theme.border
                                }
                                onEditingFinished: {
                                    const v = parseInt(text)
                                    const pure = /^[0-9]+$/.test(text.trim())
                                    // o usuario sobrescreveu `text`: restaura o binding
                                    text = Qt.binding(() => String(row.dpi))
                                    if (!pure || v > 2147483647) {
                                        page.fail(row.index, page.backend.validateDpi(0))
                                        return
                                    }
                                    if (!isNaN(v) && v === row.dpi) { page.fail(row.index, ""); return }
                                    const err = isNaN(v) ? page.backend.validateDpi(0)
                                                         : page.backend.setDpiLevel(row.index, v)
                                    page.fail(row.index, err)
                                    if (err === "") page.commit()
                                }
                            }
                            Slider {
                                id: slider
                                Layout.fillWidth: true
                                enabled: page.usable
                                from: page.backend.dpiMin
                                to: page.backend.dpiMax
                                stepSize: page.backend.dpiStep > 0 ? page.backend.dpiStep : 50
                                snapMode: Slider.SnapAlways
                                value: row.dpi
                                onMoved: page.fail(row.index, page.backend.setDpiLevel(row.index, Math.round(value)))
                                onPressedChanged: if (!pressed) page.commit()
                            }
                            PillButton {
                                text: "Remover"
                                enabled: page.usable && page.levels.length > 1
                                onClicked: page.fail(row.index, page.backend.removeLevel(row.index))
                            }
                        }
                        Text {
                            visible: page.rowErrorIndex === row.index && page.rowError !== ""
                            text: page.rowError
                            color: Theme.bad
                            Layout.leftMargin: 48
                            font.family: Theme.uiFonts[0]
                            font.pixelSize: 12
                        }
                    }
                }
                RowLayout {
                    spacing: 12
                    PillButton {
                        text: "Adicionar nível"
                        enabled: page.usable && page.levels.length < 5
                        onClicked: page.fail(-2, page.backend.addLevel())
                    }
                    Text {
                        visible: page.rowErrorIndex === -2 && page.rowError !== ""
                        text: page.rowError
                        color: Theme.bad
                        font.family: Theme.uiFonts[0]
                        font.pixelSize: 12
                    }
                }
                Text {
                    text: "Faixa: " + page.backend.dpiMin + "–" + page.backend.dpiMax
                          + " DPI, passo " + page.backend.dpiStep + ". Até 5 níveis."
                    color: Theme.fgDim
                    font.family: Theme.uiFonts[0]
                    font.pixelSize: 12
                }
            }

            Card {
                Layout.fillWidth: true
                title: "Taxa de polling"
                RowLayout {
                    spacing: 8
                    Repeater {
                        model: page.pollingOptions
                        delegate: PillButton {
                            required property int modelData
                            text: modelData + " Hz"
                            checkable: false
                            enabled: page.usable
                            primary: modelData === page.backend.pollingHz
                            onClicked: page.backend.setPolling(modelData)
                        }
                    }
                }
            }
        }
    }
}
