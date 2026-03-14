import QtQuick 2.15

Rectangle {
    id: root
    property string label: ""
    property bool value: false
    signal activated()

    height: 54
    radius: 16
    color: root.value ? "#241718" : "#171012"
    border.width: 1
    border.color: root.value ? "#7a332d" : "#43332e"

    Text {
        text: root.label
        color: "#f4ece2"
        font.pixelSize: 16
        font.weight: Font.Medium
        anchors.left: parent.left
        anchors.leftMargin: 14
        anchors.verticalCenter: parent.verticalCenter
    }

    Rectangle {
        width: 54
        height: 28
        radius: 14
        color: root.value ? "#af7b3f" : "#4d403b"
        anchors.right: parent.right
        anchors.rightMargin: 14
        anchors.verticalCenter: parent.verticalCenter

        Rectangle {
            width: 22
            height: 22
            radius: 11
            y: 3
            x: root.value ? parent.width - width - 3 : 3
            color: "#f5eee4"

            Behavior on x {
                NumberAnimation { duration: 120; easing.type: Easing.OutCubic }
            }
        }
    }

    MouseArea {
        anchors.fill: parent
        onClicked: root.activated()
        cursorShape: Qt.PointingHandCursor
    }
}
