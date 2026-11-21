import QtQuick

Item {
    id: sl
    property string label: ""
    property int count: -1
    height: 20

    Row {
        anchors.verticalCenter: parent.verticalCenter
        x: 4
        spacing: 6
        Text {
            text: sl.label
            color: Theme.text3
            font.pixelSize: 10
            font.weight: Font.DemiBold
            font.letterSpacing: 1.0
        }
        Text {
            visible: sl.count >= 0
            text: sl.count
            color: Theme.text3
            font.pixelSize: 10
            font.weight: Font.DemiBold
            font.features: ({ "tnum": 1 })
        }
    }
}
