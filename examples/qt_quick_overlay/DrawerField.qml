import QtQuick 2.15

Rectangle {
    id: root
    property string label: ""
    property string value: ""
    property string detail: ""
    property color accent: "#d39d4c"
    signal activated()

    height: 72
    radius: 18
    color: "#130d0f"
    border.width: 1
    border.color: "#4a3932"

    Rectangle {
        width: 4
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.margins: 10
        radius: 2
        color: root.accent
        opacity: 0.75
    }

    Column {
        anchors.left: parent.left
        anchors.right: chevron.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.leftMargin: 18
        anchors.rightMargin: 12
        anchors.topMargin: 12
        anchors.bottomMargin: 12
        spacing: 4

        Text {
            text: root.label
            color: "#b59f95"
            font.pixelSize: 12
            font.letterSpacing: 0.9
        }

        Text {
            text: root.value
            color: "#f4ece2"
            font.pixelSize: 16
            font.weight: Font.Medium
            elide: Text.ElideRight
        }

        Text {
            visible: root.detail.length > 0
            text: root.detail
            color: "#9f8e87"
            font.pixelSize: 11
            elide: Text.ElideRight
        }
    }

    Text {
        id: chevron
        text: ">"
        color: "#d3c2b0"
        font.pixelSize: 18
        anchors.right: parent.right
        anchors.rightMargin: 16
        anchors.verticalCenter: parent.verticalCenter
    }

    MouseArea {
        anchors.fill: parent
        onClicked: root.activated()
        cursorShape: Qt.PointingHandCursor
    }
}
