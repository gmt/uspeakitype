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

    Column {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        anchors.margins: 16
        spacing: 4

        Text {
            id: committed
            text: root.committedText
            color: "#e8deca"
            font.pixelSize: 16
            font.weight: Font.Medium
            wrapMode: Text.WordWrap
            width: parent.width
        }

        Text {
            id: partial
            text: root.partialText
            color: "#c2b8a3"
            font.pixelSize: 16
            wrapMode: Text.WordWrap
            width: parent.width
        }
    }
}