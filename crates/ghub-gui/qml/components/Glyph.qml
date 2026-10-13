import QtQuick
import QtQuick.Shapes
import org.ghublinux.gui

// Icone de tracos (viewBox 24x24 do mock), ate dois caminhos SVG.
Item {
    id: g
    property string d1: ""
    property string d2: ""
    property color color: Theme.fgDim
    property int size: 22
    implicitWidth: size
    implicitHeight: size
    Shape {
        width: 24; height: 24
        scale: g.size / 24
        transformOrigin: Item.TopLeft
        ShapePath {
            strokeColor: g.color; strokeWidth: 1.7; fillColor: "transparent"
            capStyle: ShapePath.RoundCap; joinStyle: ShapePath.RoundJoin
            PathSvg { path: g.d1 }
        }
        ShapePath {
            strokeColor: g.color; strokeWidth: 1.7; fillColor: "transparent"
            capStyle: ShapePath.RoundCap; joinStyle: ShapePath.RoundJoin
            PathSvg { path: g.d2 !== "" ? g.d2 : "M0 0" }
        }
    }
}
