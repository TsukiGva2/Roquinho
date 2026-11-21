import QtQuick
import QtQuick.Shapes

Column {
    id: ms
    property bool shown: true
    signal escapePressed()
    readonly property bool inputEmpty: inputField.empty

    function focusInput() { inputField.forceFocus() }

    spacing: 10
    opacity: shown ? 1 : 0
    visible: opacity > 0.01
    Behavior on opacity { NumberAnimation { duration: 200 } }

    SectionLabel {
        width: parent.width
        label: "EM FOCO"
        count: Store.focusModel.count
    }

    ListView {
        id: focusList
        width: parent.width
        height: contentHeight
        visible: count > 0
        interactive: false
        cacheBuffer: 4000
        spacing: 8
        model: Store.focusModel
        delegate: FocusCard { width: focusList.width }
        add: Transition {
            ParallelAnimation {
                NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 320 }
                NumberAnimation { property: "scale"; from: 0.9; to: 1; duration: 460; easing.type: Easing.OutBack }
            }
        }
        remove: Transition {
            ParallelAnimation {
                NumberAnimation { property: "opacity"; to: 0; duration: 280 }
                NumberAnimation { property: "x"; to: 80; duration: 320; easing.type: Easing.InCubic }
            }
        }
        displaced: Transition { NumberAnimation { properties: "x,y"; duration: 340; easing.type: Easing.OutCubic } }
    }

    // estado vazio
    Item {
        width: parent.width
        height: 72
        visible: Store.focusModel.count === 0
        Shape {
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: Theme.fill2
                strokeWidth: 1.2
                strokeStyle: ShapePath.DashLine
                dashPattern: [4, 4]
                fillColor: "transparent"
                PathRectangle { x: 0.5; y: 0.5; width: ms.width - 1; height: 71; radius: 18 }
            }
        }
        Pet {
            id: emptyPet
            x: 16
            anchors.verticalCenter: parent.verticalCenter
            size: 38
            kind: Store.petKind
            mood: "sleep"
            animated: parent.visible && ms.visible
        }
        Column {
            anchors.left: emptyPet.right
            anchors.leftMargin: 12
            anchors.right: startBtn.visible ? startBtn.left : parent.right
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text { text: "Nada em foco"; color: Theme.text; font.pixelSize: 14; font.weight: Font.Medium }
            Text {
                width: parent.width
                text: Store.queueModel.count > 0 ? "Puxe a próxima da fila ou crie uma nova." : "Escreva abaixo o que precisa ser feito."
                color: Theme.text2
                font.pixelSize: 12
                elide: Text.ElideRight
            }
        }
        Rectangle {
            id: startBtn
            visible: Store.queueModel.count > 0
            anchors.right: parent.right
            anchors.rightMargin: 14
            anchors.verticalCenter: parent.verticalCenter
            height: 32
            width: startRow.implicitWidth + 24
            radius: 16
            color: startMa.containsMouse ? Qt.lighter(Theme.accent, 1.1) : Theme.accent
            scale: startMa.pressed ? 0.94 : 1
            Behavior on scale { NumberAnimation { duration: 120 } }
            Row {
                id: startRow
                anchors.centerIn: parent
                spacing: 6
                Icon { anchors.verticalCenter: parent.verticalCenter; name: "play"; size: 12; color: "white" }
                Text { text: "Começar próxima"; color: "white"; font.pixelSize: 12; font.weight: Font.DemiBold }
            }
            MouseArea { id: startMa; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: Store.focusFromQueue(0) }
        }
    }

    InputField {
        id: inputField
        width: parent.width
        onSubmitted: (text, toFocus) => Store.addTask(text, toFocus)
        onEscapePressed: ms.escapePressed()
    }

    Item { width: 1; height: 2; visible: Store.queueModel.count > 0 }

    SectionLabel {
        width: parent.width
        visible: Store.queueModel.count > 0
        label: "A SEGUIR"
        count: Store.queueModel.count
    }

    QueueList {
        width: parent.width
        visible: Store.queueModel.count > 0
    }

    DoneSection {
        width: parent.width
        visible: items.length > 0
    }
}
