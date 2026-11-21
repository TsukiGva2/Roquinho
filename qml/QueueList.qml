import QtQuick

// Fila "A seguir": arraste pela alça para reorganizar, duplo clique para renomear.
ListView {
    id: list
    readonly property real rowH: 40

    implicitHeight: contentHeight
    height: contentHeight
    interactive: false
    cacheBuffer: 4000
    spacing: 2
    model: Store.queueModel

    add: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 260 }
            NumberAnimation { property: "x"; from: -24; to: 0; duration: 340; easing.type: Easing.OutCubic }
        }
    }
    remove: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; to: 0; duration: 200 }
            NumberAnimation { property: "scale"; to: 0.9; duration: 200 }
        }
    }
    displaced: Transition { NumberAnimation { properties: "x,y"; duration: 260; easing.type: Easing.OutCubic } }
    moveDisplaced: Transition { NumberAnimation { properties: "y"; duration: 220; easing.type: Easing.OutCubic } }

    delegate: DropArea {
        id: slot
        required property int index
        required property string title
        required property double accMs

        width: list.width
        height: list.rowH
        keys: ["foco-queue"]
        onEntered: (drag) => Store.moveQueue(drag.source.dragIndex, slot.index)

        Item {
            id: row
            property int dragIndex: slot.index
            property bool editing: false

            width: slot.width
            height: slot.height
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.verticalCenter: parent.verticalCenter

            Drag.active: grip.drag.active
            Drag.source: row
            Drag.hotSpot.x: width / 2
            Drag.hotSpot.y: height / 2
            Drag.keys: ["foco-queue"]

            states: State {
                when: grip.drag.active
                ParentChange { target: row; parent: list }
                AnchorChanges { target: row; anchors.horizontalCenter: undefined; anchors.verticalCenter: undefined }
                PropertyChanges { target: rowBg; color: Theme.fill; border.color: Theme.a(Theme.accent, 0.5) }
                PropertyChanges { target: row; z: 10; scale: 1.02 }
            }

            Rectangle {
                id: rowBg
                anchors.fill: parent
                radius: 12
                color: rowHover.hovered ? Theme.hover : "transparent"
                border.width: 1
                border.color: "transparent"
                Behavior on color { ColorAnimation { duration: 120 } }
            }
            HoverHandler { id: rowHover }

            Item {
                id: gripBox
                width: 28
                height: parent.height
                Icon {
                    anchors.centerIn: parent
                    name: "grip"
                    size: 14
                    color: Theme.text2
                    opacity: rowHover.hovered || grip.drag.active ? 0.9 : 0.25
                    Behavior on opacity { NumberAnimation { duration: 140 } }
                }
                MouseArea {
                    id: grip
                    anchors.fill: parent
                    drag.target: row
                    drag.axis: Drag.YAxis
                    drag.minimumY: -4
                    drag.maximumY: list.height - list.rowH + 4
                    preventStealing: true
                    cursorShape: drag.active ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                    onReleased: row.Drag.drop()
                }
            }

            Text {
                id: num
                x: 28
                width: 18
                anchors.verticalCenter: parent.verticalCenter
                text: slot.index + 1
                color: slot.index === 0 ? Theme.accentText : Theme.text3
                font.pixelSize: 12
                font.weight: Font.DemiBold
                font.features: ({ "tnum": 1 })
            }

            Text {
                id: titleText
                visible: !row.editing
                anchors.left: num.right
                anchors.leftMargin: 6
                anchors.right: acts.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                text: slot.title
                color: Theme.text
                font.pixelSize: 13
                elide: Text.ElideRight
                MouseArea {
                    anchors.fill: parent
                    onDoubleClicked: { row.editing = true; editor.text = slot.title; editor.forceActiveFocus(); editor.selectAll() }
                }
            }
            Text {
                visible: !row.editing && slot.accMs > 60000 && !rowHover.hovered
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                text: Store.fmtShort(slot.accMs)
                color: Theme.text3
                font.pixelSize: 11
            }

            TextInput {
                id: editor
                visible: row.editing
                anchors.left: num.right
                anchors.leftMargin: 6
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                color: Theme.text
                font.pixelSize: 13
                selectByMouse: true
                selectionColor: Theme.a(Theme.accent, 0.6)
                clip: true
                function commit() { if (row.editing) { Store.renameQueue(slot.index, text); row.editing = false } }
                Keys.onReturnPressed: commit()
                Keys.onEnterPressed: commit()
                Keys.onEscapePressed: row.editing = false
                onActiveFocusChanged: if (!activeFocus) commit()
            }

            Row {
                id: acts
                visible: !row.editing
                anchors.right: parent.right
                anchors.rightMargin: 6
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                opacity: rowHover.hovered ? 1 : 0
                Behavior on opacity { NumberAnimation { duration: 140 } }
                IconButton {
                    icon: "target"; size: 28; iconSize: 15
                    color: Theme.accentText; hoverColor: "white"; hoverBg: Theme.a(Theme.accent, 0.5)
                    onClicked: Store.focusFromQueue(slot.index)
                }
                IconButton {
                    icon: "trash"; size: 28; iconSize: 14
                    hoverColor: Theme.red; hoverBg: Theme.a(Theme.red, 0.15)
                    onClicked: Store.removeQueue(slot.index)
                }
            }
        }
    }
}
