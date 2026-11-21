import QtQuick
import QtQuick.Shapes

// Mascote em vetor, desenhado num grid de 40x40 e escalado.
//
// mood (estado contínuo): sleep | idle | focus | tired | happy
// reações (temporárias, via react()): celebrate | wave | surprised | love | wink | ready
//                                     yawn | curious | hum | sigh
// Extras: talking (balão falando), interactive (segue o cursor e aceita carinho),
//         listen (reage aos eventos do Store).
Item {
    id: pet
    property string kind: "cat"      // cat | ghost | slime
    property string mood: "focus"
    property real size: 32
    property bool animated: true
    property bool listen: false
    property bool interactive: false
    property bool talking: false
    signal petted()

    width: size
    height: size

    readonly property color body: kind === "cat" ? "#F7B267" : kind === "ghost" ? "#EEEEF4" : Qt.lighter(Theme.accent, 1.22)
    readonly property color shade: kind === "cat" ? "#E38B3F" : kind === "ghost" ? "#C9C9D6" : Theme.accent
    readonly property color ink: "#1d1d1f"
    readonly property color mouthFill: "#7A2533"
    readonly property bool active: animated && visible

    // ---------- expressão atual ----------
    property string reaction: ""
    readonly property bool hoveredByUser: interactive && petHover.hovered
    readonly property string expr: talking ? "talk"
        : reaction !== "" ? reaction
        : hoveredByUser ? "content"
        : mood

    readonly property string eyes: ({
        sleep: "closed", idle: "open", focus: "open", tired: "half", happy: "happy",
        celebrate: "star", wave: "open", surprised: "wide", love: "heart", wink: "wink",
        ready: "open", talk: "open", content: "happy", yawn: "closed", curious: "open",
        hum: "closed", sigh: "half"
    })[expr] || "open"

    readonly property string mouth: ({
        sleep: "small", idle: "smile", focus: kind === "cat" ? "cat" : "smile", tired: "wavy",
        happy: "grin", celebrate: "grin", wave: "grin", surprised: "o", love: "tongue",
        wink: "tongue", ready: "grin", talk: "talk", content: kind === "cat" ? "cat" : "smile",
        yawn: "yawn", curious: "small", hum: "o", sigh: "flat"
    })[expr] || "smile"

    readonly property string brows: ({
        focus: "determined", ready: "determined", tired: "worried", sigh: "worried",
        surprised: "raised", curious: "tilt", talk: "raised", wave: "raised"
    })[expr] || "none"

    // ---------- parâmetros animáveis ----------
    property real blink: 1
    property real breathe: 1
    property real bob: 0
    property real jump: 0
    property real squash: 0      // >0 achata, <0 estica
    property real tilt: 0
    property real lookX: 0
    property real lookY: 0
    property real faceDy: 0
    property real earL: 0
    property real earR: 0
    property real tailA: 0
    property real handRY: 0
    property real handRRot: 0
    property real handLY: 0
    property real talkH: 1
    property real heartsT: 1
    property real sparkT: 1
    property real popS: 0        // "!" "?" e raio
    property real notesT: 0

    Behavior on tilt { NumberAnimation { duration: 320; easing.type: Easing.OutBack } }
    Behavior on lookX { NumberAnimation { duration: 240; easing.type: Easing.OutCubic } }
    Behavior on lookY { NumberAnimation { duration: 240; easing.type: Easing.OutCubic } }

    function react(name, ms) {
        if (!animated) return
        reaction = name
        reactionTimer.interval = ms || 1800
        reactionTimer.restart()
        play(name)
    }
    Timer { id: reactionTimer; onTriggered: pet.reaction = "" }
    onReactionChanged: if (reaction === "") { tilt = 0; popS = 0 }

    onMoodChanged: if (mood === "happy" && reaction === "") play("happy")

    Connections {
        target: Store
        enabled: pet.listen
        function onPetEvent(name) {
            const ms = { celebrate: 3000, wave: 1700, surprised: 1300, ready: 1500, love: 1800 }[name] || 1600
            pet.react(name, ms)
        }
    }

    function play(name) {
        switch (name) {
        case "happy":
        case "celebrate": jumpAnim.restart(); heartsAnim.restart(); sparkAnim.restart(); break
        case "wave": waveAnim.restart(); break
        case "surprised": hopAnim.restart(); popAnim.restart(); break
        case "love": squishAnim.restart(); heartsAnim.restart(); break
        case "wink": sparkAnim.restart(); tilt = -6; break
        case "ready": fistAnim.restart(); popAnim.restart(); break
        case "yawn": stretchAnim.restart(); break
        case "curious": tilt = 9; popAnim.restart(); break
        case "hum": notesAnim.restart(); swayAnim.restart(); break
        case "sigh": sighAnim.restart(); break
        }
    }

    // ---------- animações contínuas ----------
    SequentialAnimation {
        running: pet.active
        loops: Animation.Infinite
        NumberAnimation {
            target: pet; property: "breathe"; to: pet.kind === "slime" ? 0.93 : 0.965
            duration: pet.mood === "sleep" ? 1700 : pet.mood === "tired" ? 1500 : 1150; easing.type: Easing.InOutSine
        }
        NumberAnimation {
            target: pet; property: "breathe"; to: 1
            duration: pet.mood === "sleep" ? 1700 : pet.mood === "tired" ? 1500 : 1150; easing.type: Easing.InOutSine
        }
    }

    SequentialAnimation {
        running: pet.active && pet.kind === "ghost"
        loops: Animation.Infinite
        NumberAnimation { target: pet; property: "bob"; to: -2.6; duration: 1400; easing.type: Easing.InOutSine }
        NumberAnimation { target: pet; property: "bob"; to: 0; duration: 1400; easing.type: Easing.InOutSine }
        onStopped: pet.bob = 0
    }

    SequentialAnimation {
        running: pet.active && pet.kind === "cat"
        loops: Animation.Infinite
        NumberAnimation { target: pet; property: "tailA"; to: pet.mood === "sleep" ? 3 : 9; duration: pet.mood === "sleep" ? 2200 : 1100; easing.type: Easing.InOutSine }
        NumberAnimation { target: pet; property: "tailA"; to: pet.mood === "sleep" ? -3 : -7; duration: pet.mood === "sleep" ? 2200 : 1100; easing.type: Easing.InOutSine }
    }

    SequentialAnimation {
        running: pet.active && pet.talking
        loops: Animation.Infinite
        NumberAnimation { target: pet; property: "talkH"; to: 3.6; duration: 90 }
        NumberAnimation { target: pet; property: "talkH"; to: 1.4; duration: 80 }
        NumberAnimation { target: pet; property: "talkH"; to: 3.0; duration: 110 }
        NumberAnimation { target: pet; property: "talkH"; to: 0.9; duration: 120 }
        NumberAnimation { target: pet; property: "talkH"; to: 2.4; duration: 90 }
        NumberAnimation { target: pet; property: "talkH"; to: 1; duration: 160 }
        onStopped: pet.talkH = 1
    }

    Timer {
        running: pet.active && pet.eyes !== "closed"
        repeat: true
        interval: 3200
        onTriggered: { interval = 2400 + Math.random() * 3200; blinkAnim.restart() }
    }
    SequentialAnimation {
        id: blinkAnim
        NumberAnimation { target: pet; property: "blink"; to: 0.1; duration: 70 }
        NumberAnimation { target: pet; property: "blink"; to: 1; duration: 110 }
    }
    SequentialAnimation {
        id: doubleBlink
        NumberAnimation { target: pet; property: "blink"; to: 0.1; duration: 60 }
        NumberAnimation { target: pet; property: "blink"; to: 1; duration: 80 }
        PauseAnimation { duration: 90 }
        NumberAnimation { target: pet; property: "blink"; to: 0.1; duration: 60 }
        NumberAnimation { target: pet; property: "blink"; to: 1; duration: 100 }
    }

    // pequenas manias aleatórias para ele parecer vivo
    Timer {
        running: pet.active && pet.reaction === "" && !pet.talking && !pet.hoveredByUser
        repeat: true
        interval: 5000
        onTriggered: { interval = 4200 + Math.random() * 6500; pet.fidget() }
    }
    function fidget() {
        const pools = {
            sleep: ["yawn", "ear", "turn", "none"],
            idle: ["look", "hum", "curious", "hop", "yawn", "tail", "blink2"],
            tired: ["sigh", "look", "yawn"],
            focus: ["look", "nod", "ear", "tail", "blink2", "wink", "nod"],
            happy: ["hop"]
        }
        const pool = pools[mood] || ["look"]
        const f = pool[Math.floor(Math.random() * pool.length)]
        switch (f) {
        case "yawn": react("yawn", 1500); break
        case "hum": react("hum", 2600); break
        case "curious": react("curious", 1700); break
        case "wink": react("wink", 900); break
        case "sigh": react("sigh", 1800); break
        case "look": lookAnim.restart(); break
        case "nod": nodAnim.restart(); break
        case "ear": earAnim.restart(); break
        case "tail": tailFlick.restart(); break
        case "hop": hopAnim.restart(); break
        case "blink2": doubleBlink.restart(); break
        case "turn": turnAnim.restart(); break
        }
    }

    // ---------- animações de reação ----------
    SequentialAnimation {
        id: jumpAnim
        NumberAnimation { target: pet; property: "squash"; to: 0.16; duration: 90; easing.type: Easing.OutQuad }
        ParallelAnimation {
            NumberAnimation { target: pet; property: "jump"; to: -8; duration: 180; easing.type: Easing.OutQuad }
            NumberAnimation { target: pet; property: "squash"; to: -0.1; duration: 120 }
        }
        ParallelAnimation {
            NumberAnimation { target: pet; property: "jump"; to: 0; duration: 170; easing.type: Easing.InQuad }
            NumberAnimation { target: pet; property: "squash"; to: 0; duration: 170 }
        }
        NumberAnimation { target: pet; property: "squash"; to: 0.18; duration: 70 }
        NumberAnimation { target: pet; property: "squash"; to: 0; duration: 160; easing.type: Easing.OutBack }
        NumberAnimation { target: pet; property: "jump"; to: -4.5; duration: 150; easing.type: Easing.OutQuad }
        NumberAnimation { target: pet; property: "jump"; to: 0; duration: 150; easing.type: Easing.InQuad }
        NumberAnimation { target: pet; property: "squash"; to: 0.1; duration: 60 }
        NumberAnimation { target: pet; property: "squash"; to: 0; duration: 180; easing.type: Easing.OutBack }
    }
    SequentialAnimation {
        id: hopAnim
        NumberAnimation { target: pet; property: "squash"; to: 0.1; duration: 80 }
        ParallelAnimation {
            NumberAnimation { target: pet; property: "jump"; to: -4; duration: 140; easing.type: Easing.OutQuad }
            NumberAnimation { target: pet; property: "squash"; to: -0.06; duration: 100 }
        }
        ParallelAnimation {
            NumberAnimation { target: pet; property: "jump"; to: 0; duration: 140; easing.type: Easing.InQuad }
            NumberAnimation { target: pet; property: "squash"; to: 0; duration: 140 }
        }
        NumberAnimation { target: pet; property: "squash"; to: 0.1; duration: 60 }
        NumberAnimation { target: pet; property: "squash"; to: 0; duration: 160; easing.type: Easing.OutBack }
    }
    SequentialAnimation {
        id: squishAnim
        NumberAnimation { target: pet; property: "squash"; to: 0.2; duration: 150; easing.type: Easing.OutQuad }
        NumberAnimation { target: pet; property: "squash"; to: 0; duration: 700; easing.type: Easing.OutElastic; easing.amplitude: 1.2; easing.period: 0.35 }
    }
    SequentialAnimation {
        id: waveAnim
        NumberAnimation { target: pet; property: "handRY"; to: -9; duration: 200; easing.type: Easing.OutBack }
        SequentialAnimation {
            loops: 3
            NumberAnimation { target: pet; property: "handRRot"; to: -30; duration: 130; easing.type: Easing.InOutSine }
            NumberAnimation { target: pet; property: "handRRot"; to: 18; duration: 130; easing.type: Easing.InOutSine }
        }
        NumberAnimation { target: pet; property: "handRRot"; to: 0; duration: 120 }
        NumberAnimation { target: pet; property: "handRY"; to: 0; duration: 220; easing.type: Easing.InOutQuad }
    }
    SequentialAnimation {
        id: fistAnim
        loops: 2
        ParallelAnimation {
            NumberAnimation { target: pet; property: "handRY"; to: -10; duration: 130; easing.type: Easing.OutQuad }
            NumberAnimation { target: pet; property: "handLY"; to: -3; duration: 130 }
            NumberAnimation { target: pet; property: "jump"; to: -2.5; duration: 130; easing.type: Easing.OutQuad }
        }
        ParallelAnimation {
            NumberAnimation { target: pet; property: "handRY"; to: -4; duration: 150; easing.type: Easing.InQuad }
            NumberAnimation { target: pet; property: "handLY"; to: 0; duration: 150 }
            NumberAnimation { target: pet; property: "jump"; to: 0; duration: 150; easing.type: Easing.InQuad }
        }
        onFinished: pet.handRY = 0
    }
    SequentialAnimation {
        id: stretchAnim
        NumberAnimation { target: pet; property: "squash"; to: -0.1; duration: 600; easing.type: Easing.OutSine }
        PauseAnimation { duration: 350 }
        NumberAnimation { target: pet; property: "squash"; to: 0; duration: 450; easing.type: Easing.OutBack }
    }
    SequentialAnimation {
        id: sighAnim
        NumberAnimation { target: pet; property: "squash"; to: -0.05; duration: 500; easing.type: Easing.InOutSine }
        NumberAnimation { target: pet; property: "squash"; to: 0.1; duration: 700; easing.type: Easing.InOutSine }
        NumberAnimation { target: pet; property: "squash"; to: 0; duration: 500; easing.type: Easing.InOutSine }
    }
    SequentialAnimation {
        id: lookAnim
        ScriptAction { script: { pet.lookX = -1.6; pet.lookY = -0.3 } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: { pet.lookX = 1.6; pet.lookY = 0.2 } }
        PauseAnimation { duration: 700 }
        ScriptAction { script: { pet.lookX = 0; pet.lookY = 0 } }
    }
    SequentialAnimation {
        id: nodAnim
        loops: 2
        NumberAnimation { target: pet; property: "faceDy"; to: 1.3; duration: 140; easing.type: Easing.OutQuad }
        NumberAnimation { target: pet; property: "faceDy"; to: 0; duration: 180; easing.type: Easing.InOutQuad }
    }
    SequentialAnimation {
        id: earAnim
        NumberAnimation { target: pet; property: "earR"; to: -16; duration: 70 }
        NumberAnimation { target: pet; property: "earR"; to: 0; duration: 110 }
        NumberAnimation { target: pet; property: "earR"; to: -10; duration: 70 }
        NumberAnimation { target: pet; property: "earR"; to: 0; duration: 140 }
        NumberAnimation { target: pet; property: "earL"; to: 12; duration: 80 }
        NumberAnimation { target: pet; property: "earL"; to: 0; duration: 160 }
    }
    SequentialAnimation {
        id: tailFlick
        NumberAnimation { target: pet; property: "tailA"; to: 24; duration: 120; easing.type: Easing.OutQuad }
        NumberAnimation { target: pet; property: "tailA"; to: -14; duration: 160 }
        NumberAnimation { target: pet; property: "tailA"; to: 0; duration: 200 }
    }
    SequentialAnimation {
        id: turnAnim
        ScriptAction { script: pet.tilt = 6 }
        PauseAnimation { duration: 1400 }
        ScriptAction { script: pet.tilt = 0 }
    }
    SequentialAnimation {
        id: swayAnim
        loops: 2
        ScriptAction { script: pet.tilt = -5 }
        PauseAnimation { duration: 600 }
        ScriptAction { script: pet.tilt = 5 }
        PauseAnimation { duration: 600 }
        onFinished: pet.tilt = 0
    }
    NumberAnimation { id: heartsAnim; target: pet; property: "heartsT"; from: 0; to: 1; duration: 1500; easing.type: Easing.OutCubic }
    NumberAnimation { id: sparkAnim; target: pet; property: "sparkT"; from: 0; to: 1; duration: 1100 }
    NumberAnimation { id: popAnim; target: pet; property: "popS"; from: 0; to: 1; duration: 420; easing.type: Easing.OutBack; easing.overshoot: 3 }
    NumberAnimation { id: notesAnim; target: pet; property: "notesT"; from: 0; to: 1; duration: 2400 }

    // ---------- interação ----------
    HoverHandler {
        id: petHover
        enabled: pet.interactive
        cursorShape: Qt.PointingHandCursor
        onPointChanged: {
            if (!hovered) return
            pet.lookX = Math.max(-1.8, Math.min(1.8, (point.position.x / pet.width - 0.5) * 4))
            pet.lookY = Math.max(-1.2, Math.min(1.2, (point.position.y / pet.height - 0.5) * 3))
        }
        onHoveredChanged: if (!hovered) { pet.lookX = 0; pet.lookY = 0 }
    }
    TapHandler {
        enabled: pet.interactive
        onTapped: { pet.react("love", 1900); pet.petted() }
    }

    // ---------- desenho ----------
    Item {
        id: stage
        width: 40
        height: 40
        scale: pet.size / 40
        transformOrigin: Item.TopLeft

        // sombra no chão
        Rectangle {
            visible: pet.size >= 36
            x: 20 - width / 2
            y: 36.4
            width: (pet.kind === "ghost" ? 16 : 22) * (1 + (pet.jump + pet.bob) / 22)
            height: 2.6
            radius: 1.3
            color: "black"
            opacity: (pet.kind === "ghost" ? 0.22 : 0.32) * (1 + (pet.jump + pet.bob) / 14)
        }

        Item {
            id: figure
            width: 40
            height: 40
            y: pet.bob + pet.jump
            transform: [
                Scale {
                    origin.x: 20; origin.y: 37
                    yScale: pet.breathe * (1 - pet.squash)
                    xScale: (1 + (1 - pet.breathe) * (pet.kind === "slime" ? 0.9 : 0.5)) * (1 + pet.squash * 0.7)
                },
                Rotation { origin.x: 20; origin.y: 37; angle: pet.tilt }
            ]

            // ---- cauda (gato) ----
            Shape {
                visible: pet.kind === "cat"
                anchors.fill: parent
                preferredRendererType: Theme.shapeRenderer
                transform: Rotation { origin.x: 30; origin.y: 32; angle: pet.tailA }
                ShapePath {
                    strokeColor: pet.body; strokeWidth: 3.4; fillColor: "transparent"
                    capStyle: ShapePath.RoundCap
                    PathSvg { path: "M29 32.5Q37.5 33 37 24.5Q36.8 20.8 34.6 19.6" }
                }
                ShapePath {
                    strokeColor: pet.shade; strokeWidth: 3.4; fillColor: "transparent"
                    capStyle: ShapePath.RoundCap
                    PathSvg { path: "M36.2 21.6Q35.8 20.2 34.6 19.6" }
                }
            }

            // ---- orelhas (gato) ----
            Shape {
                visible: pet.kind === "cat"
                anchors.fill: parent
                preferredRendererType: Theme.shapeRenderer
                transform: Rotation { origin.x: 12; origin.y: 15; angle: pet.earL }
                ShapePath { fillColor: pet.body; strokeColor: "transparent"; PathSvg { path: "M7 19.5L9.4 5.8Q9.9 4.6 10.9 5.4L19 12.5Z" } }
                ShapePath { fillColor: "#F49A9A"; strokeColor: "transparent"; PathSvg { path: "M10.4 14.5L11.3 8.8L15.6 12.6Z" } }
            }
            Shape {
                visible: pet.kind === "cat"
                anchors.fill: parent
                preferredRendererType: Theme.shapeRenderer
                transform: Rotation { origin.x: 28; origin.y: 15; angle: pet.earR }
                ShapePath { fillColor: pet.body; strokeColor: "transparent"; PathSvg { path: "M33 19.5L30.6 5.8Q30.1 4.6 29.1 5.4L21 12.5Z" } }
                ShapePath { fillColor: "#F49A9A"; strokeColor: "transparent"; PathSvg { path: "M29.6 14.5L28.7 8.8L24.4 12.6Z" } }
            }

            // ---- corpo ----
            Item {
                visible: pet.kind === "cat"
                anchors.fill: parent
                Rectangle { x: 5; y: 10.5; width: 30; height: 25.5; radius: 12.75; color: pet.body }
                Rectangle { x: 18.7; y: 10.8; width: 2.6; height: 4.6; radius: 1.3; color: pet.shade }
                Rectangle { x: 15.1; y: 11.6; width: 2; height: 3.2; radius: 1; color: pet.shade; opacity: 0.8 }
                Rectangle { x: 22.9; y: 11.6; width: 2; height: 3.2; radius: 1; color: pet.shade; opacity: 0.8 }
                Rectangle { x: 10.2; y: 33.6; width: 6; height: 3.4; radius: 1.7; color: Qt.darker(pet.body, 1.06) }
                Rectangle { x: 23.8; y: 33.6; width: 6; height: 3.4; radius: 1.7; color: Qt.darker(pet.body, 1.06) }
            }
            Shape {
                visible: pet.kind === "ghost"
                anchors.fill: parent
                preferredRendererType: Theme.shapeRenderer
                ShapePath {
                    fillColor: pet.body; strokeColor: "transparent"
                    PathSvg { path: "M7 35V19.5A13 13 0 0 1 33 19.5V35Q30.8 32.2 28.6 35Q26.4 37.8 24.2 35Q22 32.2 19.8 35Q17.6 37.8 15.4 35Q13.2 32.2 11 35Q9 37.6 7 35Z" }
                }
            }
            Item {
                visible: pet.kind === "slime"
                anchors.fill: parent
                Shape {
                    anchors.fill: parent
                    preferredRendererType: Theme.shapeRenderer
                    ShapePath {
                        fillColor: pet.body; strokeColor: "transparent"
                        PathSvg { path: "M4 35.5C4 22 11.5 12.5 20 12.5C28.5 12.5 36 22 36 35.5Q36 36.6 34.9 36.6H5.1Q4 36.6 4 35.5Z" }
                    }
                }
                Rectangle { x: 10; y: 17.5; width: 5.5; height: 2.8; radius: 1.4; rotation: -38; color: "white"; opacity: 0.55 }
                Rectangle { x: 16.5; y: 15.4; width: 1.8; height: 1.8; radius: 0.9; color: "white"; opacity: 0.45 }
            }

            // ---- rosto ----
            Item {
                id: face
                width: 40
                height: 40
                x: pet.lookX * 0.6
                y: (pet.kind === "slime" ? 4 : pet.kind === "ghost" ? -0.5 : 1) + pet.faceDy + pet.lookY * 0.4

                // sobrancelhas
                Shape {
                    visible: pet.brows !== "none"
                    anchors.fill: parent
                    preferredRendererType: Theme.shapeRenderer
                    ShapePath {
                        strokeColor: pet.ink; strokeWidth: 1.25; fillColor: "transparent"
                        capStyle: ShapePath.RoundCap
                        PathSvg {
                            path: pet.brows === "determined" ? "M11.6 14.4L16.2 15.9M28.4 14.4L23.8 15.9"
                                : pet.brows === "worried" ? "M11.8 15.9L16.2 14.5M28.2 15.9L23.8 14.5"
                                : pet.brows === "tilt" ? "M11.8 14.2Q14 12.8 16.4 14M23.8 15.5L28.2 15.5"
                                : "M11.8 14.6Q14 13.2 16.4 14.4M23.6 14.4Q26 13.2 28.2 14.6"
                        }
                    }
                }

                // olhos
                Repeater {
                    model: [14.2, 25.8]
                    Item {
                        id: eye
                        required property real modelData
                        required property int index
                        readonly property string style: pet.eyes === "wink" ? (index === 0 ? "open" : "happy") : pet.eyes
                        x: modelData - 4
                        y: 16.5
                        width: 8
                        height: 8

                        // aberto
                        Rectangle {
                            visible: eye.style === "open" || eye.style === "half"
                            width: eye.style === "half" ? 4.4 : 4.2
                            height: Math.max(0.8, (eye.style === "half" ? 2.4 : 5.4) * pet.blink)
                            radius: Math.min(width, height) / 2
                            x: 4 - width / 2 + pet.lookX * 0.55
                            y: (eye.style === "half" ? 4.8 : 4) - height / 2 + pet.lookY * 0.5
                            color: pet.ink
                            Rectangle {
                                visible: parent.height > 3
                                x: 0.7; y: 0.8; width: 1.6; height: 1.6; radius: 0.8
                                color: "white"
                            }
                            Rectangle {
                                visible: parent.height > 4
                                x: 2.6; y: 3.4; width: 0.8; height: 0.8; radius: 0.4
                                color: "white"; opacity: 0.8
                            }
                        }
                        // arregalado
                        Rectangle {
                            visible: eye.style === "wide"
                            anchors.centerIn: parent
                            width: 6.6; height: 6.8 * pet.blink; radius: width / 2
                            color: "white"
                            Rectangle {
                                width: 3.4; height: 3.6; radius: 1.7
                                x: (parent.width - width) / 2 + pet.lookX * 0.4
                                y: (parent.height - height) / 2 + 0.3
                                color: pet.ink
                                Rectangle { x: 0.6; y: 0.6; width: 1.2; height: 1.2; radius: 0.6; color: "white" }
                            }
                        }
                        // fechado / feliz / linha
                        Shape {
                            visible: eye.style === "closed" || eye.style === "happy" || eye.style === "line"
                            anchors.fill: parent
                            preferredRendererType: Theme.shapeRenderer
                            ShapePath {
                                strokeColor: pet.ink; strokeWidth: 1.5; fillColor: "transparent"
                                capStyle: ShapePath.RoundCap
                                PathSvg {
                                    path: eye.style === "happy" ? "M1.6 5.2Q4 1.6 6.4 5.2"
                                        : eye.style === "line" ? "M1.8 4.4L6.2 4.4"
                                        : "M1.6 3.8Q4 6.4 6.4 3.8"
                                }
                            }
                        }
                        // estrela
                        Shape {
                            visible: eye.style === "star"
                            anchors.fill: parent
                            preferredRendererType: Theme.shapeRenderer
                            rotation: (1 - pet.sparkT) * 72
                            scale: 1.15
                            ShapePath {
                                fillColor: "#FFD60A"; strokeColor: "#E0A800"; strokeWidth: 0.4
                                joinStyle: ShapePath.RoundJoin
                                PathSvg { path: "M4 0.3L5.1 2.8L7.8 3L5.7 4.8L6.4 7.5L4 6L1.6 7.5L2.3 4.8L0.2 3L2.9 2.8Z" }
                            }
                        }
                        // coração
                        Shape {
                            visible: eye.style === "heart"
                            anchors.fill: parent
                            preferredRendererType: Theme.shapeRenderer
                            scale: 1.1 + Math.sin(pet.heartsT * Math.PI * 4) * 0.12
                            ShapePath {
                                fillColor: "#FF375F"; strokeColor: "transparent"
                                PathSvg { path: "M4 7.2C1 5.2 0.3 3.5 0.9 2.2C1.6 0.9 3.3 1 4 2.5C4.7 1 6.4 0.9 7.1 2.2C7.7 3.5 7 5.2 4 7.2Z" }
                            }
                        }
                    }
                }

                // bochechas
                Rectangle {
                    x: 8.4; y: 23.4; width: 4.6; height: 2.5; radius: 1.25
                    color: "#FF7A90"
                    opacity: pet.expr === "love" || pet.expr === "content" ? 0.85 : pet.mood === "sleep" ? 0.3 : 0.55
                }
                Rectangle {
                    x: 27; y: 23.4; width: 4.6; height: 2.5; radius: 1.25
                    color: "#FF7A90"
                    opacity: pet.expr === "love" || pet.expr === "content" ? 0.85 : pet.mood === "sleep" ? 0.3 : 0.55
                }

                // boca
                Item {
                    x: 15
                    y: 22.6
                    width: 10
                    height: 6

                    Shape {
                        visible: ["small", "smile", "cat", "flat", "wavy", "tongue"].indexOf(pet.mouth) >= 0
                        anchors.fill: parent
                        preferredRendererType: Theme.shapeRenderer
                        ShapePath {
                            strokeColor: pet.ink; strokeWidth: 1.2; fillColor: "transparent"
                            capStyle: ShapePath.RoundCap; joinStyle: ShapePath.RoundJoin
                            PathSvg {
                                path: pet.mouth === "small" ? "M3.9 2.4Q5 3.4 6.1 2.4"
                                    : pet.mouth === "cat" ? "M2.6 1.9Q3.8 3.5 5 1.9Q6.2 3.5 7.4 1.9"
                                    : pet.mouth === "flat" ? "M3.4 2.8L6.6 2.8"
                                    : pet.mouth === "wavy" ? "M2.8 3Q3.9 2 5 3Q6.1 4 7.2 3"
                                    : "M3.2 2Q5 4 6.8 2"
                            }
                        }
                    }
                    Rectangle {
                        visible: pet.mouth === "tongue"
                        x: 4.6; y: 2.7; width: 2; height: 2.3; radius: 1
                        color: "#FF7A90"
                    }
                    Rectangle {
                        visible: pet.mouth === "o"
                        x: 3.6; y: 1.2; width: 2.8; height: 3.2; radius: 1.4
                        color: pet.mouthFill
                    }
                    Shape {
                        visible: pet.mouth === "grin"
                        anchors.fill: parent
                        preferredRendererType: Theme.shapeRenderer
                        ShapePath {
                            fillColor: pet.mouthFill; strokeColor: "transparent"
                            PathSvg { path: "M2.6 1.4H7.4Q7.4 5.2 5 5.2Q2.6 5.2 2.6 1.4Z" }
                        }
                        ShapePath {
                            fillColor: "#FF7A90"; strokeColor: "transparent"
                            PathSvg { path: "M3.6 4.2Q5 3 6.4 4.2Q5.8 5.1 5 5.1Q4.2 5.1 3.6 4.2Z" }
                        }
                    }
                    Rectangle {
                        visible: pet.mouth === "talk"
                        width: 3.4
                        height: pet.talkH
                        radius: Math.min(width, height) / 2
                        x: 5 - width / 2
                        y: 2.6 - height / 2 + 0.6
                        color: pet.mouthFill
                    }
                    Rectangle {
                        visible: pet.mouth === "yawn"
                        x: 3; y: 0.6; width: 4; height: 4.8; radius: 2
                        color: pet.mouthFill
                        Rectangle { x: 0.9; y: 2.8; width: 2.2; height: 1.6; radius: 0.8; color: "#FF7A90" }
                    }
                }
            }

            // ---- mãozinhas ----
            Rectangle {
                x: 3.4; y: 25.5 + pet.handLY
                width: 5; height: 4.4; radius: 2.2
                color: pet.body
                border.color: Theme.a(pet.shade, 0.55); border.width: 0.5
            }
            Rectangle {
                x: 31.6; y: 25.5 + pet.handRY
                width: 5; height: 4.4; radius: 2.2
                color: pet.body
                border.color: Theme.a(pet.shade, 0.55); border.width: 0.5
                transform: Rotation { origin.x: 1; origin.y: 4; angle: pet.handRRot }
            }
        }

        // ---- partículas ----
        Repeater {
            model: 3
            Text {
                required property int index
                visible: pet.expr === "sleep" && pet.animated
                text: "z"
                color: Theme.text2
                font.pixelSize: 6 + index * 1.6
                font.weight: Font.Bold
                x: 31 + index * 2.6
                y: 9 - index * 4.4
                opacity: 0
                SequentialAnimation on opacity {
                    running: pet.active && pet.expr === "sleep"
                    loops: Animation.Infinite
                    PauseAnimation { duration: index * 500 }
                    NumberAnimation { from: 0; to: 0.9; duration: 500 }
                    NumberAnimation { to: 0; duration: 900 }
                    PauseAnimation { duration: (2 - index) * 500 + 300 }
                }
            }
        }

        Shape {
            id: sweat
            visible: pet.expr === "tired" || pet.expr === "sigh"
            property real t: 0
            x: 31.5; y: 9 + t * 4
            width: 6; height: 8
            opacity: 1 - t * 0.7
            preferredRendererType: Theme.shapeRenderer
            NumberAnimation on t {
                running: pet.active && sweat.visible
                from: 0; to: 1; duration: 1400; loops: Animation.Infinite; easing.type: Easing.InQuad
            }
            ShapePath {
                fillColor: Theme.blue; strokeColor: "transparent"
                PathSvg { path: "M3 0Q6 4 5.2 5.6Q3 8.4 0.8 5.6Q0 4 3 0Z" }
            }
        }

        // corações
        Repeater {
            model: [[30, 9, 0], [5, 7, 0.15], [20, 6, 0.3]]
            Shape {
                required property var modelData
                readonly property real t: Math.max(0, Math.min(1, (pet.heartsT - modelData[2]) / (1 - modelData[2])))
                visible: pet.heartsT < 1 && t > 0
                width: 8; height: 8
                x: modelData[0] + Math.sin(t * Math.PI * 2) * 1.6
                y: 12 - t * 16
                opacity: t < 0.55 ? 1 : (1 - t) / 0.45
                scale: (0.5 + Math.min(1, t * 5) * 0.6) * modelData[1] / 8
                preferredRendererType: Theme.shapeRenderer
                ShapePath {
                    fillColor: modelData[2] === 0.15 ? "#FF9FB0" : "#FF6482"
                    strokeColor: "transparent"
                    PathSvg { path: "M4 7.2C1 5.2 0.3 3.5 0.9 2.2C1.6 0.9 3.3 1 4 2.5C4.7 1 6.4 0.9 7.1 2.2C7.7 3.5 7 5.2 4 7.2Z" }
                }
            }
        }

        // brilhos
        Repeater {
            model: [[3, 8, 0], [34, 14, 0.2], [30, 2, 0.35], [7, 28, 0.5]]
            Shape {
                required property var modelData
                readonly property real t: Math.max(0, Math.min(1, (pet.sparkT - modelData[2]) / 0.5))
                visible: pet.sparkT < 1 && t > 0 && t < 1
                x: modelData[0] - 3; y: modelData[1] - 3
                width: 6; height: 6
                scale: Math.sin(t * Math.PI)
                preferredRendererType: Theme.shapeRenderer
                ShapePath {
                    fillColor: "#FFD60A"; strokeColor: "transparent"
                    PathSvg { path: "M3 0Q3.4 2.6 6 3Q3.4 3.4 3 6Q2.6 3.4 0 3Q2.6 2.6 3 0Z" }
                }
            }
        }

        // "!" e "?"
        Text {
            visible: pet.expr === "surprised" || pet.expr === "curious"
            text: pet.expr === "surprised" ? "!" : "?"
            color: pet.expr === "surprised" ? Theme.orange : Theme.accentText
            font.pixelSize: 11
            font.weight: Font.Black
            x: 32; y: 0
            scale: pet.popS
            rotation: pet.expr === "curious" ? 12 : 0
        }

        // raio (pronto pra focar)
        Shape {
            visible: pet.expr === "ready"
            x: 32; y: 2
            width: 8; height: 10
            scale: pet.popS
            preferredRendererType: Theme.shapeRenderer
            ShapePath {
                fillColor: "#FFD60A"; strokeColor: "transparent"
                PathSvg { path: "M5 0L0.6 5.8H3.8L3 10L7.4 4.2H4.2Z" }
            }
        }

        // notinhas (cantarolando)
        Repeater {
            model: [[31, 0], [35, 0.4]]
            Text {
                required property var modelData
                readonly property real t: Math.max(0, Math.min(1, (pet.notesT - modelData[1]) / 0.6))
                visible: pet.expr === "hum" && t > 0 && t < 1
                text: modelData[1] === 0 ? "♪" : "♫"
                color: Theme.accentText
                font.pixelSize: 8
                x: modelData[0] + Math.sin(t * Math.PI * 2) * 1.5
                y: 12 - t * 13
                opacity: 1 - t
            }
        }
    }
}
