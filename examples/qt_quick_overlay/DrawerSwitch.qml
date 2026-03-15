import QtQuick 2.15

Rectangle {
    id: root
    property string label: ""
    property bool value: false
    signal activated()

    height: 40
    radius: 14
    color: root.value ? "#2e382e" : "#342728"
    border.width: 1
    border.color: root.value ? "#3d4a3d" : "#4a383a"

    Text {
        text: root.label
        color: "#d1c5aa"
        font.pixelSize: 13
        font.weight: Font.Medium
        anchors.left: parent.left
        anchors.leftMargin: 16
        anchors.verticalCenter: parent.verticalCenter
    }

    Rectangle {
        width: 42
        height: 24
        radius: 12
        color: root.value ? "#5c7a45" : "#594a44"
        anchors.right: parent.right
        anchors.rightMargin: 10
        anchors.verticalCenter: parent.verticalCenter

        Rectangle {
            width: 18
            height: 18
            radius: 9
            y: 3
            x: root.value ? parent.width - width - 3 : 3
            color: "#f4ede4"

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