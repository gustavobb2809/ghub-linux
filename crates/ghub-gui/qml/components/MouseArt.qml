import QtQuick
import QtQuick.Shapes
import org.ghublinux.gui

// Ilustracao do mouse (viewBox 220x340 do mock) desenhada com os mesmos tracados SVG.
Item {
    id: art
    property real maxWidth: 250
    property real maxHeight: 386
    // Falso quando outra coisa (ex.: a previa do LED) desenha o logo por cima.
    property bool showLogo: true
    readonly property real k: Math.min(maxWidth / 220, maxHeight / 340)
    implicitWidth: 220 * k
    implicitHeight: 340 * k
    Shape {
        width: 220; height: 340
        scale: art.k
        transformOrigin: Item.TopLeft
        ShapePath {
            strokeColor: Theme.border; strokeWidth: 2; fillColor: Theme.raised
            PathSvg { path: "M110 8C60 8 24 44 24 110v120c0 60 36 102 86 102s86-42 86-102V110C196 44 160 8 110 8z" }
        }
        ShapePath {
            strokeColor: Theme.border; strokeWidth: 2; fillColor: "transparent"
            PathSvg { path: "M110 8v118" }
        }
        ShapePath {
            strokeColor: Theme.border; strokeWidth: 2; fillColor: "transparent"
            PathSvg { path: "M26 128Q110 150 194 128" }
        }
        ShapePath {
            strokeColor: Theme.muted; strokeWidth: 1.5; fillColor: Theme.hi
            PathSvg { path: "M110 58h0a10 10 0 0 1 10 10v26a10 10 0 0 1 -10 10h0a10 10 0 0 1 -10 -10v-26a10 10 0 0 1 10 -10z" }
        }
        ShapePath {
            strokeColor: Theme.muted; strokeWidth: 1; fillColor: Theme.hi
            PathSvg { path: "M19 150h4a5 5 0 0 1 5 5v26a5 5 0 0 1 -5 5h-4a5 5 0 0 1 -5 -5v-26a5 5 0 0 1 5 -5z" }
        }
        ShapePath {
            strokeColor: Theme.muted; strokeWidth: 1; fillColor: Theme.hi
            PathSvg { path: "M19 194h4a5 5 0 0 1 5 5v26a5 5 0 0 1 -5 5h-4a5 5 0 0 1 -5 -5v-26a5 5 0 0 1 5 -5z" }
        }
        ShapePath {
            strokeColor: Theme.accent; strokeWidth: art.showLogo ? 3 : 0; fillColor: art.showLogo ? Qt.rgba(0.996, 0.502, 0.098, 0.133) : "transparent"
            PathSvg { path: "M92 236a18 18 0 1 0 36 0a18 18 0 1 0 -36 0z" }
        }
    }
}
