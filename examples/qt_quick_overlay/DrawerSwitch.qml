import QtQuick 2.15

Rectangle {
    id: root
    property string label: ""
    property bool value: false
    signal activated()

    height: 44
    radius: 12
    color: root.value ? "#20342f" : "#171e22"
    border.width: 1
    border.color: root.value ? "#3f8b70" : "#2d3a42"

    Text {
        text: root.label
        color: "#edf1eb"
        font.pixelSize: 14
        font.weight: Font.Medium
        anchors.left: parent.left
        anchors.leftMargin: 12
        anchors.verticalCenter: parent.verticalCenter
    }

    Rectangle {
        width: 44
        height: 24
        radius: 12
        color: root.value ? "#3b8e74" : "#4b5459"
        anchors.right: parent.right
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter

        Rectangle {
            width: 18
            height: 18
            radius: 9
            y: 3
            x: root.value ? parent.width - width - 3 : 3
            color: "#f4f4ee"

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