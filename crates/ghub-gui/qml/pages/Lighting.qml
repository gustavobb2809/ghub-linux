import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic
import org.ghublinux.gui

Item {
    id: page
    required property var backend

    readonly property var effects: JSON.parse(backend.ledEffectsJson)
    readonly property var labels: ({ "fixed": "Fixo", "breathing": "Respiração", "cycle": "Ciclo de cores", "off": "Desligado" })
    readonly property var swatches: ["#fe8019", "#fabd2f", "#b8bb26", "#8ec07c", "#83a598", "#d3869b", "#fb4934", "#ebdbb2"]
    readonly property bool usable: backend.connected && backend.ledSupported && !backend.ledBusy

    // Estado editado: comeca no do mouse e volta a ele quando o daemon muda.
    property string effect: backend.ledEffect
    property string color: backend.ledColor !== "" ? backend.ledColor : "#FE8019"
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

    RowLayout {
        id: body
        x: Theme.pad
        y: Theme.pad
        width: flick.width - 2 * Theme.pad
        spacing: Theme.gap

        Card {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignTop
            Layout.preferredWidth: 1
            Layout.preferredHeight: Math.max(420, controls.implicitHeight)
            Item {
                Layout.alignment: Qt.AlignHCenter | Qt.AlignVCenter
                implicitWidth: art.implicitWidth
                implicitHeight: art.implicitHeight
                MouseArt { id: art; maxWidth: 220; maxHeight: 340; showLogo: false }
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
                Flow {
                    Layout.fillWidth: true
                    spacing: 8
                    Repeater {
                        model: ["fixed", "breathing", "cycle", "off"].filter(e => page.effects.indexOf(e) >= 0)
                        delegate: PillButton {
                            required property string modelData
                            text: page.labels[modelData]
                            primary: page.effect === modelData
                            enabled: page.usable
                            onClicked: page.select(modelData)
                        }
                    }
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
                            required property string modelData
                            width: 38; height: 38; radius: 10
                            color: modelData
                            border.width: 2
                            border.color: page.color.toLowerCase() === modelData ? Theme.fg : Theme.line
                            MouseArea {
                                anchors.fill: parent
                                enabled: page.usable
                                onClicked: page.color = parent.modelData
                            }
                        }
                    }
                }
                TextField {
                    Layout.topMargin: 14
                    Layout.preferredWidth: 140
                    enabled: page.usable
                    text: page.color
                    font.family: Theme.mono
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
                        value: page.periodMs
                        onMoved: page.periodMs = Math.round(value)
                    }
                    Text { text: page.periodMs + " ms"; color: Theme.fg; font.family: Theme.mono }
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 16
                    Text { Layout.preferredWidth: 84; text: "Intensidade"; color: Theme.fgDim; font.family: Theme.ui; font.pixelSize: 13 }
                    DpiSlider {
                        Layout.fillWidth: true
                        active: true
                        from: 0; to: 100; stepSize: 5
                        enabled: page.usable
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
                    color: page.formError !== "" ? Theme.bad : Theme.muted
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
