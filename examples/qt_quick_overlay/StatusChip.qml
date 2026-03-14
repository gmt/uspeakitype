import QtQuick 2.15

Rectangle {
    id: root
    property string label: ""
    property color tint: "#2e6f68"

    radius: 13
    color: Qt.rgba(root.tint.r * 0.4, root.tint.g * 0.34, root.tint.b * 0.34, 0.36)
    border.width: 1
    border.color: Qt.rgba(root.tint.r, root.tint.g, root.tint.b, 0.82)
    height: 30
    width: chipLabel.implicitWidth + 24

    Text {
        id: chipLabel
        anchors.centerIn: parent
        text: root.label
        color: "#f3ebe1"
        font.pixelSize: 13
        font.weight: Font.Medium
    }
}
