import QtQuick
import QtQuick.Effects

// Janela transparente (layer-shell, camada overlay) ancorada no topo da tela.
// Só a área visível do notch recebe cliques; o resto passa direto para o desktop.
Window {
    id: win
    width: 760
    height: 760
    color: "transparent"
    visible: false // o C++ configura o layer-shell e mostra
    title: "Foquinho"

    property bool booted: false
    property bool quitting: false
    property bool expanded: false
    property bool settingsOpen: false
    property double expandStamp: 0
    property var alertData: null

    readonly property bool hovered: notchHover.hovered && !expanded && !alertData
    readonly property bool hiddenMode: Store.hidden && !quitting

    readonly property real targetW: (!booted || quitting) ? 0
        : hiddenMode ? 36
        : expanded ? 548
        : alertData ? 400
        : collapsedView.implicitWidth
    readonly property real targetH: (!booted || quitting) ? 0
        : hiddenMode ? 0
        : expanded ? expandedView.implicitHeight
        : alertData ? 62
        : (hovered ? 42 : 38)
    readonly property real targetR: expanded ? 32 : alertData ? 28 : (hovered ? 21 : 19)

    property real curW: targetW
    property real curH: targetH
    property real curR: targetR
    Behavior on curW { SpringAnimation { spring: 3.8; damping: 0.32; epsilon: 0.2 } }
    Behavior on curH { SpringAnimation { spring: 3.4; damping: 0.38; epsilon: 0.2 } }
    Behavior on curR { NumberAnimation { duration: 320; easing.type: Easing.OutCubic } }

    // ---------- ações ----------
    function expand(withInput) {
        if (Store.hidden) Store.hidden = false
        alertData = null
        expanded = true
        expandStamp = Date.now()
        if (withInput) {
            // Captura rápida: pega o teclado até Enter/Esc (ou 45 s parado sem texto).
            settingsOpen = false
            quickMode = true
            Backend.grabKeyboard(true)
            quickIdle.restart()
            Qt.callLater(expandedView.focusInput)
        }
    }
    function collapse() {
        expanded = false
        settingsOpen = false
        quickMode = false
        quickIdle.stop()
        root.forceActiveFocus()
        Backend.grabKeyboard(false)
    }
    function hideToDot() {
        collapse()
        alertData = null
        Store.hidden = true
    }
    function quitApp() {
        Store.save()
        expanded = false
        quitting = true
        quitTimer.start()
    }
    function showAlert(a, ms) {
        if (expanded || Store.hidden) return
        alertData = a
        alertTimer.interval = ms || 2600
        alertTimer.restart()
    }

    property bool quickMode: false
    Timer {
        id: quickIdle; interval: 45000
        onTriggered: if (win.quickMode) { if (expandedView.inputEmpty) win.collapse(); else restart() }
    }
    Timer { id: alertTimer; interval: 2600; onTriggered: win.alertData = null }
    Timer { id: quitTimer; interval: 650; onTriggered: Backend.quit() }
    Timer {
        id: bootTimer
        interval: 250
        onTriggered: {
            win.booted = true
            if (Store.firstRun) {
                if (Backend && !Backend.autostart) Backend.autostart = true
                welcome.start()
            }
        }
    }
    Timer {
        id: welcome
        interval: 900
        onTriggered: win.showAlert({ kind: "info", title: "Oi! Eu sou o " + Store.petName,
                                     sub: "Clique aqui e registre sua primeira demanda" }, 5200)
    }

    // ---------- balão de atalhos: ao iniciar e uma vez por dia ----------
    property bool tipPending: false
    readonly property bool tipShown: tipPending && booted && !quitting && !expanded && !hiddenMode && !alertData
    function maybeTip(onStartup) {
        if (!Store.tipsEnabled || tipPending) return
        if (onStartup || Store.lastTipDay !== Store.dayKey()) tipPending = true
    }
    function answerTip(never) {
        tipPending = false
        Store.tipTalking = false
        Store.lastTipDay = Store.dayKey()
        if (never) { Store.tipsEnabled = false; Store.say("Combinado, não falo mais!") }
        else Store.petEvent("love")
    }
    Timer { id: tipStartup; interval: Store.firstRun ? 7500 : 3000; onTriggered: win.maybeTip(true) }
    Timer { interval: 60000; running: win.booted; repeat: true; onTriggered: win.maybeTip(false) }

    property double lastWave: 0
    onExpandedChanged: {
        if (expanded && Date.now() - lastWave > 90000) {
            lastWave = Date.now()
            waveDelay.restart()
        }
    }
    Timer { id: waveDelay; interval: 380; onTriggered: Store.petEvent("wave") }

    Component.onCompleted: {
        Store.backend = Backend
        Store.load()
        bootTimer.start()
        tipStartup.start()
    }

    onActiveChanged: {
        // clicou fora: recolhe o painel (nunca esconde o notch)
        if (!active && expanded && Date.now() - expandStamp > 450) collapse()
    }

    Connections {
        target: Backend
        function onCommand(name, arg) {
            switch (name) {
            case "toggle": win.expanded ? win.collapse() : win.expand(false); break
            case "open": win.expand(false); break
            case "add":
                if (arg) Store.addTask(arg, false)
                else win.expand(true)
                break
            case "focus":
                if (arg) Store.addTask(arg, true)
                break
            case "hide": Store.hidden ? (Store.hidden = false) : win.hideToDot(); break
            case "show": Store.hidden = false; break
            case "done": Store.completeFirst(); break
            case "pause": Store.toggleAll(); break
            case "quit": win.quitApp(); break
            }
        }
    }

    Connections {
        target: Store
        function onCompleted(title, spentMs) {
            win.showAlert({ kind: "done", title: "Concluída!", sub: title + "  ·  " + Store.fmtShort(spentMs) })
        }
        function onAdded(title, toFocus) {
            if (win.quickMode) win.collapse()
            win.showAlert({ kind: "added", title: toFocus ? "Em foco agora" : "Adicionada à fila", sub: title }, 2000)
        }
    }

    // ---------- área clicável ----------
    function updateMask() {
        const rects = []
        if (hiddenMode) {
            rects.push(Qt.rect(win.width / 2 - 18, 4, 36, 36))
        } else {
            const w = Math.max(targetW, 8) + 2 * 12 + 8
            const h = Math.max(targetH, 6) + 4
            rects.push(Qt.rect((win.width - w) / 2, 0, w, h))
        }
        if (tipShown)
            rects.push(Qt.rect(tip.x - 4, tip.y - 2, tip.width + 8, tip.height + 8))
        Backend.setInputRegion(rects)
    }
    onTargetWChanged: Qt.callLater(updateMask)
    onTargetHChanged: Qt.callLater(updateMask)
    onHiddenModeChanged: Qt.callLater(updateMask)
    onTipShownChanged: Qt.callLater(updateMask)

    // ---------- visual ----------
    Item {
        id: root
        anchors.fill: parent
        focus: true
        Keys.onEscapePressed: if (win.expanded) win.collapse()

        Item {
            id: notch
            readonly property real ear: Math.min(12, height / 2.6)
            width: Math.max(0, win.curW) + 2 * ear
            height: Math.max(0, win.curH)
            x: Math.round((win.width - width) / 2)
            y: 0
            visible: height > 0.5 && win.curW > 1

            RectangularShadow {
                x: notch.ear
                width: Math.max(0, win.curW)
                height: notch.height
                radius: Math.min(win.curR, height / 2)
                blur: 34
                spread: -4
                offset.y: 14
                color: "#000000"
                opacity: win.expanded ? 0.65 : win.alertData ? 0.45 : 0
                Behavior on opacity { NumberAnimation { duration: 280 } }
            }

            NotchShape {
                anchors.fill: parent
                ear: notch.ear
                radius: win.curR
                color: Theme.notch
            }

            MouseArea {
                anchors.fill: parent
                enabled: !win.expanded
                cursorShape: Qt.PointingHandCursor
                onClicked: win.expand(false)
            }
            HoverHandler { id: notchHover }

            Item {
                id: clipper
                x: notch.ear
                width: Math.max(0, win.curW)
                height: notch.height
                clip: true

                CollapsedView {
                    id: collapsedView
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: implicitWidth
                    height: win.hovered ? 42 : 38
                    hovered: win.hovered
                    shown: win.booted && !win.quitting && !win.expanded && !win.alertData && !win.hiddenMode
                }

                AlertView {
                    anchors.horizontalCenter: parent.horizontalCenter
                    alert: win.alertData
                    shown: !!win.alertData && !win.expanded && !win.hiddenMode && !win.quitting
                }

                ExpandedView {
                    id: expandedView
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 548
                    height: implicitHeight
                    shown: win.expanded && !win.quitting
                    settingsOpen: win.settingsOpen
                    onCollapseRequested: win.collapse()
                    onHideRequested: win.hideToDot()
                    onQuitRequested: win.quitApp()
                    onSettingsToggled: win.settingsOpen = !win.settingsOpen
                }
            }
        }

        TipBubble {
            id: tip
            x: Math.round((win.width - collapsedView.width) / 2 + collapsedView.petCenter - tailX)
            y: Math.round(Math.max(win.curH, 30) + 8)
            shown: win.tipShown
            onTypingChanged: (typing) => Store.tipTalking = typing
            onOkClicked: win.answerTip(false)
            onNeverClicked: win.answerTip(true)
            onXChanged: if (win.tipShown) Qt.callLater(win.updateMask)
            onHeightChanged: if (win.tipShown) Qt.callLater(win.updateMask)
        }

        Dot {
            x: Math.round((win.width - width) / 2)
            y: 5
            shown: win.hiddenMode && win.booted
            onClicked: Store.hidden = false
        }
    }
}
