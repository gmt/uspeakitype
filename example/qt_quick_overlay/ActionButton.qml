import QtQuick 2.15

Rectangle {
    id: root
    property string label: ""
    signal activated()

    width: buttonLabel.implicitWidth + 24
    height: 28
    radius: 12
    color: "#2b211d"
    border.width: 1
    border.color: "#4a3831"

    Text {
        id: buttonLabel
        anchors.centerIn: parent
        text: root.label
        color: "#f0eadf"
        font.pixelSize: 12
        font.weight: Font.Medium
    }

    MouseArea {
        anchors.fill: parent
        onClicked: root.activated()
        cursorShape: Qt.PointingHandCursor
    }
}