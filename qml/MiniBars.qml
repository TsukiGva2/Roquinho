import QtQuick

// Barrinhas cilíndricas (sombreadas nas bordas) que dançam com as bandas do áudio.
Row {
    id: mb
    property var bands: [3, 1, 7, 12]   // índice da banda (0 = graves) para cada barra
    property real barWidth: 3
    property real minH: 3
    property real maxH: 14
    property color color: Theme.music
    property bool live: Media.playing

    spacing: 2

    Repeater {
        model: mb.bands.length
        Rectangle {
            required property int index
            readonly property real v: mb.live ? (Media.levels[mb.bands[index]] || 0) : 0
            anchors.verticalCenter: parent.verticalCenter
            width: mb.barWidth
            height: mb.minH + (mb.maxH - mb.minH) * Math.min(1, v * 1.15)
            radius: width / 2
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0.0; color: Qt.darker(mb.color, 1.6) }
                GradientStop { position: 0.4; color: Qt.lighter(mb.color, 1.25) }
                GradientStop { position: 1.0; color: Qt.darker(mb.color, 1.8) }
            }
        }
    }
}
