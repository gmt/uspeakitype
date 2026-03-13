import QtQuick 2.15

Rectangle {
    id: root
    property string label: ""
    property color tint: "#2e6f68"

    radius: 13
    color: Qt.rgba(root.tint.r, root.tint.g, root.tint.b, 0.26)
    border.width: 1
    border.color: Qt.rgba(root.tint.r, root.tint.g, root.tint.b, 0.72)
    height: 30
    width: chipLabel.implicitWidth + 24

    Text {
        id: chipLabel
        anchors.centerIn: parent
        text: root.label
        color: "#edf1ea"
        font.pixelSize: 13
        font.weight: Font.Medium
    }
}
