import QtQuick

// "Concluídas hoje": recolhível, com opção de devolver para a fila.
Column {
    id: ds
    property bool open: false
    spacing: 2

    readonly property var items: {
        Store.revision
        const sod = Store.startOfDay()
        const out = []
        for (let i = 0; i < Store.doneModel.count; i++) {
            const t = Store.doneModel.get(i)
            if (t.doneAt < sod) break
            out.push({ index: i, title: t.title, spentMs: t.spentMs })
        }
        return out
    }

    Item {
        width: ds.width
        height: 26
        Row {
            x: 4
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            Text {
                text: "CONCLUÍDAS HOJE"
                color: headMa.containsMouse ? Theme.text2 : Theme.text3
                font.pixelSize: 10
                font.weight: Font.DemiBold
                font.letterSpacing: 1.0
            }
            Text {
                text: ds.items.length
                color: Theme.green
                font.pixelSize: 10
                font.weight: Font.DemiBold
            }
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: "chevronDown"
                size: 12
                color: Theme.text3
                rotation: ds.open ? 180 : 0
                Behavior on rotation { NumberAnimation { duration: 220; easing.type: Easing.OutCubic } }
            }
        }
        MouseArea {
            id: headMa
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: ds.open = !ds.open
        }
    }

    Repeater {
        model: ds.open ? ds.items : []
        Item {
            id: doneRow
            required property var modelData
            width: ds.width
            height: 34
            HoverHandler { id: dh }
            Rectangle { anchors.fill: parent; radius: 10; color: dh.hovered ? Theme.hover : "transparent" }
            Rectangle {
                id: chk
                x: 8
                anchors.verticalCenter: parent.verticalCenter
                width: 18; height: 18; radius: 9
                color: Theme.a(Theme.green, 0.2)
                Icon { anchors.centerIn: parent; name: "check"; size: 12; stroke: 2.2; color: Theme.green }
            }
            Text {
                anchors.left: chk.right
                anchors.leftMargin: 10
                anchors.right: spent.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                text: doneRow.modelData.title
                color: Theme.text2
                font.pixelSize: 13
                font.strikeout: true
                elide: Text.ElideRight
            }
            Text {
                id: spent
                anchors.right: restoreBtn.left
                anchors.rightMargin: 6
                anchors.verticalCenter: parent.verticalCenter
                text: Store.fmtShort(doneRow.modelData.spentMs)
                color: Theme.text3
                font.pixelSize: 11
                font.features: ({ "tnum": 1 })
            }
            IconButton {
                id: restoreBtn
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.verticalCenter: parent.verticalCenter
                icon: "restore"; size: 26; iconSize: 13
                opacity: dh.hovered ? 1 : 0
                onClicked: Store.restoreDone(doneRow.modelData.index)
            }
        }
    }
}
