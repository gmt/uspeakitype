import QtQuick 2.15

Rectangle {
    id: root
    property string label: ""
    property bool value: false
    signal activated()

    height: 32
    radius: 16
    color: root.value ? "#2e382e" : "#342728"
    border.width: 1
    border.color: root.value ? "#3d4a3d" : "#4a383a"

    Text {
        text: root.label
        color: "#dcd2b8"
        font.pixelSize: 13
        font.weight: Font.Medium
        anchors.left: parent.left
        anchors.leftMargin: 16
        anchors.verticalCenter: parent.verticalCenter
    }

    Rectangle {
        width: 38
        height: 20
        radius: 10
        color: root.value ? "#5c7a45" : "#594a44"
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter

        Rectangle {
            width: 14
            height: 14
            radius: 7
            y: 3
            x: root.value ? parent.width - width - 3 : 3
            color: "#e8deca"

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