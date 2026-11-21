import QtQuick
import QtQuick.Shapes
import "../../qml"

// Capa do README (gerada pelo gravador de demos).
Window {
    id: w
    width: 960
    height: 380
    visible: false
    color: "#1b1840"

    Component.onCompleted: {
        Store.backend = Backend
        Theme.shapeRenderer = Shape.GeometryRenderer
        go.start()
    }
    Timer {
        id: go
        interval: 500
        onTriggered: { cat.react("celebrate", 99999); ghost.react("love", 99999); slime.react("wink", 99999) }
    }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: "#1b1840" }
            GradientStop { position: 0.6; color: "#2a1f5c" }
            GradientStop { position: 1; color: "#3b1d5e" }
        }
    }
    Rectangle {
        x: -120; y: 160; width: 520; height: 520; radius: 260
        color: Theme.a(Theme.accent, 0.18)
    }
    Rectangle {
        x: 700; y: -260; width: 480; height: 480; radius: 240
        color: Qt.rgba(1, 0.47, 0.67, 0.10)
    }

    // notch decorativo no topo
    NotchShape {
        x: (w.width - width) / 2
        width: 300 + 24; height: 34
        ear: 12; radius: 17
        color: "black"
        Row {
            anchors.centerIn: parent
            anchors.horizontalCenterOffset: -4
            spacing: 10
            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 7; height: 7; radius: 3.5; color: Theme.accent }
            Text { text: "Revisar PR do checkout"; color: Theme.text; font.pixelSize: 12; font.weight: Font.Medium }
            Text { text: "23:16"; color: Theme.accentText; font.pixelSize: 12; font.weight: Font.DemiBold; font.features: ({ "tnum": 1 }) }
        }
    }

    Column {
        x: 64
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: 14
        spacing: 12
        Text {
            text: "Foquinho"
            color: "white"
            font.pixelSize: 64
            font.weight: Font.Black
            font.letterSpacing: -1.5
        }
        Text {
            width: 420
            wrapMode: Text.WordWrap
            text: "O notch de foco que mora no topo da sua tela.\nRegistre, foque, cronometre e conclua — com um mascote torcendo por você."
            color: "#d6d1ee"
            font.pixelSize: 16
            lineHeight: 1.25
        }
        Row {
            spacing: 8
            topPadding: 6
            Repeater {
                model: ["KDE Plasma 6", "Wayland", "Qt 6 · QML", "WhiteSur"]
                Rectangle {
                    required property string modelData
                    height: 26; radius: 13
                    width: chip.implicitWidth + 22
                    color: "#26ffffff"
                    border.width: 1; border.color: "#33ffffff"
                    Text { id: chip; anchors.centerIn: parent; text: modelData; color: "white"; font.pixelSize: 12; font.weight: Font.DemiBold }
                }
            }
        }
    }

    Row {
        anchors.right: parent.right
        anchors.rightMargin: 56
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: 28
        spacing: 18
        Pet { id: ghost; size: 104; kind: "ghost"; mood: "focus"; anchors.bottom: parent.bottom }
        Pet { id: cat; size: 132; kind: "cat"; mood: "focus"; anchors.bottom: parent.bottom }
        Pet { id: slime; size: 104; kind: "slime"; mood: "focus"; anchors.bottom: parent.bottom }
    }
}
