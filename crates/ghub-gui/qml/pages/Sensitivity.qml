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
    readonly property var liftOffLevels: JSON.parse(backend.liftOffLevelsJson)
    function liftOffLabel(name) {
        return ({ low: "Baixa", medium: "Média", high: "Alta" })[name] || name
    }
    // "usable": mouse acordado e DPI ja lido (nunca mostrar "0 DPI")
    readonly property bool usable: backend.connected && backend.dpiCurrent > 0
    property string rowError: ""
    property int rowErrorIndex: -1
    property int commits: 0
    // houve mudança de DPI desde a última gravação (só então o "Gravado" aparece)
    property bool pendingEdit: false

    function commit() {
        page.commits++
        page.backend.commitDpi()
        if (page.pendingEdit && page.rowError === "") savedFlash.restart()
        page.pendingEdit = false
    }
    // teclado e roda gravam depois de uma pausa, não a cada passo
    Timer { id: nudgeCommit; interval: 400; onTriggered: page.commit() }
    Timer { id: savedFlash; interval: 1800 }
    // usado pelo smoke: simula soltar o slider / terminar a edicao do campo
    function simulateRelease(i) { levelRepeater.itemAt(i).slider.pressedChanged() }
    function fieldText(i) { return levelRepeater.itemAt(i).field.text }
    function simulateEdit(i, t) {
        const it = levelRepeater.itemAt(i)
        it.field.text = t
        it.field.editingFinished()
    }
    // menor largura de trilho entre as linhas (smoke de geometria)
    function minTrackWidth() {
        let m = 1e9
        for (let i = 0; i < levelRepeater.count; i++) m = Math.min(m, levelRepeater.itemAt(i).slider.width)
        return m
    }

    function fail(index, err) {
        page.rowError = err
        page.rowErrorIndex = err === "" ? -1 : index
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

            Card {
                Layout.fillWidth: true
                padH: 24; padV: 22
                heading: "Níveis de DPI"
                subtitle: "Até 5 níveis. Escolha o ativo; o botão de DPI do mouse alterna entre eles."
                headerExtra: PillButton {
                    text: "Adicionar nível"
                    enabled: page.usable && page.levels.length < 5
                    onClicked: page.fail(-2, page.backend.addLevel())
                }
                Repeater {
                    id: levelRepeater
                    model: page.levels.length
                    delegate: ColumnLayout {
                        id: row
                        required property int index
                        readonly property int dpi: page.levels[index]
                        readonly property bool on: index === page.backend.activeLevel
                        property alias field: field
                        property alias slider: slider
                        Layout.fillWidth: true
                        spacing: 0
                        RowLayout {
                            Layout.fillWidth: true
                            Layout.topMargin: 9
                            Layout.bottomMargin: 9
                            spacing: 16
                            Text {
                                Layout.preferredWidth: 28
                                text: String(row.index + 1)
                                color: Theme.muted
                                font.family: Theme.mono
                                font.pixelSize: 13
                            }
                            DpiSlider {
                                id: slider
                                Layout.fillWidth: true
                                Layout.minimumWidth: 60
                                enabled: page.usable
                                active: row.on
                                from: page.backend.dpiMin
                                to: page.backend.dpiMax
                                stepSize: page.backend.dpiStep > 0 ? page.backend.dpiStep : 50
                                value: row.dpi
                                logScale: true
                                ticks: [400, 800, 1600, 3200, 6400, 12800]
                                unit: "DPI"
                                onMoved: { page.pendingEdit = true; page.fail(row.index, page.backend.setDpiLevel(row.index, Math.round(value))) }
                                onPressedChanged: if (!pressed) page.commit()
                                onNudged: nudgeCommit.restart()
                                accessibleName: "Nível " + (row.index + 1) + " de DPI"
                            }
                            TextField {
                                id: field
                                Layout.preferredWidth: 120
                                enabled: page.usable
                                text: String(row.dpi)
                                inputMethodHints: Qt.ImhDigitsOnly
                                horizontalAlignment: Text.AlignRight
                                opacity: enabled ? 1 : 0.5
                                Accessible.name: "Nível " + (row.index + 1) + ", DPI"
                                color: row.on ? Theme.accentText : Theme.fg
                                font.family: Theme.mono
                                font.pixelSize: 14
                                leftPadding: 10; rightPadding: 10; topPadding: 7; bottomPadding: 7
                                background: Rectangle {
                                    radius: 8
                                    color: Theme.bg
                                    border.color: field.activeFocus || row.on ? Theme.accent : Theme.border
                                    border.width: 1
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
                                    if (err === "") { page.pendingEdit = true; page.commit() }
                                }
                            }
                            // rádio do nível ativo (16 px, borda 2 px)
                            AbstractButton {
                                id: radio
                                Layout.preferredWidth: 22
                                Layout.preferredHeight: 22
                                enabled: page.usable
                                opacity: enabled ? 1 : 0.5
                                focusPolicy: Qt.TabFocus
                                checkable: false
                                Accessible.role: Accessible.RadioButton
                                Accessible.name: "Usar o nível " + (row.index + 1) + " (" + row.dpi + " DPI)"
                                Accessible.checked: row.on
                                onClicked: page.backend.selectLevel(row.index)
                                background: Item {
                                    FocusRing { target: radio; baseRadius: 11; anchors.margins: -1 }
                                    Rectangle {
                                        anchors.centerIn: parent
                                        width: 16; height: 16; radius: 8
                                        color: "transparent"
                                        border.width: 2
                                        border.color: row.on ? Theme.accent : Theme.border
                                        Rectangle {
                                            visible: row.on
                                            anchors.centerIn: parent
                                            width: 7; height: 7; radius: 3.5
                                            color: Theme.accent
                                        }
                                    }
                                }
                                contentItem: Item { }
                                HoverHandler { cursorShape: Qt.PointingHandCursor }
                            }
                            AbstractButton {
                                id: removeBtn
                                Layout.preferredWidth: 22
                                Layout.preferredHeight: 22
                                enabled: page.usable && page.levels.length > 1
                                focusPolicy: Qt.TabFocus
                                Accessible.name: "Remover o nível " + (row.index + 1)
                                onClicked: page.fail(row.index, page.backend.removeLevel(row.index))
                                background: Item { FocusRing { target: removeBtn; baseRadius: 11; anchors.margins: -1 } }
                                contentItem: Text {
                                    horizontalAlignment: Text.AlignHCenter
                                    verticalAlignment: Text.AlignVCenter
                                    text: "✕"
                                    color: removeBtn.hovered && removeBtn.enabled ? Theme.badText : Theme.muted
                                    opacity: removeBtn.enabled ? 1 : 0.35
                                    font.family: Theme.ui
                                    font.pixelSize: 13
                                }
                                HoverHandler { cursorShape: removeBtn.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
                            }
                        }
                        Text {
                            visible: page.rowErrorIndex === row.index && page.rowError !== ""
                            text: page.rowError
                            color: Theme.badText
                            Layout.leftMargin: 44
                            Layout.bottomMargin: 6
                            font.family: Theme.ui
                            font.pixelSize: 12
                        }
                    }
                }
                Text {
                    visible: page.rowErrorIndex === -2 && page.rowError !== ""
                    Layout.topMargin: 6
                    text: page.rowError
                    color: Theme.badText
                    font.family: Theme.ui
                    font.pixelSize: 12
                }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.topMargin: 8
                    spacing: 12
                    Text {
                        Layout.fillWidth: true
                        text: page.backend.dpiMax > 0
                              ? "Faixa: " + page.backend.dpiMin + "–" + page.backend.dpiMax
                                + " DPI, passo " + page.backend.dpiStep + "."
                              : "Faixa disponível quando o mouse acordar."
                        color: Theme.muted
                        elide: Text.ElideRight
                        font.family: Theme.ui
                        font.pixelSize: 12
                    }
                    Text {
                        text: "✓ Gravado no mouse"
                        color: Theme.good
                        opacity: savedFlash.running ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
                        Accessible.role: Accessible.StaticText
                        Accessible.name: savedFlash.running ? text : ""
                        font.family: Theme.ui
                        font.pixelSize: 12
                        font.weight: Font.Medium
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.gap
                Card {
                    Layout.fillWidth: true
                    Layout.preferredWidth: 100
                    Layout.alignment: Qt.AlignTop
                    padH: 24; padV: 22
                    heading: "Taxa de polling"
                    subtitle: "Mais Hz = menos latência e mais consumo de bateria."
                    Text {
                        visible: page.pollingOptions.length === 0
                        text: !page.backend.connected
                              ? "Mouse dormindo — mova o mouse para ver a taxa."
                              : page.backend.pollingHz > 0
                                ? page.backend.pollingHz + " Hz — este mouse não permite alterar a taxa por aqui."
                                : "Indisponível neste mouse."
                        color: Theme.fgDim
                        font.family: Theme.mono
                        font.pixelSize: 13
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }
                    SegmentedControl {
                        visible: page.pollingOptions.length > 0
                        Layout.maximumWidth: parent.width
                        enabled: page.usable
                        name: "Taxa de polling"
                        mono: true
                        values: page.pollingOptions
                        current: page.backend.pollingHz
                        label: function (v) { return v === page.backend.pollingHz ? v + " Hz" : String(v) }
                        spoken: function (v) { return v + " Hz" }
                        onChosen: v => page.backend.setPolling(v)
                    }
                }
                // espaçador que mantém o card em meia largura; com muitas opções (7 no X2) o card
                // precisa da linha toda para a barra de botões caber
                Item { visible: page.pollingOptions.length <= 4; Layout.fillWidth: true; Layout.preferredWidth: 100 }
            }

            // Lift-off: só em mouses que permitem ajustar (DPI estendido, ex.: G Pro X2)
            RowLayout {
                visible: page.backend.liftOffSupported
                Layout.fillWidth: true
                spacing: Theme.gap
                Card {
                    Layout.fillWidth: true
                    Layout.preferredWidth: 100
                    Layout.alignment: Qt.AlignTop
                    padH: 24; padV: 22
                    heading: "Distância de levantamento"
                    subtitle: "Altura em que o sensor para de rastrear ao levantar o mouse."
                    SegmentedControl {
                        enabled: page.usable
                        name: "Distância de levantamento"
                        values: page.liftOffLevels
                        current: page.backend.liftOff
                        label: page.liftOffLabel
                        onChosen: v => page.backend.chooseLiftOff(v)
                    }
                }
                Item { Layout.fillWidth: true; Layout.preferredWidth: 100 }
            }
        }
    }
}
