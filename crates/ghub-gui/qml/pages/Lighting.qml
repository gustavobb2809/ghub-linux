import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic
import org.ghublinux.gui

Item {
    id: page
    required property var backend

    readonly property var effects: JSON.parse(backend.ledEffectsJson)
    readonly property var labels: ({ "fixed": "Fixo", "breathing": "Respiração", "cycle": "Ciclo de cores", "off": "Desligado" })
    // paleta do tema, com o destaque do sistema primeiro (o mouse acende na cor do desktop)
    readonly property var swatches: Theme.ledSwatches
    readonly property bool usable: backend.connected && backend.ledSupported && !backend.ledBusy
    // em janelas estreitas (tile de 900 px) a prévia vai para cima e os controles ganham a largura toda
    readonly property bool stacked: width < 980

    // Estado editado: comeca no do mouse e volta a ele quando o daemon muda.
    property string effect: backend.ledEffect
    property string color: backend.ledColor !== "" ? backend.ledColor : String(Theme.ledSwatches[0]).toUpperCase()
    property int periodMs: backend.ledPeriodMs > 0 ? backend.ledPeriodMs : 4000
    property int intensity: backend.ledIntensity
    property string formError: ""

    readonly property bool wantsColor: effect === "fixed" || effect === "breathing"
    readonly property bool wantsMotion: effect === "cycle" || effect === "breathing"
    readonly property bool dirty: effect !== backend.ledEffect
        || (wantsColor && color.toUpperCase() !== backend.ledColor.toUpperCase())
        || (wantsMotion && (periodMs !== backend.ledPeriodMs || intensity !== backend.ledIntensity))
    readonly property bool applyEnabled: usable && dirty

    Connections {
        target: page.backend
        function onLedEffectChanged() { page.effect = page.backend.ledEffect }
        function onLedColorChanged() { if (page.backend.ledColor !== "") page.color = page.backend.ledColor }
        function onLedPeriodMsChanged() { if (page.backend.ledPeriodMs > 0) page.periodMs = page.backend.ledPeriodMs }
        function onLedIntensityChanged() { page.intensity = page.backend.ledIntensity }
    }

    function select(e) { page.effect = e }
    function apply() {
        page.formError = page.backend.applyLed(page.effect, page.color, page.periodMs, page.intensity)
    }
    function busyText() { return backend.ledBusy ? "Gravando no mouse…" : "" }

    Flickable {
        id: flick
        anchors.fill: parent
        contentWidth: width
        contentHeight: body.implicitHeight + 2 * Theme.pad
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { }

    GridLayout {
        id: body
        x: Theme.pad
        y: Theme.pad
        width: flick.width - 2 * Theme.pad
        columns: page.stacked ? 1 : 2
        columnSpacing: Theme.gap
        rowSpacing: Theme.gap

        Card {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignTop
            Layout.preferredWidth: 1
            Layout.preferredHeight: page.stacked ? 300 : Math.max(420, controls.implicitHeight)
            Item {
                Layout.alignment: Qt.AlignHCenter | Qt.AlignVCenter
                implicitWidth: art.implicitWidth
                implicitHeight: art.implicitHeight
                MouseArt { id: art; maxWidth: 220; maxHeight: page.stacked ? 240 : 340; showLogo: false }
                LedPreview {
                    // Logo no ponto (110, 236) do desenho; o brilho cobre ~110 unidades.
                    width: 110 * art.k; height: width
                    x: 110 * art.k - width / 2
                    y: 236 * art.k - height / 2
                    effect: page.effect
                    color: page.color
                    periodMs: page.periodMs
                }
            }
        }

        ColumnLayout {
            id: controls
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignTop
            Layout.preferredWidth: 1
            spacing: Theme.gap

            Card {
                Layout.fillWidth: true
                heading: "Efeito"
                subtitle: "Gravado no perfil ativo do mouse."
                Text {
                    visible: !page.backend.connected
                    Layout.fillWidth: true
                    text: "Mouse dormindo — mova o mouse para ajustar a iluminação."
                    color: Theme.muted
                    wrapMode: Text.WordWrap
                    font.family: Theme.ui
                    font.pixelSize: 13
                }
                SegmentedControl {
                    visible: page.backend.connected
                    Layout.maximumWidth: parent.width
                    enabled: page.usable
                    name: "Efeito do LED"
                    values: ["fixed", "breathing", "cycle", "off"].filter(e => page.effects.indexOf(e) >= 0)
                    current: page.effect
                    label: function (v) { return page.labels[v] }
                    onChosen: v => page.select(v)
                }
            }

            Card {
                Layout.fillWidth: true
                visible: page.wantsColor
                heading: "Cor"
                subtitle: "Escolha uma da paleta ou digite #RRGGBB."
                Flow {
                    Layout.fillWidth: true
                    spacing: 10
                    Repeater {
                        model: page.swatches
                        delegate: Rectangle {
                            id: sw
                            required property string modelData
                            readonly property bool on: page.color.toLowerCase() === String(modelData).toLowerCase()
                            width: 38; height: 38; radius: 10
                            color: modelData
                            border.width: 2
                            border.color: on ? Theme.fg : Theme.line
                            activeFocusOnTab: page.usable
                            Accessible.role: Accessible.RadioButton
                            Accessible.name: "Cor " + String(modelData).toUpperCase()
                            Accessible.checked: on
                            Keys.onReturnPressed: page.color = String(sw.modelData).toUpperCase()
                            Keys.onSpacePressed: page.color = String(sw.modelData).toUpperCase()
                            FocusRing { target: sw; baseRadius: 10 }
                            MouseArea {
                                anchors.fill: parent
                                enabled: page.usable
                                cursorShape: Qt.PointingHandCursor
                                onClicked: page.color = String(sw.modelData).toUpperCase()
                            }
                        }
                    }
                }
                TextField {
                    id: hexField
                    Layout.topMargin: 14
                    Layout.preferredWidth: 140
                    enabled: page.usable
                    text: page.color
                    color: Theme.fg
                    selectionColor: Theme.accent
                    selectedTextColor: Theme.onAccent
                    font.family: Theme.mono
                    font.pixelSize: 14
                    leftPadding: 10; rightPadding: 10; topPadding: 7; bottomPadding: 7
                    Accessible.name: "Cor em hexadecimal"
                    background: Rectangle {
                        radius: 8
                        color: Theme.bg
                        border.color: hexField.activeFocus ? Theme.accent : Theme.border
                        border.width: 1
                    }
                    onEditingFinished: page.color = text
                }
            }

            Card {
                Layout.fillWidth: true
                visible: page.wantsMotion
                heading: "Velocidade e intensidade"
                subtitle: "Período do efeito em ms e brilho (0 = 100%)."
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 16
                    Text { Layout.preferredWidth: 84; text: "Período"; color: Theme.fgDim; font.family: Theme.ui; font.pixelSize: 13 }
                    DpiSlider {
                        Layout.fillWidth: true
                        active: true
                        from: 500; to: 20000; stepSize: 100
                        enabled: page.usable
                        accessibleName: "Período do efeito"
                        unit: "ms"
                        value: page.periodMs
                        onMoved: page.periodMs = Math.round(value)
                    }
                    Text { text: page.periodMs + " ms"; color: Theme.fg; font.family: Theme.mono }
                }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.topMargin: 14
                    spacing: 16
                    Text { Layout.preferredWidth: 84; text: "Intensidade"; color: Theme.fgDim; font.family: Theme.ui; font.pixelSize: 13 }
                    DpiSlider {
                        Layout.fillWidth: true
                        active: true
                        from: 0; to: 100; stepSize: 5
                        enabled: page.usable
                        accessibleName: "Intensidade do brilho"
                        unit: "%"
                        value: page.intensity
                        onMoved: page.intensity = Math.round(value)
                    }
                    Text { text: page.intensity + "%"; color: Theme.fg; font.family: Theme.mono }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 12
                Text {
                    Layout.fillWidth: true
                    text: page.formError !== "" ? page.formError
                          : (page.backend.ledBusy ? "Gravando no mouse…"
                             : "Ao aplicar, a mudança é gravada no mouse.")
                    color: page.formError !== "" ? Theme.badText : Theme.muted
                    wrapMode: Text.WordWrap
                    font.family: Theme.ui
                    font.pixelSize: 13
                }
                PillButton {
                    text: "Aplicar"
                    primary: true
                    enabled: page.applyEnabled
                    onClicked: page.apply()
                }
            }
        }
    }
    }
}
