import QtQuick
import QtQuick.Shapes
import org.ghublinux.gui

// Marca "ghub": emblema (disco laranja + mouse visto de cima) e, opcionalmente,
// a palavra em tracos de ponta redonda com a etiqueta LINUX. Desenhada so com
// Shapes/PathSvg: sem imagens e sem depender de fontes. Geometria identica a
// packaging/icons/ghub-logo.svg (emblema em viewBox 128, palavra em unidades de 40).
Item {
    id: logo
    property int symbolSize: 30
    property bool showWordmark: true
    readonly property real wordScale: symbolSize * 0.0267 // 30 px -> 0.8 px por unidade
    readonly property real wordWidth: 84 * wordScale
    implicitWidth: symbolSize + (showWordmark ? 10 + wordWidth : 0)
    implicitHeight: symbolSize

    Shape {
        id: symbol
        width: 128; height: 128
        scale: logo.symbolSize / 128
        transformOrigin: Item.TopLeft
        antialiasing: true
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: "transparent"; fillColor: Theme.accent
            PathSvg { path: "M4 64A60 60 0 1 1 124 64A60 60 0 1 1 4 64Z" }
        }
        ShapePath {
            strokeColor: "transparent"; fillColor: Theme.bg
            PathSvg { path: "M38 46a26 26 0 0 1 52 0v36a26 26 0 0 1-52 0z" }
        }
        ShapePath {
            strokeColor: Theme.accent; strokeWidth: 9; fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            PathSvg { path: "M64 37v16" }
        }
        ShapePath {
            strokeColor: Theme.accent; strokeWidth: 5; fillColor: "transparent"
            capStyle: ShapePath.FlatCap
            PathSvg { path: "M38 67h52" }
        }
    }

    Shape {
        id: word
        visible: logo.showWordmark
        x: logo.symbolSize + 10
        y: (logo.symbolSize - 36 * logo.wordScale) / 2
        width: 84; height: 38
        scale: logo.wordScale
        transformOrigin: Item.TopLeft
        antialiasing: true
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: Theme.fg; strokeWidth: 4.2; fillColor: "transparent"
            capStyle: ShapePath.RoundCap; joinStyle: ShapePath.RoundJoin
            PathSvg { path: "M2 17A7 7 0 1 0 16 17A7 7 0 1 0 2 17M16 10V27A7 7 0 0 1 9 34H7" }
        }
        ShapePath {
            strokeColor: Theme.fg; strokeWidth: 4.2; fillColor: "transparent"
            capStyle: ShapePath.RoundCap; joinStyle: ShapePath.RoundJoin
            PathSvg { path: "M25 0V24M25 17A7 7 0 0 1 39 17V24M45 10V17A7 7 0 0 0 59 17M59 10V24M68 0V24M68 17A7 7 0 1 1 82 17A7 7 0 1 1 68 17" }
        }
        ShapePath {
            strokeColor: Theme.accent; strokeWidth: 1.8; fillColor: "transparent"
            capStyle: ShapePath.RoundCap; joinStyle: ShapePath.RoundJoin
            PathSvg { path: "M24 31V38H29.5M37 31V38M44 38V31L50 38V31M58 31V35A3 3 0 0 0 64 35V31M72 31L78 38M78 31L72 38" }
        }
    }
}
