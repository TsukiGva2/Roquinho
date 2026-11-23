import QtQuick

// Aviso rápido estilo "Dynamic Island" (concluída, adicionada, boas-vindas).
Item {
    id: av
    property var alert: null
    property bool shown: false
    property var last: ({ kind: "info", title: "", sub: "" })
    onAlertChanged: if (alert) { last = alert; pop.restart() }

    width: 400
    height: 62

    opacity: shown ? 1 : 0
    visible: opacity > 0.01
    Behavior on opacity {
        SequentialAnimation {
            PauseAnimation { duration: av.shown ? 110 : 0 }
            NumberAnimation { duration: av.shown ? 240 : 100 }
        }
    }

    readonly property bool isMusic: last.kind === "music"
    readonly property color tone: last.kind === "done" ? Theme.green : isMusic ? Theme.music : Theme.accent

    Pet {
        id: pet
        x: 18
        anchors.verticalCenter: parent.verticalCenter
        size: 40
        kind: Store.petKind
        mood: av.last.kind === "done" || av.isMusic ? "happy" : Store.petMood
        animated: av.visible
    }

    Column {
        anchors.left: pet.right
        anchors.leftMargin: 12
        anchors.right: badge.left
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        spacing: 2
        Text {
            width: parent.width
            text: av.last.title
            color: Theme.text
            font.pixelSize: 14
            font.weight: Font.DemiBold
            elide: Text.ElideRight
        }
        Text {
            width: parent.width
            text: av.last.sub
            color: Theme.text2
            font.pixelSize: 12
            elide: Text.ElideRight
        }
    }

    Rectangle {
        id: badge
        anchors.right: parent.right
        anchors.rightMargin: 18
        anchors.verticalCenter: parent.verticalCenter
        width: av.isMusic ? 40 : 30; height: width; radius: width / 2
        color: av.isMusic ? "transparent" : av.tone
        Disc {
            visible: av.isMusic
            anchors.centerIn: parent
            size: 40
            spinning: Media.playing && av.visible
        }
        Icon {
            visible: !av.isMusic
            anchors.centerIn: parent
            name: av.last.kind === "done" ? "check" : av.last.kind === "added" ? "plus" : "sparkle"
            size: 16
            stroke: 2.2
            color: "white"
        }
    }

    SequentialAnimation {
        id: pop
        PropertyAction { target: badge; property: "scale"; value: 0 }
        PropertyAction { target: badge; property: "rotation"; value: -40 }
        PauseAnimation { duration: 180 }
        ParallelAnimation {
            NumberAnimation { target: badge; property: "scale"; to: 1; duration: 480; easing.type: Easing.OutBack; easing.overshoot: 2.6 }
            NumberAnimation { target: badge; property: "rotation"; to: 0; duration: 420; easing.type: Easing.OutCubic }
        }
    }
}
