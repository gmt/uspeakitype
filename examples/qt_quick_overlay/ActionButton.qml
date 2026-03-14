import QtQuick 2.15

Rectangle {
    id: root
    property string label: ""
    signal activated()

    width: buttonLabel.implicitWidth + 28
    height: 34
    radius: 14
    color: "#170f11"
    border.width: 1
    border.color: "#6a4a38"

    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: parent.height * 0.48
        radius: parent.radius
        color: "#d39d4c"
        opacity: 0.08
    }

    Text {
        id: buttonLabel
        anchors.centerIn: parent
        text: root.label
        color: "#f3ebe1"
        font.pixelSize: 14
        font.weight: Font.Medium
    }

    MouseArea {
        anchors.fill: parent
        onClicked: root.activated()
        cursorShape: Qt.PointingHandCursor
    }
}
