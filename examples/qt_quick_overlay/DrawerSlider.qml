import QtQuick 2.15

Rectangle {
    id: root
    property string label: ""
    property string detail: ""
    property real value: 0.5
    property color accent: "#d39d4c"
    signal valueEdited(real nextValue)

    function clamp(v) {
        return Math.max(0.0, Math.min(1.0, v))
    }

    function updateFromMouse(xPos) {
        const usable = groove.width - knob.width
        const local = Math.max(0, Math.min(usable, xPos - knob.width / 2))
        root.valueEdited(root.clamp(usable > 0 ? local / usable : 0))
    }

    height: 78
    radius: 18
    color: "#130d0f"
    border.width: 1
    border.color: "#483832"

    Column {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.margins: 14
        spacing: 10

        Row {
            width: parent.width
            spacing: 10

            Text {
                text: root.label
                color: "#f4ece2"
                font.pixelSize: 15
                font.weight: Font.Medium
            }

            Item {
                width: 1
                height: 1
            }

            Text {
                text: Math.round(root.value * 100) + "%"
                color: "#d8c7b7"
                font.pixelSize: 13
                anchors.verticalCenter: parent.verticalCenter
            }
        }

        Item {
            width: parent.width
            height: 20

            Rectangle {
                id: groove
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                height: 8
                radius: 4
                color: "#2b1e21"
                border.width: 1
                border.color: "#523f37"

                Rectangle {
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    width: root.value * parent.width
                    radius: parent.radius
                    color: root.accent
                    opacity: 0.9
                }
            }

            Rectangle {
                id: knob
                width: 18
                height: 18
                radius: 9
                color: "#f4ece2"
                border.width: 1
                border.color: "#8f7869"
                anchors.verticalCenter: groove.verticalCenter
                x: root.value * Math.max(0, groove.width - width)
            }

            MouseArea {
                anchors.fill: parent
                onPressed: root.updateFromMouse(mouse.x)
                onPositionChanged: if (pressed) root.updateFromMouse(mouse.x)
                cursorShape: Qt.PointingHandCursor
            }
        }

        Text {
            visible: root.detail.length > 0
            text: root.detail
            color: "#9f8e87"
            font.pixelSize: 11
            wrapMode: Text.WordWrap
        }
    }
}
