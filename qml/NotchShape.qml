import QtQuick
import QtQuick.Shapes

// Silhueta do notch: pendurado na borda superior, cantos de baixo arredondados
// e "orelhas" côncavas que derretem na borda da tela, como no macOS.
Shape {
    id: s
    property real ear: 10
    property real radius: 18
    property color color: "#000000"

    readonly property real bw: Math.max(0, width - 2 * ear)
    readonly property real r: Math.max(0, Math.min(radius, height / 2, bw / 2))
    readonly property real earY: Math.max(0, Math.min(ear, height - r))

    preferredRendererType: Theme.shapeRenderer

    ShapePath {
        fillColor: s.color
        strokeColor: "transparent"
        strokeWidth: 0
        startX: 0
        startY: 0
        PathQuad { x: s.ear; y: s.earY; controlX: s.ear; controlY: 0 }
        PathLine { x: s.ear; y: s.height - s.r }
        PathArc { x: s.ear + s.r; y: s.height; radiusX: s.r; radiusY: s.r; direction: PathArc.Counterclockwise }
        PathLine { x: s.width - s.ear - s.r; y: s.height }
        PathArc { x: s.width - s.ear; y: s.height - s.r; radiusX: s.r; radiusY: s.r; direction: PathArc.Counterclockwise }
        PathLine { x: s.width - s.ear; y: s.earY }
        PathQuad { x: s.width; y: 0; controlX: s.width - s.ear; controlY: 0 }
        PathLine { x: 0; y: 0 }
    }
}
