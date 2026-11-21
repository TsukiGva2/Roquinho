import QtQuick

// Modo escondido: só uma bolinha no topo. Clique para voltar.
Item {
    id: dot
    property bool shown: false
    signal clicked()

    width: 34
    height: 34

    property bool appear: false
    onShownChanged: shown ? appearDelay.restart() : (appearDelay.stop(), appear = false)
    Timer { id: appearDelay; interval: 170; onTriggered: dot.appear = true }

    readonly property bool busy: Store.runningCount > 0
    readonly property color tone: busy ? Theme.accent : "#8e8e93"

    Item {
        id: core
        anchors.centerIn: parent
        width: 14
        height: 14
        scale: dot.appear ? (ma.containsMouse ? 1.3 : 1) : 0
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
                running: dot.appear && dot.busy
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

    MouseArea {
        id: ma
        anchors.fill: parent
        enabled: dot.shown
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: dot.clicked()
    }
}
