import QtQuick

// Campo para registrar uma demanda. Enter = fila · Ctrl+Enter = direto em foco.
Rectangle {
    id: f
    signal submitted(string text, bool toFocus)
    signal escapePressed()
    readonly property bool empty: input.text.length === 0

    function forceFocus() { input.forceActiveFocus() }

    height: 44
    radius: 14
    color: input.activeFocus ? Theme.cardHover : Theme.card
    border.width: 1
    border.color: input.activeFocus ? Theme.a(Theme.accent, 0.75) : Theme.stroke
    Behavior on border.color { ColorAnimation { duration: 160 } }
    Behavior on color { ColorAnimation { duration: 160 } }

    function submit(toFocus) {
        const t = input.text.trim()
        if (!t) { shake.restart(); return }
        f.submitted(t, toFocus)
        input.text = ""
    }

    Rectangle {
        id: plusBg
        x: 9
        anchors.verticalCenter: parent.verticalCenter
        width: 26; height: 26; radius: 13
        color: input.activeFocus || input.text ? Theme.accent : Theme.fill
        Behavior on color { ColorAnimation { duration: 180 } }
        Icon { anchors.centerIn: parent; name: "plus"; size: 14; stroke: 2.2; color: "white" }
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: input.text.trim() ? f.submit(false) : input.forceActiveFocus()
        }
    }

    TextInput {
        id: input
        anchors.left: plusBg.right
        anchors.leftMargin: 10
        anchors.right: hint.visible ? hint.left : parent.right
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        color: Theme.text
        font.pixelSize: 13
        selectByMouse: true
        selectionColor: Theme.a(Theme.accent, 0.6)
        selectedTextColor: "white"
        clip: true
        maximumLength: 160
        Keys.onReturnPressed: (e) => f.submit(e.modifiers & Qt.ControlModifier)
        Keys.onEnterPressed: (e) => f.submit(e.modifiers & Qt.ControlModifier)
        Keys.onEscapePressed: (e) => {
            if (input.text) input.text = ""
            else { input.focus = false; f.escapePressed() }
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: !input.text
            text: "Nova demanda…"
            color: Theme.text3
            font: input.font
        }
    }

    Row {
        id: hint
        visible: input.activeFocus && f.width > 300
        anchors.right: parent.right
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        spacing: 10
        Repeater {
            model: [["⏎", "fila"], ["Ctrl ⏎", "foco"]]
            Row {
                required property var modelData
                spacing: 4
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    height: 17; radius: 5
                    width: keyText.implicitWidth + 10
                    color: Theme.fill
                    Text { id: keyText; anchors.centerIn: parent; text: modelData[0]; color: Theme.text2; font.pixelSize: 10 }
                }
                Text { anchors.verticalCenter: parent.verticalCenter; text: modelData[1]; color: Theme.text3; font.pixelSize: 11 }
            }
        }
    }

    transform: Translate { id: shakeT }
    SequentialAnimation {
        id: shake
        NumberAnimation { target: shakeT; property: "x"; to: -6; duration: 50 }
        NumberAnimation { target: shakeT; property: "x"; to: 6; duration: 70 }
        NumberAnimation { target: shakeT; property: "x"; to: -3; duration: 60 }
        NumberAnimation { target: shakeT; property: "x"; to: 0; duration: 60 }
    }
}
