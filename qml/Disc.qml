import QtQuick
import QtQuick.Shapes
import QtQuick.Effects

// DVD com a capa do álbum: gira enquanto toca, com reflexo iridescente fixo (a luz não gira junto).
Item {
    id: d
    property real size: 92
    property string source: Media.art
    property bool spinning: Media.playing
    property color tint: Theme.music
    readonly property bool detailed: size >= 50
    readonly property real r: size / 2
    readonly property real hole: Math.max(2.5, size * 0.075)

    width: size
    height: size

    RectangularShadow {
        anchors.fill: parent
        visible: d.detailed
        radius: d.r
        blur: d.size * 0.2
        spread: -2
        offset.y: d.size * 0.06
        color: Qt.darker(d.tint, 3.2)
        opacity: 0.85
    }

    Image {
        id: art
        source: d.source
        visible: false
        width: d.size
        height: d.size
        sourceSize: Qt.size(256, 256)
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        smooth: true
        mipmap: true
    }

    LinearGradient {
        id: fallback
        x1: 0; y1: 0; x2: d.size; y2: d.size
        GradientStop { position: 0; color: Qt.lighter(d.tint, 1.15) }
        GradientStop { position: 1; color: Qt.darker(d.tint, 2.4) }
    }

    Item {
        id: spinner
        anchors.fill: parent
        NumberAnimation on rotation {
            from: 0; to: 360
            duration: 9000
            loops: Animation.Infinite
            running: d.visible
            paused: !d.spinning
        }

        // sem capa: disco em degradê
        Shape {
            anchors.fill: parent
            visible: art.status !== Image.Ready
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                fillRule: ShapePath.OddEvenFill
                strokeColor: "transparent"
                fillGradient: fallback
                PathAngleArc { centerX: d.r; centerY: d.r; radiusX: d.r; radiusY: d.r; startAngle: 0; sweepAngle: 360 }
                PathAngleArc { centerX: d.r; centerY: d.r; radiusX: d.hole; radiusY: d.hole; startAngle: 0; sweepAngle: 360; moveToStart: true }
            }
        }

        // com capa: a imagem recortada por uma máscara em anel (funciona em qualquer escala de tela)
        Item {
            id: ringMask
            anchors.fill: parent
            visible: false
            layer.enabled: true
            layer.smooth: true
            Shape {
                anchors.fill: parent
                preferredRendererType: Shape.CurveRenderer
                ShapePath {
                    fillRule: ShapePath.OddEvenFill
                    strokeColor: "transparent"
                    fillColor: "white"
                    PathAngleArc { centerX: d.r; centerY: d.r; radiusX: d.r; radiusY: d.r; startAngle: 0; sweepAngle: 360 }
                    PathAngleArc { centerX: d.r; centerY: d.r; radiusX: d.hole; radiusY: d.hole; startAngle: 0; sweepAngle: 360; moveToStart: true }
                }
            }
        }
        MultiEffect {
            anchors.fill: parent
            visible: art.status === Image.Ready
            source: art
            autoPaddingEnabled: false
            maskEnabled: true
            maskSource: ringMask
            maskThresholdMin: 0.5
            maskSpreadAtMin: 1.0
        }

        // nota musical quando não há capa
        Icon {
            visible: art.status !== Image.Ready && d.detailed
            anchors.horizontalCenter: parent.horizontalCenter
            y: d.size * 0.12
            name: "music"
            size: d.size * 0.22
            stroke: 1.6
            color: Theme.a("#ffffff", 0.85)
        }
    }

    // reflexo de DVD (arco-íris suave) + miolo transparente
    Shape {
        anchors.fill: parent
        visible: d.detailed
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            fillRule: ShapePath.OddEvenFill
            strokeColor: "transparent"
            fillGradient: ConicalGradient {
                centerX: d.r; centerY: d.r; angle: 35
                GradientStop { position: 0.00; color: "#00ffffff" }
                GradientStop { position: 0.07; color: "#3378dcff" }
                GradientStop { position: 0.12; color: "#38ffffff" }
                GradientStop { position: 0.17; color: "#2eff8cdc" }
                GradientStop { position: 0.26; color: "#00ffffff" }
                GradientStop { position: 0.50; color: "#00ffffff" }
                GradientStop { position: 0.57; color: "#26fff3a0" }
                GradientStop { position: 0.62; color: "#2effffff" }
                GradientStop { position: 0.67; color: "#268cffc8" }
                GradientStop { position: 0.76; color: "#00ffffff" }
                GradientStop { position: 1.00; color: "#00ffffff" }
            }
            PathAngleArc { centerX: d.r; centerY: d.r; radiusX: d.r; radiusY: d.r; startAngle: 0; sweepAngle: 360 }
            PathAngleArc { centerX: d.r; centerY: d.r; radiusX: d.size * 0.2; radiusY: d.size * 0.2; startAngle: 0; sweepAngle: 360; moveToStart: true }
        }
        // anel de plástico do miolo
        ShapePath {
            fillRule: ShapePath.OddEvenFill
            strokeColor: "#30ffffff"
            strokeWidth: 1
            fillColor: "#59000000"
            PathAngleArc { centerX: d.r; centerY: d.r; radiusX: d.size * 0.2; radiusY: d.size * 0.2; startAngle: 0; sweepAngle: 360 }
            PathAngleArc { centerX: d.r; centerY: d.r; radiusX: d.hole; radiusY: d.hole; startAngle: 0; sweepAngle: 360; moveToStart: true }
        }
        ShapePath {
            strokeColor: "#38ffffff"
            strokeWidth: 1
            fillColor: "transparent"
            PathAngleArc { centerX: d.r; centerY: d.r; radiusX: d.size * 0.13; radiusY: d.size * 0.13; startAngle: 0; sweepAngle: 360 }
        }
        // borda
        ShapePath {
            strokeColor: "#24ffffff"
            strokeWidth: 1
            fillColor: "transparent"
            PathAngleArc { centerX: d.r; centerY: d.r; radiusX: d.r - 0.5; radiusY: d.r - 0.5; startAngle: 0; sweepAngle: 360 }
        }
    }
}
