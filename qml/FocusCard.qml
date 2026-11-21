import QtQuick
import QtQuick.Shapes

// Cartão de uma demanda em foco: anel do minuto, cronômetro e ações.
Item {
    id: card
    required property int index
    required property string title
    required property double accMs
    required property double startedAt
    required property bool running

    height: 80
    readonly property double elapsed: accMs + (running ? Math.max(0, Store.now - startedAt) : 0)
    readonly property bool longRun: elapsed > 50 * 60000
    property bool completing: false

    function finish() {
        if (completing) return
        completing = true
        finishAnim.restart()
    }

    Rectangle {
        anchors.fill: parent
        radius: 18
        color: hov.hovered ? Theme.cardHover : Theme.card
        border.width: 1
        border.color: card.running ? Theme.a(Theme.accent, 0.38) : Theme.stroke
        Behavior on border.color { ColorAnimation { duration: 240 } }
        Behavior on color { ColorAnimation { duration: 140 } }

        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            opacity: card.running ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: 300 } }
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: Theme.a(Theme.accent, 0.18) }
                GradientStop { position: 0.55; color: "transparent" }
            }
        }
        Rectangle {
            id: flash
            anchors.fill: parent
            radius: parent.radius
            color: Theme.green
            opacity: 0
        }
    }
    HoverHandler { id: hov }

    Item {
        id: ring
        x: 14
        anchors.verticalCenter: parent.verticalCenter
        width: 48; height: 48
        readonly property real frac: (card.elapsed % 60000) / 60000

        Shape {
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: Theme.fill; strokeWidth: 3.5; fillColor: "transparent"
                PathAngleArc { centerX: 24; centerY: 24; radiusX: 21; radiusY: 21; startAngle: 0; sweepAngle: 360 }
            }
            ShapePath {
                strokeColor: card.running ? Theme.accent : Theme.text3
                strokeWidth: 3.5; fillColor: "transparent"; capStyle: ShapePath.RoundCap
                PathAngleArc { centerX: 24; centerY: 24; radiusX: 21; radiusY: 21; startAngle: -90; sweepAngle: Math.max(2, 360 * ring.frac) }
            }
        }
        Rectangle {
            anchors.centerIn: parent
            width: 34; height: 34; radius: 17
            color: ringMa.containsMouse ? Theme.fill : "transparent"
            Behavior on color { ColorAnimation { duration: 140 } }
        }
        Icon {
            anchors.centerIn: parent
            name: card.running ? "pause" : "play"
            size: 16
            stroke: 2.2
            color: ringMa.containsMouse ? Theme.text : Theme.text2
        }
        MouseArea {
            id: ringMa
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: Store.togglePause(card.index)
        }
    }

    Column {
        anchors.left: ring.right
        anchors.leftMargin: 14
        anchors.right: actions.left
        anchors.rightMargin: 10
        anchors.verticalCenter: parent.verticalCenter
        spacing: 1
        Text {
            width: parent.width
            text: card.title
            color: Theme.text
            font.pixelSize: 14
            font.weight: Font.Medium
            elide: Text.ElideRight
        }
        Row {
            spacing: 8
            Text {
                id: timer
                text: Store.fmt(card.elapsed)
                color: card.running ? Theme.accentText : Theme.text2
                font.pixelSize: 23
                font.weight: Font.DemiBold
                font.features: ({ "tnum": 1 })
                Behavior on color { ColorAnimation { duration: 200 } }
            }
            Text {
                anchors.baseline: timer.baseline
                text: !card.running ? "pausado" : card.longRun ? "hora de uma pausa?" : "em andamento"
                color: card.running && card.longRun ? Theme.orange : Theme.text3
                font.pixelSize: 11
            }
        }
    }

    Row {
        id: actions
        anchors.right: parent.right
        anchors.rightMargin: 14
        anchors.verticalCenter: parent.verticalCenter
        spacing: 6
        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            icon: "undo"
            size: 30; iconSize: 15
            opacity: hov.hovered ? 1 : 0.0
            Behavior on opacity { NumberAnimation { duration: 160 } }
            onClicked: Store.unfocus(card.index)
        }
        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            icon: "check"
            size: 38; iconSize: 19
            color: "white"; hoverColor: "white"
            bg: Theme.green; hoverBg: Qt.lighter(Theme.green, 1.12)
            onClicked: card.finish()
        }
    }

    SequentialAnimation {
        id: finishAnim
        NumberAnimation { target: flash; property: "opacity"; to: 0.35; duration: 130 }
        ScriptAction { script: Store.complete(card.index) }
    }
}
