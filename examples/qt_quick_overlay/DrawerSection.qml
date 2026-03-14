import QtQuick 2.15

Rectangle {
    id: root
    property string title: ""
    property string subtitle: ""
    default property alias sectionContent: contentColumn.data

    radius: 24
    color: "#171012"
    border.width: 1
    border.color: "#473730"
    clip: true

    Rectangle {
        anchors.fill: parent
        radius: parent.radius
        gradient: Gradient {
            GradientStop { position: 0.0; color: "#1e1416" }
            GradientStop { position: 0.52; color: "#140e10" }
            GradientStop { position: 1.0; color: "#110c0d" }
        }
    }

    Rectangle {
        width: parent.width * 0.34
        height: parent.height + 34
        radius: width / 2
        color: "#6f4f6c"
        opacity: 0.06
        x: parent.width * 0.54
        y: -18
        rotation: 5
    }

    Column {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        Column {
            spacing: 4

            Text {
                text: root.title
                color: "#f4ece2"
                font.pixelSize: 15
                font.weight: Font.DemiBold
            }

            Text {
                visible: root.subtitle.length > 0
                text: root.subtitle
                color: "#af9c93"
                font.pixelSize: 12
                wrapMode: Text.WordWrap
            }
        }

        Column {
            id: contentColumn
            spacing: 10
            width: parent.width
        }
    }
}
