import QtQuick

Column {
    id: sp
    property bool shown: false
    signal quitRequested()

    spacing: 10
    opacity: shown ? 1 : 0
    visible: opacity > 0.01
    Behavior on opacity { NumberAnimation { duration: 200 } }

    SectionLabel { width: parent.width; label: "MASCOTE" }

    Row {
        spacing: 8
        Repeater {
            model: [["cat", "Gato"], ["ghost", "Fantasma"], ["slime", "Slime"]]
            Rectangle {
                id: tile
                required property var modelData
                readonly property bool selected: Store.petKind === modelData[0]
                width: (sp.width - 16) / 3
                height: 92
                radius: 16
                color: tileMa.containsMouse ? Theme.cardHover : Theme.card
                border.width: selected ? 1.5 : 1
                border.color: selected ? Theme.accent : Theme.stroke
                Behavior on border.color { ColorAnimation { duration: 160 } }
                Pet {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 12
                    size: 46
                    kind: tile.modelData[0]
                    mood: tile.selected ? "happy" : (tileMa.containsMouse ? "focus" : "idle")
                    animated: sp.visible
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 9
                    text: tile.modelData[1]
                    color: tile.selected ? Theme.text : Theme.text2
                    font.pixelSize: 12
                    font.weight: tile.selected ? Font.DemiBold : Font.Normal
                }
                MouseArea {
                    id: tileMa
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: Store.petKind = tile.modelData[0]
                }
            }
        }
    }

    Rectangle {
        width: parent.width
        height: 44
        radius: 14
        color: Theme.card
        border.width: 1
        border.color: nameInput.activeFocus ? Theme.a(Theme.accent, 0.75) : Theme.stroke
        Text {
            id: nameLabel
            x: 14
            anchors.verticalCenter: parent.verticalCenter
            text: "Nome"
            color: Theme.text2
            font.pixelSize: 13
        }
        TextInput {
            id: nameInput
            anchors.left: nameLabel.right
            anchors.leftMargin: 14
            anchors.right: parent.right
            anchors.rightMargin: 14
            anchors.verticalCenter: parent.verticalCenter
            horizontalAlignment: TextInput.AlignRight
            text: Store.petName
            color: Theme.text
            font.pixelSize: 13
            maximumLength: 20
            selectByMouse: true
            selectionColor: Theme.a(Theme.accent, 0.6)
            onEditingFinished: if (text.trim()) Store.petName = text.trim()
        }
    }

    Item { width: 1; height: 2 }
    SectionLabel { width: parent.width; label: "COMPORTAMENTO" }

    Rectangle {
        width: parent.width
        height: 150
        radius: 14
        color: Theme.card
        border.width: 1
        border.color: Theme.stroke
        Column {
            anchors.fill: parent
            Repeater {
                model: 3
                Item {
                    required property int index
                    width: parent.width
                    height: 50
                    Rectangle {
                        visible: index > 0
                        x: 14; width: parent.width - 28; height: 1
                        color: Theme.stroke
                    }
                    Column {
                        x: 14
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 1
                        Text {
                            text: ["Puxar a próxima ao concluir", "Iniciar junto com o sistema", "Dicas de atalhos do " + Store.petName][index]
                            color: Theme.text
                            font.pixelSize: 13
                        }
                        Text {
                            text: ["A primeira da fila entra em foco na hora", "Abre o notch quando você faz login", "Um balãozinho ao abrir e uma vez por dia"][index]
                            color: Theme.text3
                            font.pixelSize: 11
                        }
                    }
                    Toggle {
                        anchors.right: parent.right
                        anchors.rightMargin: 14
                        anchors.verticalCenter: parent.verticalCenter
                        checked: index === 0 ? Store.autoAdvance : index === 2 ? Store.tipsEnabled : (Store.backend ? Store.backend.autostart : false)
                        onToggled: (v) => {
                            if (index === 0) Store.autoAdvance = v
                            else if (index === 2) Store.tipsEnabled = v
                            else if (Store.backend) Store.backend.autostart = v
                        }
                    }
                }
            }
        }
    }

    Item { width: 1; height: 2 }
    SectionLabel { width: parent.width; label: "ATALHOS GLOBAIS" }

    Rectangle {
        width: parent.width
        height: shortcutsCol.implicitHeight + 16
        radius: 14
        color: Theme.card
        border.width: 1
        border.color: Theme.stroke
        Column {
            id: shortcutsCol
            x: 14
            y: 8
            width: parent.width - 28
            spacing: 2
            Repeater {
                model: [["N", "Nova demanda"], ["F", "Abrir / fechar painel"], ["H", "Esconder em bolinha"], ["D", "Concluir demanda em foco"]]
                Item {
                    required property var modelData
                    width: shortcutsCol.width
                    height: 28
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData[1]
                        color: Theme.text2
                        font.pixelSize: 12
                    }
                    Row {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 4
                        Repeater {
                            model: ["Meta", "Alt", modelData[0]]
                            Rectangle {
                                required property string modelData
                                height: 20
                                width: Math.max(20, keyT.implicitWidth + 12)
                                radius: 6
                                color: Theme.fill
                                border.width: 1
                                border.color: Theme.stroke
                                Text { id: keyT; anchors.centerIn: parent; text: modelData; color: Theme.text; font.pixelSize: 11 }
                            }
                        }
                    }
                }
            }
        }
    }
    Text {
        x: 4
        width: parent.width - 8
        wrapMode: Text.WordWrap
        text: "Dá para trocar em Configurações do Sistema › Teclado › Atalhos › Foquinho."
        color: Theme.text3
        font.pixelSize: 11
    }

    Item { width: 1; height: 4 }

    Item {
        width: parent.width
        height: 36
        Rectangle {
            id: quitBtn
            height: 34
            width: quitRow.implicitWidth + 26
            radius: 17
            color: quitMa.containsMouse ? Theme.a(Theme.red, 0.22) : Theme.a(Theme.red, 0.12)
            Behavior on color { ColorAnimation { duration: 140 } }
            Row {
                id: quitRow
                anchors.centerIn: parent
                spacing: 7
                Icon { anchors.verticalCenter: parent.verticalCenter; name: "power"; size: 14; color: Theme.red }
                Text { text: "Encerrar Foquinho"; color: Theme.red; font.pixelSize: 12; font.weight: Font.DemiBold }
            }
            MouseArea { id: quitMa; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: sp.quitRequested() }
        }
        Text {
            anchors.right: parent.right
            anchors.rightMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            text: "Foquinho 1.0 · " + Store.totalDone + " concluídas no total"
            color: Theme.text3
            font.pixelSize: 11
        }
    }
}
