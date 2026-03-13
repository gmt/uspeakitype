import QtQuick 2.15

Rectangle {
    id: root
    property string committedText: ""
    property string partialText: ""
    property string footerText: ""

    radius: 22
    color: "#1a1d1d"
    border.width: 1
    border.color: "#304048"

    Text {
        id: committed
        text: root.committedText
        color: "#f3f4ee"
        font.pixelSize: 22
        font.weight: Font.Medium
        wrapMode: Text.WordWrap
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: 18
    }

    Text {
        id: partial
        text: root.partialText
        color: "#aeb6b4"
        font.pixelSize: 21
        wrapMode: Text.WordWrap
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: committed.bottom
        anchors.leftMargin: 18
        anchors.rightMargin: 18
        anchors.topMargin: 10
    }

    Rectangle {
        height: 1
        color: "#2e3b42"
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: footer.top
        anchors.leftMargin: 18
        anchors.rightMargin: 18
        anchors.bottomMargin: 10
    }

    Text {
        id: footer
        text: root.footerText
        color: "#8b9ca1"
        font.pixelSize: 13
        elide: Text.ElideRight
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 18
    }
}
