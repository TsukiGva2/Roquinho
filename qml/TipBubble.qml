import QtQuick
import QtQuick.Shapes
import QtQuick.Effects

// Balãozinho do mascote lembrando os atalhos globais.
// "Ok" → volta amanhã ou na próxima vez que o app abrir. "Não mostrar mais" → desliga.
Item {
    id: tb
    property bool shown: false
    property real tailX: 34
    signal okClicked()
    signal neverClicked()
    signal typingChanged(bool typing)

    readonly property real tailH: 9
    readonly property string title: "Psiu! Lembra dos meus atalhos?"
    width: 318
    height: tailH + content.implicitHeight + 30

    // ---- entrada / saída ----
    property real appear: 0
    visible: appear > 0.01
    opacity: Math.min(1, appear * 1.4)
    onShownChanged: {
        if (shown) { outAnim.stop(); inAnim.restart() }
        else { inAnim.stop(); outAnim.restart() }
    }
    SequentialAnimation {
        id: inAnim
        PropertyAction { target: tb; property: "chars"; value: 0 }
        PropertyAction { target: tb; property: "rowsT"; value: 0 }
        PauseAnimation { duration: 120 }
        NumberAnimation { target: tb; property: "appear"; to: 1; duration: 520; easing.type: Easing.OutBack; easing.overshoot: 1.8 }
        ScriptAction { script: tb.typingChanged(true) }
        NumberAnimation { target: tb; property: "chars"; to: tb.title.length; duration: 900 }
        ScriptAction { script: tb.typingChanged(false) }
        NumberAnimation { target: tb; property: "rowsT"; to: 1; duration: 700 }
    }
    NumberAnimation { id: outAnim; target: tb; property: "appear"; to: 0; duration: 200; easing.type: Easing.InCubic }

    property real chars: 0
    property real rowsT: 0

    transform: [
        Scale { origin.x: tb.tailX; origin.y: 0; xScale: 0.4 + 0.6 * tb.appear; yScale: 0.4 + 0.6 * tb.appear },
        Translate { id: floatT }
    ]
    SequentialAnimation {
        running: tb.visible && tb.shown
        loops: Animation.Infinite
        NumberAnimation { target: floatT; property: "y"; to: 2; duration: 1600; easing.type: Easing.InOutSine }
        NumberAnimation { target: floatT; property: "y"; to: 0; duration: 1600; easing.type: Easing.InOutSine }
    }

    // ---- balão ----
    RectangularShadow {
        x: 0; y: tb.tailH
        width: tb.width; height: tb.height - tb.tailH
        radius: 18
        blur: 26
        offset.y: 8
        spread: -2
        color: "#b0000000"
    }
    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            fillColor: "#1c1c1e"
            strokeColor: "#24ffffff"
            strokeWidth: 1
            joinStyle: ShapePath.RoundJoin
            PathSvg {
                readonly property real w: tb.width - 0.5
                readonly property real h: tb.height - 0.5
                readonly property real t: tb.tailH + 0.5
                readonly property real r: 18
                readonly property real x0: tb.tailX
                path: "M " + (r + 0.5) + " " + t
                    + " L " + (x0 - 8) + " " + t
                    + " Q " + (x0 - 2) + " " + t + " " + (x0 - 0.8) + " 1.6"
                    + " Q " + x0 + " 0.2 " + (x0 + 0.8) + " 1.6"
                    + " Q " + (x0 + 2) + " " + t + " " + (x0 + 8) + " " + t
                    + " L " + (w - r) + " " + t
                    + " Q " + w + " " + t + " " + w + " " + (t + r)
                    + " L " + w + " " + (h - r)
                    + " Q " + w + " " + h + " " + (w - r) + " " + h
                    + " L " + (r + 0.5) + " " + h
                    + " Q 0.5 " + h + " 0.5 " + (h - r)
                    + " L 0.5 " + (t + r)
                    + " Q 0.5 " + t + " " + (r + 0.5) + " " + t + " Z"
            }
        }
    }

    // ---- conteúdo ----
    Column {
        id: content
        x: 16
        y: tb.tailH + 14
        width: tb.width - 32
        spacing: 9

        Row {
            spacing: 6
            Text {
                text: tb.title.substring(0, Math.round(tb.chars))
                color: Theme.text
                font.pixelSize: 13
                font.weight: Font.DemiBold
            }
            Rectangle {
                visible: tb.chars < tb.title.length && tb.appear > 0.9
                anchors.verticalCenter: parent.verticalCenter
                width: 2; height: 13; radius: 1
                color: Theme.accent
            }
        }

        Column {
            width: parent.width
            spacing: 5
            Repeater {
                model: [["N", "Nova demanda"], ["F", "Abrir / fechar o painel"], ["H", "Me esconder numa bolinha"], ["D", "Concluir o que está em foco"]]
                Item {
                    id: tipRow
                    required property var modelData
                    required property int index
                    readonly property real t: Math.max(0, Math.min(1, tb.rowsT * 4 - index * 0.9))
                    width: parent.width
                    height: 22
                    opacity: t
                    transform: Translate { x: (1 - tipRow.t) * -10 }
                    Row {
                        id: keys
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 3
                        Repeater {
                            model: ["Meta", "Alt", tipRow.modelData[0]]
                            Rectangle {
                                required property string modelData
                                required property int index
                                height: 19
                                width: Math.max(19, kt.implicitWidth + 10)
                                radius: 5
                                color: index === 2 ? Theme.a(Theme.accent, 0.3) : Theme.fill
                                border.width: 1
                                border.color: index === 2 ? Theme.a(Theme.accent, 0.6) : Theme.stroke
                                Text {
                                    id: kt
                                    anchors.centerIn: parent
                                    text: modelData
                                    color: index === 2 ? Theme.accentText : Theme.text2
                                    font.pixelSize: 10
                                    font.weight: Font.DemiBold
                                }
                            }
                        }
                    }
                    Text {
                        anchors.left: keys.right
                        anchors.leftMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        text: tipRow.modelData[1]
                        color: Theme.text2
                        font.pixelSize: 12
                    }
                }
            }
        }

        Item {
            width: parent.width
            height: 30
            opacity: Math.max(0, Math.min(1, tb.rowsT * 2 - 1))

            Text {
                id: never
                anchors.verticalCenter: parent.verticalCenter
                text: "Não mostrar mais"
                color: neverMa.containsMouse ? Theme.text2 : Theme.text3
                font.pixelSize: 11
                font.underline: neverMa.containsMouse
                MouseArea {
                    id: neverMa
                    anchors.fill: parent
                    anchors.margins: -6
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: tb.neverClicked()
                }
            }
            Rectangle {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                height: 28
                width: okText.implicitWidth + 28
                radius: 14
                color: okMa.containsMouse ? Qt.lighter(Theme.accent, 1.12) : Theme.accent
                scale: okMa.pressed ? 0.92 : 1
                Behavior on scale { NumberAnimation { duration: 120 } }
                Behavior on color { ColorAnimation { duration: 120 } }
                Text {
                    id: okText
                    anchors.centerIn: parent
                    text: "Ok, valeu!"
                    color: "white"
                    font.pixelSize: 12
                    font.weight: Font.DemiBold
                }
                MouseArea {
                    id: okMa
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: tb.okClicked()
                }
            }
        }
    }
}
