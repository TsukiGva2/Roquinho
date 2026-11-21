import QtQuick
import QtQuick.Shapes
import "../../qml"

// Vitrine dos mascotes passeando pelas expressões (usada pelo gravador de demos).
Window {
    id: w
    width: 760
    height: 250
    color: "#121214"
    visible: false

    property int step: -1
    readonly property var exprs: [
        ["focado", "focus", ""], ["comemorando", "focus", "celebrate"], ["apaixonado", "focus", "love"],
        ["assustado", "focus", "surprised"], ["acenando", "focus", "wave"], ["piscadinha", "focus", "wink"],
        ["curioso", "idle", "curious"], ["cantarolando", "idle", "hum"], ["cansado", "tired", ""],
        ["dormindo", "sleep", ""]
    ]

    Component.onCompleted: { Store.backend = Backend; Theme.shapeRenderer = Shape.GeometryRenderer; ticker.start() }

    Timer {
        id: ticker
        interval: 950
        repeat: true
        triggeredOnStart: true
        onTriggered: {
            w.step = (w.step + 1) % w.exprs.length
            const e = w.exprs[w.step]
            for (const p of [cat, ghost, slime]) {
                p.reaction = ""
                p.mood = e[1]
                if (e[2]) p.react(e[2], 940)
            }
            labelAnim.restart()
        }
    }

    Row {
        anchors.horizontalCenter: parent.horizontalCenter
        y: 34
        spacing: 90
        Repeater {
            model: [["cat", "Gato"], ["ghost", "Fantasma"], ["slime", "Slime"]]
            Column {
                required property var modelData
                required property int index
                spacing: 6
                Pet {
                    id: p
                    size: 120
                    kind: modelData[0]
                    mood: "focus"
                    Component.onCompleted: {
                        if (index === 0) w.cat = p
                        else if (index === 1) w.ghost = p
                        else w.slime = p
                    }
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: modelData[1]
                    color: Theme.text3
                    font.pixelSize: 12
                    font.weight: Font.DemiBold
                    font.letterSpacing: 1
                }
            }
        }
    }

    property Item cat
    property Item ghost
    property Item slime

    Text {
        id: label
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 18
        text: w.step >= 0 ? w.exprs[w.step][0] : ""
        color: Theme.accentText
        font.pixelSize: 16
        font.weight: Font.DemiBold
    }
    SequentialAnimation {
        id: labelAnim
        PropertyAction { target: label; property: "opacity"; value: 0 }
        PropertyAction { target: label; property: "scale"; value: 0.8 }
        ParallelAnimation {
            NumberAnimation { target: label; property: "opacity"; to: 1; duration: 220 }
            NumberAnimation { target: label; property: "scale"; to: 1; duration: 320; easing.type: Easing.OutBack }
        }
    }
}
