import QtQuick

// Interruptor no estilo macOS.
Item {
    id: t
    property bool checked: false
    signal toggled(bool value)

    width: 38
    height: 22

    Rectangle {
        anchors.fill: parent
        radius: height / 2
        color: t.checked ? Theme.accent : Theme.fill2
        Behavior on color { ColorAnimation { duration: 180 } }
    }
    Rectangle {
        width: 18; height: 18; radius: 9
        y: 2
        x: t.checked ? t.width - width - 2 : 2
        color: "white"
        Behavior on x { NumberAnimation { duration: 240; easing.type: Easing.OutBack; easing.overshoot: 1.6 } }
    }
    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: t.toggled(!t.checked)
    }
}
