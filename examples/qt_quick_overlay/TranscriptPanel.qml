import QtQuick 2.15

Rectangle {
    id: root
    property string committedText: ""
    property string partialText: ""

    radius: 16
    color: "#1c1613"
    border.width: 1
    border.color: "#42322a"

    Rectangle {
        anchors.fill: parent
        radius: parent.radius
        gradient: Gradient {
            GradientStop { position: 0.0; color: "#241c18" }
            GradientStop { position: 1.0; color: "#1c1613" }
        }
    }

    Text {
        id: committed
        text: root.committedText
        color: "#e8deca"
        font.pixelSize: 16
        font.weight: Font.Medium
        wrapMode: Text.WordWrap
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.leftMargin: 16
        anchors.rightMargin: 16
        anchors.topMargin: 10
    }

    Text {
        id: partial
        text: root.partialText
        color: "#c2b8a3"
        font.pixelSize: 16
        wrapMode: Text.WordWrap
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: committed.bottom
        anchors.leftMargin: 16
        anchors.rightMargin: 16
        anchors.topMargin: 4
    }
}