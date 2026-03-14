import QtQuick 2.15

Rectangle {
    id: root
    property string committedText: ""
    property string partialText: ""
    property string footerText: ""

    radius: 32
    color: "#120d0e"
    border.width: 1
    border.color: "#4a3a33"
    clip: true

    Rectangle {
        anchors.fill: parent
        radius: parent.radius
        gradient: Gradient {
            GradientStop { position: 0.0; color: "#1b1315" }
            GradientStop { position: 0.52; color: "#110c0d" }
            GradientStop { position: 1.0; color: "#171012" }
        }
    }

    Rectangle {
        width: parent.width * 0.26
        height: parent.height + 60
        radius: width / 2
        color: "#6f4f6c"
        opacity: 0.08
        x: parent.width * 0.58
        y: -30
        rotation: 5
    }

    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: 3
        color: "#d39d4c"
        opacity: 0.55
    }

    Text {
        text: "Transcript"
        color: "#b8a39a"
        font.pixelSize: 13
        font.letterSpacing: 1.2
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.leftMargin: 18
        anchors.topMargin: 12
    }

    Text {
        id: committed
        text: root.committedText
        color: "#f3ece2"
        font.pixelSize: 22
        font.weight: Font.Medium
        wrapMode: Text.WordWrap
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: 18
        anchors.topMargin: 34
    }

    Text {
        id: partial
        text: root.partialText
        color: "#c2b3a7"
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
        color: "#43332e"
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
        color: "#a39189"
        font.pixelSize: 13
        elide: Text.ElideRight
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 18
    }
}
