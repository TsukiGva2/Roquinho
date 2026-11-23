import QtQuick
import QtQuick.Shapes

// Ícones vetoriais de traço fino, no espírito dos SF Symbols.
Item {
    id: root
    property string name: "plus"
    property color color: Theme.text
    property real size: 16
    property real stroke: 1.8

    width: size
    height: size

    readonly property var paths: ({
        plus: "M12 5v14M5 12h14",
        check: "M5 12.5l4.5 4.5L19 7.5",
        x: "M6.5 6.5l11 11M17.5 6.5l-11 11",
        play: "M8.5 5.8v12.4a.8.8 0 0 0 1.2.7l9.6-6.2a.8.8 0 0 0 0-1.4L9.7 5.1a.8.8 0 0 0-1.2.7z",
        pause: "M9 6v12M15 6v12",
        chevronUp: "M6 14.5l6-6 6 6",
        chevronDown: "M6 9.5l6 6 6-6",
        eyeoff: "M3.5 3.5l17 17M10.6 10.6a2 2 0 0 0 2.8 2.8M9.9 5.3A9.6 9.6 0 0 1 12 5c5 0 8.6 4.4 9.5 7a12.4 12.4 0 0 1-2.6 3.8M6.6 6.6C4.6 8 3.2 10 2.5 12c.9 2.6 4.5 7 9.5 7 1.7 0 3.2-.5 4.5-1.2",
        target: "M12 3.5a8.5 8.5 0 1 0 0 17a8.5 8.5 0 1 0 0-17M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8M12 11.6a.4.4 0 1 0 0 .8a.4.4 0 1 0 0-.8",
        undo: "M9 14.5L4.5 10 9 5.5M4.5 10h10a5 5 0 0 1 0 10H11",
        sliders: "M4 7.5h9M17 7.5h3M4 16.5h3M11 16.5h9M15 5.5v4M9 14.5v4",
        grip: "M9 6.5h.01M15 6.5h.01M9 12h.01M15 12h.01M9 17.5h.01M15 17.5h.01",
        trash: "M4.5 7h15M10 11v6M14 11v6M6.5 7l.9 12.1a1 1 0 0 0 1 .9h7.2a1 1 0 0 0 1-.9L17.5 7M9.5 7V4.5h5V7",
        power: "M12 3.5v8M6.6 6.6a7.6 7.6 0 1 0 10.8 0",
        restore: "M4.5 12a7.5 7.5 0 1 0 2.2-5.3M4.5 4.5v4h4",
        bolt: "M13 3L5.5 13.5H12L11 21l7.5-10.5H12z",
        music: "M9 18V6.5l10-2V16M9 18a2.5 2.5 0 1 1-5 0a2.5 2.5 0 1 1 5 0M19 16a2.5 2.5 0 1 1-5 0a2.5 2.5 0 1 1 5 0",
        next: "M5.5 6.6v10.8a.8.8 0 0 0 1.2.7l8.3-5.4a.8.8 0 0 0 0-1.4L6.7 5.9a.8.8 0 0 0-1.2.7zM16.8 6h1.6a.6.6 0 0 1 .6.6v10.8a.6.6 0 0 1-.6.6h-1.6a.6.6 0 0 1-.6-.6V6.6a.6.6 0 0 1 .6-.6z",
        previous: "M18.5 6.6v10.8a.8.8 0 0 1-1.2.7L9 12.7a.8.8 0 0 1 0-1.4l8.3-5.4a.8.8 0 0 1 1.2.7zM7.2 6H5.6a.6.6 0 0 0-.6.6v10.8a.6.6 0 0 0 .6.6h1.6a.6.6 0 0 0 .6-.6V6.6a.6.6 0 0 0-.6-.6z",
        pauseFill: "M7.5 5.5h2.4a.9.9 0 0 1 .9.9v11.2a.9.9 0 0 1-.9.9H7.5a.9.9 0 0 1-.9-.9V6.4a.9.9 0 0 1 .9-.9zM14.1 5.5h2.4a.9.9 0 0 1 .9.9v11.2a.9.9 0 0 1-.9.9h-2.4a.9.9 0 0 1-.9-.9V6.4a.9.9 0 0 1 .9-.9z",
        display: "M4.5 5h15a1.5 1.5 0 0 1 1.5 1.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 15.5v-9A1.5 1.5 0 0 1 4.5 5zM9 20.5h6M12 17v3.5",
        star: "M12 3.8l2.5 5.2 5.7.8-4.1 4 1 5.7L12 16.8l-5.1 2.7 1-5.7-4.1-4 5.7-.8z",
        sparkle: "M12 3.5l1.9 5.6 5.6 1.9-5.6 1.9L12 18.5l-1.9-5.6L4.5 11l5.6-1.9z"
    })
    readonly property var filledNames: ({ play: true, bolt: true, sparkle: true, next: true, previous: true, pauseFill: true })
    readonly property bool filled: !!filledNames[name]

    Shape {
        width: 24
        height: 24
        scale: root.size / 24
        transformOrigin: Item.TopLeft
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            strokeColor: root.filled ? "transparent" : root.color
            fillColor: root.filled ? root.color : "transparent"
            strokeWidth: root.name === "grip" ? 3.2 : root.stroke * 24 / root.size
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            PathSvg { path: root.paths[root.name] || "" }
        }
    }
}
