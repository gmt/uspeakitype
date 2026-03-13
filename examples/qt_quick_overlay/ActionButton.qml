import QtQuick 2.15

Rectangle {
    id: root
    property string label: ""
    signal activated()

    width: buttonLabel.implicitWidth + 28
    height: 34
    radius: 14
    color: "#203039"
    border.width: 1
    border.color: "#35505d"

    Text {
        id: buttonLabel
        anchors.centerIn: parent
        text: root.label
        color: "#f0f3ee"
        font.pixelSize: 14
        font.weight: Font.Medium
    }

    MouseArea {
        anchors.fill: parent
        onClicked: root.activated()
        cursorShape: Qt.PointingHandCursor
    }
}
