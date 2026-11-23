import QtQuick

// Conteúdo do notch fechado: mascote, demanda em foco e cronômetro.
Item {
    id: cv
    property bool hovered: false
    property bool shown: true

    readonly property bool hasFocus: Store.focusModel.count > 0
    readonly property real controlsW: hovered && hasFocus ? 62 : 0
    // ondinhas da música na pontinha direita do notch
    readonly property bool musicOn: Media.active && Media.playing
    property real musicW: musicOn ? 24 : 0
    Behavior on musicW { NumberAnimation { duration: 320; easing.type: Easing.OutCubic } }

    property int cycle: 0
    property int shownIdx: 0
    readonly property int idx: Math.max(0, Math.min(shownIdx, Store.focusModel.count - 1))
    readonly property var task: { Store.revision; return hasFocus ? Store.focusModel.get(idx) : null }
    readonly property string title: task ? task.title : ""
    readonly property bool running: task ? task.running : false
    readonly property double elapsed: { Store.now; return Store.elapsedOf(task) }

    readonly property real petCenter: pet.x + pet.width / 2
    implicitHeight: 38
    implicitWidth: hasFocus
        ? Math.max(250, 14 + pet.width + 9 + Math.min(titleText.implicitWidth, 230) + (badge.visible ? badge.width + 8 : 0)
                         + 22 + controlsW + timerText.implicitWidth + 16 + musicW)
        : (hovered ? 226 : 212) + musicW

    opacity: shown ? 1 : 0
    visible: opacity > 0.01
    Behavior on opacity {
        SequentialAnimation {
            PauseAnimation { duration: cv.shown ? 120 : 0 }
            NumberAnimation { duration: cv.shown ? 240 : 90; easing.type: Easing.OutCubic }
        }
    }
    scale: shown ? 1 : 0.92
    Behavior on scale { NumberAnimation { duration: 320; easing.type: Easing.OutCubic } }

    Timer {
        interval: 5000
        repeat: true
        running: Store.focusModel.count > 1 && !cv.hovered && cv.shown
        onTriggered: cv.cycle = (cv.cycle + 1) % Store.focusModel.count
    }
    onCycleChanged: swap.restart()
    SequentialAnimation {
        id: swap
        ParallelAnimation {
            NumberAnimation { target: swapBox; property: "opacity"; to: 0; duration: 140 }
            NumberAnimation { target: swapT; property: "y"; to: -7; duration: 140; easing.type: Easing.InCubic }
        }
        ScriptAction { script: { cv.shownIdx = cv.cycle; swapT.y = 7 } }
        ParallelAnimation {
            NumberAnimation { target: swapBox; property: "opacity"; to: 1; duration: 220 }
            NumberAnimation { target: swapT; property: "y"; to: 0; duration: 260; easing.type: Easing.OutCubic }
        }
    }

    Pet {
        id: pet
        x: 14
        anchors.verticalCenter: parent.verticalCenter
        size: 26
        kind: Store.petKind
        mood: Store.petMood
        animated: cv.visible
        listen: true
        talking: Store.tipTalking
    }

    // ---- com foco ----
    Item {
        id: swapBox
        visible: cv.hasFocus
        anchors.fill: parent
        transform: Translate { id: swapT }

        Text {
            id: titleText
            x: pet.x + pet.width + 10
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, 230)
            text: cv.title
            color: Theme.text
            font.pixelSize: 13
            font.weight: Font.Medium
            elide: Text.ElideRight
        }
        Rectangle {
            id: badge
            visible: Store.focusModel.count > 1
            anchors.left: titleText.right
            anchors.leftMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            height: 17
            width: badgeText.implicitWidth + 12
            radius: 8.5
            color: Theme.a(Theme.accent, 0.25)
            Text {
                id: badgeText
                anchors.centerIn: parent
                text: (cv.idx + 1) + "/" + Store.focusModel.count
                color: Theme.accentText
                font.pixelSize: 10
                font.weight: Font.DemiBold
                font.features: ({ "tnum": 1 })
            }
        }

        Text {
            id: timerText
            anchors.right: parent.right
            anchors.rightMargin: 16 + cv.musicW
            anchors.verticalCenter: parent.verticalCenter
            text: Store.fmt(cv.elapsed)
            color: cv.running ? Theme.accentText : Theme.text2
            font.pixelSize: 13
            font.weight: Font.DemiBold
            font.features: ({ "tnum": 1 })
            Behavior on color { ColorAnimation { duration: 200 } }
        }
        Rectangle {
            id: liveDot
            anchors.right: timerText.left
            anchors.rightMargin: 7
            anchors.verticalCenter: parent.verticalCenter
            width: 6; height: 6; radius: 3
            color: cv.running ? Theme.accent : Theme.text3
            visible: !cv.hovered
            SequentialAnimation on opacity {
                running: cv.running && cv.visible
                loops: Animation.Infinite
                NumberAnimation { to: 0.25; duration: 900; easing.type: Easing.InOutSine }
                NumberAnimation { to: 1; duration: 900; easing.type: Easing.InOutSine }
                onStopped: liveDot.opacity = 1
            }
        }

        Row {
            id: controls
            anchors.right: timerText.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            opacity: cv.hovered ? 1 : 0
            visible: opacity > 0.01
            scale: cv.hovered ? 1 : 0.6
            Behavior on opacity { NumberAnimation { duration: 180 } }
            Behavior on scale { NumberAnimation { duration: 260; easing.type: Easing.OutBack } }
            IconButton {
                size: 26; iconSize: 13
                icon: cv.running ? "pause" : "play"
                bg: Theme.fill
                onClicked: Store.togglePause(cv.idx)
            }
            IconButton {
                size: 26; iconSize: 14
                icon: "check"
                color: "white"; hoverColor: "white"
                bg: Theme.green; hoverBg: Qt.lighter(Theme.green, 1.15)
                onClicked: Store.complete(cv.idx)
            }
        }
    }

    // ---- sem foco ----
    Item {
        visible: !cv.hasFocus
        anchors.fill: parent
        Text {
            x: pet.x + pet.width + 10
            anchors.verticalCenter: parent.verticalCenter
            text: Store.queueModel.count > 0 ? Store.queueModel.count + " na fila" : "Sem foco agora"
            color: Theme.text2
            font.pixelSize: 13
        }
        Icon {
            anchors.right: parent.right
            anchors.rightMargin: 16 + cv.musicW
            anchors.verticalCenter: parent.verticalCenter
            name: "plus"
            size: 15
            color: cv.hovered ? Theme.text : Theme.text3
        }
    }

    MiniBars {
        anchors.right: parent.right
        anchors.rightMargin: 15
        anchors.verticalCenter: parent.verticalCenter
        bands: [3, 1, 7, 12]
        barWidth: 2.6
        minH: 3
        maxH: 15
        opacity: cv.musicOn ? 1 : 0
        visible: opacity > 0.01
        Behavior on opacity { NumberAnimation { duration: 260 } }
    }
}
