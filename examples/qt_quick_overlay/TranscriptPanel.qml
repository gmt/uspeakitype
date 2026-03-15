import QtQuick 2.15

Rectangle {
    id: root
    property string committedText: ""
    property string partialText: ""

    radius: 16
    color: "#1a1d1d"
    border.width: 1
    border.color: "#304048"

    Rectangle {
        anchors.fill: parent
        radius: parent.radius
        gradient: Gradient {
            GradientStop { position: 0.0; color: "#1b1f20" }
            GradientStop { position: 1.0; color: "#171a1b" }
        }
    }

    Text {
        id: transcriptLabel
        text: "Transcript"
        color: "#90a1a5"
        font.pixelSize: 12
        font.letterSpacing: 1.0
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.leftMargin: 14
        anchors.topMargin: 8
    }

    Text {
        id: committed
        text: root.committedText
        color: "#f3f4ee"
        font.pixelSize: 16
        font.weight: Font.Medium
        wrapMode: Text.WordWrap
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: transcriptLabel.bottom
        anchors.leftMargin: 14
        anchors.rightMargin: 14
        anchors.topMargin: 4
    }

    Text {
        id: partial
        text: root.partialText
        color: "#aeb6b4"
        font.pixelSize: 16
        wrapMode: Text.WordWrap
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: committed.bottom
        anchors.leftMargin: 14
        anchors.rightMargin: 14
        anchors.topMargin: 2
    }
}