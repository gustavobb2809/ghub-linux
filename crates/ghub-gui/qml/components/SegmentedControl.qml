import QtQuick
import org.ghublinux.gui

// Escolha única em segmentos (taxa de polling, lift-off). O clique e as setas aplicam na hora;
// Tab entra no grupo, Esquerda/Direita andam, Home/End vão às pontas.
// `values`: os valores; `label(v)`: o texto visível; `spoken(v)`: o nome para o leitor de tela.
// Quando o layout dá menos largura que a de uma linha, os segmentos quebram em mais linhas.
FocusScope {
    id: sc
    property var values: []
    property var current
    property var label: function (v) { return String(v) }
    property var spoken: function (v) { return sc.label(v) }
    property string name: ""
    property bool mono: false
    signal chosen(var value)

    readonly property int index: values.indexOf(current)
    // largura de uma linha só (a preferida); a altura acompanha a quebra
    readonly property real lineWidth: {
        let w = 0
        for (let i = 0; i < rep.count; i++) { const it = rep.itemAt(i); if (it) w += it.implicitWidth }
        return w + Math.max(0, rep.count - 1) * flow.spacing + 8
    }
    implicitWidth: lineWidth
    implicitHeight: flow.implicitHeight + 8
    activeFocusOnTab: true
    opacity: enabled ? 1 : 0.5
    Accessible.role: Accessible.Grouping
    Accessible.name: sc.name

    function pick(i) {
        if (!sc.enabled || sc.values.length === 0) return
        const v = sc.values[Math.max(0, Math.min(sc.values.length - 1, i))]
        if (v !== sc.current) sc.chosen(v)
    }
    Keys.onLeftPressed: pick(index < 0 ? 0 : index - 1)
    Keys.onRightPressed: pick(index < 0 ? 0 : index + 1)
    Keys.onPressed: e => {
        if (e.key === Qt.Key_Home) { pick(0); e.accepted = true }
        else if (e.key === Qt.Key_End) { pick(sc.values.length - 1); e.accepted = true }
    }

    Rectangle {
        id: frame
        width: sc.width
        height: sc.height
        color: Theme.bg
        radius: 10
        border.color: Theme.hi
        border.width: 1
        FocusRing { target: sc; baseRadius: 10 }
        Flow {
            id: flow
            x: 4; y: 4
            width: Math.max(0, frame.width - 8)
            spacing: 3
            Repeater {
                id: rep
                model: sc.values
                delegate: Rectangle {
                    id: seg
                    required property var modelData
                    readonly property bool on: modelData === sc.current
                    implicitWidth: txt.implicitWidth + 30
                    width: implicitWidth
                    height: txt.implicitHeight + 16
                    radius: 7
                    color: on ? Theme.accent : (ma.containsMouse && sc.enabled ? Theme.alpha(Theme.fg, 0.06) : "transparent")
                    Behavior on color { ColorAnimation { duration: 120; easing.type: Easing.OutCubic } }
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: sc.spoken(seg.modelData)
                    Accessible.checkable: true
                    Accessible.checked: seg.on
                    Text {
                        id: txt
                        anchors.centerIn: parent
                        text: sc.label(seg.modelData)
                        color: seg.on ? Theme.onAccent : Theme.fgDim
                        font.family: sc.mono ? Theme.mono : Theme.ui
                        font.pixelSize: 13
                        font.weight: seg.on ? Font.DemiBold : Font.Normal
                    }
                    MouseArea {
                        id: ma
                        anchors.fill: parent
                        hoverEnabled: true
                        enabled: sc.enabled
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            sc.forceActiveFocus(Qt.MouseFocusReason)
                            if (!seg.on) sc.chosen(seg.modelData)
                        }
                    }
                }
            }
        }
    }
}
