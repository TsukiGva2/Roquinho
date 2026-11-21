pragma Singleton
import QtQuick
import QtQuick.Shapes

// Paleta inspirada no WhiteSur-dark (macOS Big Sur). O destaque vem do KDE em tempo real.
QtObject {
    readonly property color accent: Store.backend ? Store.backend.accent : "#926ee4"
    readonly property color accentText: Qt.lighter(accent, 1.35)

    readonly property color notch: "#000000"
    readonly property color card: "#1c1c1e"
    readonly property color cardHover: "#232325"
    readonly property color fill: "#2c2c2e"
    readonly property color fill2: "#3a3a3c"
    readonly property color stroke: "#16ffffff"
    readonly property color hover: "#14ffffff"

    readonly property color text: "#f5f5f7"
    readonly property color text2: "#98989d"
    readonly property color text3: "#636366"

    readonly property color green: "#30d158"
    readonly property color red: "#ff453a"
    readonly property color orange: "#ff9f0a"
    readonly property color blue: "#64d2ff"

    // renderizador dos vetores do mascote (a vitrine de demos troca para GeometryRenderer)
    property int shapeRenderer: Shape.CurveRenderer

    function a(c, alpha) { return Qt.rgba(c.r, c.g, c.b, alpha) }
}
