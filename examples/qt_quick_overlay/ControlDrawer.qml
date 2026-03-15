import QtQuick 2.15

Rectangle {
    id: root
    property bool expanded: true
    property bool paused: false
    property bool listening: true
    property bool injectionEnabled: true
    property bool waterfallMode: false

    signal togglePaused()
    signal toggleListening()
    signal toggleInjection()
    signal toggleWaterfall()
    signal toggleExpanded()

    radius: 18
    color: "#12181c"
    border.width: 1
    border.color: "#233740"
    clip: true

    Rectangle {
        id: topSection
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: root.expanded ? 90 : parent.height
        color: "#18242b"
        radius: 18
        
        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: 18
            color: "#18242b"
            visible: root.expanded
        }
    }

    Item {
        id: headerArea
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: 52
        
        Text {
            id: title
            visible: root.expanded
            text: "Controls"
            color: "#f1f3ed"
            font.pixelSize: 18
            font.weight: Font.DemiBold
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            anchors.leftMargin: 16
        }

        ActionButton {
            id: collapseButton
            label: root.expanded ? "Collapse" : "Open"
            anchors.right: root.expanded ? parent.right : undefined
            anchors.horizontalCenter: root.expanded ? undefined : parent.horizontalCenter
            anchors.verticalCenter: parent.verticalCenter
            anchors.rightMargin: root.expanded ? 12 : 0
            onActivated: root.toggleExpanded()
        }
    }

    Text {
        id: descriptionText
        visible: root.expanded
        text: "Qt Quick handles shell chrome while the spectrogram is WGPU."
        color: "#9dafb2"
        font.pixelSize: 12
        wrapMode: Text.WordWrap
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: headerArea.bottom
        anchors.leftMargin: 16
        anchors.rightMargin: 16
    }

    Column {
        visible: root.expanded
        spacing: 8
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: descriptionText.bottom
        anchors.margins: 12
        anchors.topMargin: 16

        DrawerSwitch {
            width: parent.width
            label: "Paused"
            value: root.paused
            onActivated: root.togglePaused()
        }

        DrawerSwitch {
            width: parent.width
            label: "Listening"
            value: root.listening
            onActivated: root.toggleListening()
        }

        DrawerSwitch {
            width: parent.width
            label: "Injection"
            value: root.injectionEnabled
            onActivated: root.toggleInjection()
        }

        DrawerSwitch {
            width: parent.width
            label: "Waterfall"
            value: root.waterfallMode
            onActivated: root.toggleWaterfall()
        }
    }
}