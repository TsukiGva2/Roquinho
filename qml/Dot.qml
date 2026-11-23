import QtQuick

// Modo escondido: só uma bolinha no topo. Clique para voltar.
// Com música tocando, vira uma bolinha preta com ondas cilíndricas na cor da capa,
// pulsando na batida. Clique direito pausa/retoma a música.
Item {
    id: dot
    property bool shown: false
    signal clicked()

    readonly property bool music: Media.active && Media.playing
    width: music ? 40 : 34
    height: width

    property bool appear: false
    onShownChanged: shown ? appearDelay.restart() : (appearDelay.stop(), appear = false)
    Timer { id: appearDelay; interval: 170; onTriggered: dot.appear = true }

    readonly property bool busy: Store.runningCount > 0
    readonly property color tone: busy ? Theme.accent : "#8e8e93"

    // ---------- bolinha normal ----------
    Item {
        id: core
        anchors.centerIn: parent
        width: 14
        height: 14
        scale: dot.appear && !dot.music ? (ma.containsMouse ? 1.3 : 1) : 0
        Behavior on scale { NumberAnimation { duration: 420; easing.type: Easing.OutBack; easing.overshoot: 2.4 } }

        Rectangle {
            id: pulse
            anchors.centerIn: parent
            width: 14; height: 14; radius: 7
            color: "transparent"
            border.color: dot.tone
            border.width: 1.5
            opacity: 0
            visible: dot.busy
            SequentialAnimation {
                running: dot.appear && dot.busy && !dot.music
                loops: Animation.Infinite
                ParallelAnimation {
                    NumberAnimation { target: pulse; property: "scale"; from: 1; to: 2.3; duration: 1500; easing.type: Easing.OutCubic }
                    NumberAnimation { target: pulse; property: "opacity"; from: 0.8; to: 0; duration: 1500 }
                }
                PauseAnimation { duration: 600 }
            }
        }
        Rectangle {
            anchors.fill: parent
            radius: width / 2
            color: dot.tone
            border.color: "#40ffffff"
            border.width: 1
            Behavior on color { ColorAnimation { duration: 300 } }
        }
        Rectangle {
            x: 3; y: 2.5; width: 4; height: 3; radius: 2
            color: "white"; opacity: 0.45
        }
    }

    // ---------- bolinha musical ----------
    Item {
        id: musicBall
        anchors.centerIn: parent
        width: 30
        height: 30
        scale: dot.appear && dot.music ? (ma.containsMouse ? 1.14 : 1) + Media.beat * 0.06 : 0
        Behavior on scale { NumberAnimation { duration: 380; easing.type: Easing.OutBack; easing.overshoot: 2 } }
        visible: scale > 0.01

        // halo na cor da música, respira com o volume
        Rectangle {
            anchors.centerIn: parent
            width: parent.width; height: width; radius: width / 2
            color: Theme.music
            opacity: 0.16 + Media.level * 0.5
            scale: 1.08 + Media.level * 0.32
        }
        // anel que estoura na batida
        Rectangle {
            anchors.centerIn: parent
            width: parent.width; height: width; radius: width / 2
            color: "transparent"
            border.color: Theme.music
            border.width: 1.5
            opacity: Media.beat * 0.85
            scale: 1 + (1 - Media.beat) * 0.45
        }
        Rectangle {
            anchors.fill: parent
            radius: width / 2
            color: "#000000"
            border.color: Theme.a(Theme.music, 0.45)
            border.width: 1
        }
        MiniBars {
            anchors.centerIn: parent
            bands: [10, 4, 1, 6, 14]
            barWidth: 2.6
            spacing: 1.8
            minH: 3
            maxH: 16
        }
    }

    MouseArea {
        id: ma
        anchors.fill: parent
        enabled: dot.shown
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: Qt.PointingHandCursor
        onClicked: (mouse) => {
            if (mouse.button === Qt.RightButton && Media.active) Media.playPause()
            else dot.clicked()
        }
    }
}
