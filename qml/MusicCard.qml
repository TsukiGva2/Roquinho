import QtQuick
import QtQuick.Effects

// "Tocando agora": DVD girando com a capa, faixa, progresso e controles do player.
Item {
    id: mc
    implicitHeight: 124

    readonly property color tone: Theme.music
    readonly property color tone2: Media.color2.valid ? Media.color2 : Qt.darker(Theme.music, 1.6)

    // posição interpolada localmente (o player só informa de vez em quando)
    property double tick: Date.now()
    Timer {
        interval: 250
        repeat: true
        running: mc.visible && Media.playing
        onTriggered: mc.tick = Date.now()
    }
    readonly property double pos: Math.max(0, Math.min(Media.length > 0 ? Media.length : 1e12,
        Media.position + (Media.playing ? tick - Media.positionStamp : 0)))
    property double dragPos: -1
    readonly property double shownPos: dragPos >= 0 ? dragPos : pos

    function fmt(ms) {
        const s = Math.floor(ms / 1000)
        return Math.floor(s / 60) + ":" + Store.pad(s % 60)
    }

    // ---------- fundo: capa desfocada, recortada no cantinho arredondado ----------
    Rectangle {
        anchors.fill: parent
        radius: 20
        color: Theme.card
    }
    Item {
        id: bgSource
        anchors.fill: parent
        visible: false
        layer.enabled: true
        Image {
            // maior que o card para o desfoque não "lavar" as bordas
            anchors.fill: parent
            anchors.margins: -40
            source: Media.art
            fillMode: Image.PreserveAspectCrop
            sourceSize: Qt.size(128, 128)
            opacity: 0.5
            asynchronous: true
        }
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0.0; color: Theme.a(mc.tone, 0.30) }
                GradientStop { position: 0.55; color: Theme.a("#1c1c1e", 0.55) }
                GradientStop { position: 1.0; color: Theme.a("#1c1c1e", 0.85) }
            }
        }
    }
    Rectangle {
        id: bgMask
        anchors.fill: parent
        radius: 20
        visible: false
        layer.enabled: true
    }
    MultiEffect {
        anchors.fill: parent
        source: bgSource
        // sem isso o efeito ganha uma borda extra e a máscara fica deslocada (card "torto")
        autoPaddingEnabled: false
        blurEnabled: true
        blur: 1.0
        blurMax: 48
        saturation: 0.25
        brightness: -0.22
        maskEnabled: true
        maskSource: bgMask
    }

    // visualizador espelhado no rodapé, bem sutil
    Row {
        id: viz
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 1
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: 3
        opacity: Media.playing ? 0.26 : 0
        Behavior on opacity { NumberAnimation { duration: 400 } }
        Repeater {
            model: 46
            Rectangle {
                required property int index
                readonly property int band: Math.min(23, Math.abs(index - 22.5) | 0)
                readonly property real v: Media.playing ? (Media.levels[band] || 0) : 0
                anchors.bottom: parent.bottom
                width: 6
                height: 2 + 22 * v
                radius: 3
                color: mc.tone
            }
        }
        layer.enabled: true
        layer.effect: MultiEffect {
            autoPaddingEnabled: false
            maskEnabled: true
            maskSource: vizMask
        }
    }
    Rectangle {
        id: vizMask
        width: viz.width; height: viz.height
        visible: false
        layer.enabled: true
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0.0; color: "transparent" }
            GradientStop { position: 0.3; color: "white" }
            GradientStop { position: 0.7; color: "white" }
            GradientStop { position: 1.0; color: "transparent" }
        }
    }

    Rectangle {
        anchors.fill: parent
        radius: 20
        color: "transparent"
        border.width: 1
        border.color: Theme.a(mc.tone, 0.22)
    }

    // ---------- DVD ----------
    Disc {
        id: disc
        x: 16
        anchors.verticalCenter: parent.verticalCenter
        size: 92
        scale: discMa.pressed ? 0.95 : 1
        Behavior on scale { NumberAnimation { duration: 140 } }
        MouseArea {
            id: discMa
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: Media.raise()
        }
    }

    // ---------- textos + controles ----------
    Item {
        id: info
        anchors.left: disc.right
        anchors.leftMargin: 18
        anchors.right: parent.right
        anchors.rightMargin: 16
        y: 16
        height: 54

        Row {
            id: label
            spacing: 5
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: "music"
                size: 11
                stroke: 2
                color: mc.tone
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: (Media.playing ? "TOCANDO NO " : "PAUSADO NO ") + Media.player.toUpperCase()
                color: mc.tone
                font.pixelSize: 10
                font.weight: Font.DemiBold
                font.letterSpacing: 0.8
            }
        }

        Text {
            id: titleT
            anchors.top: label.bottom
            anchors.topMargin: 5
            width: info.width - controls.width - 12
            text: Media.title
            color: Theme.text
            font.pixelSize: 15
            font.weight: Font.DemiBold
            elide: Text.ElideRight
        }
        Text {
            anchors.top: titleT.bottom
            anchors.topMargin: 1
            width: titleT.width
            text: Media.artist || Media.album
            color: Theme.text2
            font.pixelSize: 12
            elide: Text.ElideRight
        }

        Row {
            id: controls
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: 6
            spacing: 6
            IconButton {
                anchors.verticalCenter: parent.verticalCenter
                size: 32; iconSize: 15
                icon: "previous"
                color: Theme.text; hoverColor: "white"
                opacity: Media.canPrevious ? 1 : 0.35
                onClicked: Media.previous()
            }
            Item {
                anchors.verticalCenter: parent.verticalCenter
                width: 42; height: 42
                scale: playMa.pressed ? 0.88 : (playMa.containsMouse ? 1.06 : 1)
                Behavior on scale { NumberAnimation { duration: 160; easing.type: Easing.OutBack } }
                Rectangle {
                    anchors.fill: parent
                    radius: width / 2
                    gradient: Gradient {
                        GradientStop { position: 0; color: Qt.lighter(mc.tone, 1.12) }
                        GradientStop { position: 1; color: mc.tone }
                    }
                }
                Rectangle { // brilho da batida
                    anchors.centerIn: parent
                    width: parent.width; height: width; radius: width / 2
                    color: "transparent"
                    border.color: mc.tone
                    border.width: 1.5
                    opacity: Media.beat * 0.7
                    scale: 1 + (1 - Media.beat) * 0.35
                }
                Icon {
                    anchors.centerIn: parent
                    anchors.horizontalCenterOffset: Media.playing ? 0 : 1.5
                    name: Media.playing ? "pauseFill" : "play"
                    size: 18
                    color: "#111113"
                }
                MouseArea { id: playMa; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: Media.playPause() }
            }
            IconButton {
                anchors.verticalCenter: parent.verticalCenter
                size: 32; iconSize: 15
                icon: "next"
                color: Theme.text; hoverColor: "white"
                opacity: Media.canNext ? 1 : 0.35
                onClicked: Media.next()
            }
        }
    }

    // ---------- progresso ----------
    Item {
        id: progress
        anchors.left: info.left
        anchors.right: info.right
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 15
        height: 26
        visible: Media.length > 0

        Rectangle {
            id: track
            y: 4
            width: parent.width
            height: seekMa.containsMouse || seekMa.pressed ? 6 : 4
            radius: height / 2
            color: Theme.a("#ffffff", 0.14)
            Behavior on height { NumberAnimation { duration: 140 } }
            Rectangle {
                width: Math.max(track.height, track.width * Math.min(1, mc.shownPos / Math.max(1, Media.length)))
                height: parent.height
                radius: parent.radius
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0; color: mc.tone2 }
                    GradientStop { position: 1; color: mc.tone }
                }
            }
            Rectangle { // bolinha de arrastar
                x: track.width * Math.min(1, mc.shownPos / Math.max(1, Media.length)) - width / 2
                anchors.verticalCenter: parent.verticalCenter
                width: 12; height: 12; radius: 6
                color: "white"
                scale: (seekMa.containsMouse || seekMa.pressed) && Media.canSeek ? 1 : 0
                Behavior on scale { NumberAnimation { duration: 160; easing.type: Easing.OutBack } }
            }
        }
        MouseArea {
            id: seekMa
            x: 0; y: -4
            width: parent.width; height: 18
            enabled: Media.canSeek
            hoverEnabled: true
            cursorShape: Media.canSeek ? Qt.PointingHandCursor : Qt.ArrowCursor
            function at(mx) { return Math.max(0, Math.min(1, mx / width)) * Media.length }
            onPressed: (mouse) => mc.dragPos = at(mouse.x)
            onPositionChanged: (mouse) => { if (pressed) mc.dragPos = at(mouse.x) }
            onReleased: (mouse) => { Media.seek(at(mouse.x)); mc.dragPos = -1 }
            onCanceled: mc.dragPos = -1
        }
        Text {
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            text: mc.fmt(mc.shownPos)
            color: Theme.text3
            font.pixelSize: 10
            font.features: ({ "tnum": 1 })
        }
        Text {
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            text: "-" + mc.fmt(Math.max(0, Media.length - mc.shownPos))
            color: Theme.text3
            font.pixelSize: 10
            font.features: ({ "tnum": 1 })
        }
    }

    // troca de faixa: o DVD "entra" de novo
    Connections {
        target: Media
        function onTrackChanged() { swapAnim.restart() }
    }
    SequentialAnimation {
        id: swapAnim
        NumberAnimation { target: disc; property: "opacity"; to: 0.3; duration: 120 }
        ParallelAnimation {
            NumberAnimation { target: disc; property: "opacity"; to: 1; duration: 320 }
            NumberAnimation { target: disc; property: "rotation"; from: -40; to: 0; duration: 520; easing.type: Easing.OutBack }
        }
    }
}
