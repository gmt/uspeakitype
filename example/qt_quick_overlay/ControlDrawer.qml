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
    color: "#1a1411"
    border.width: 1
    border.color: "#362721"
    clip: true

    Rectangle {
        id: topSection
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: root.expanded ? 48 : parent.height
        color: "#211915"
        radius: 18
        
        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: 18
            color: "#211915"
            visible: root.expanded
        }
    }

    Item {
        id: headerArea
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: 48

        Text {
            id: title
            visible: root.expanded
            text: "Control Panel"
            color: "#e2d8bd"
            font.pixelSize: 18
            font.weight: Font.DemiBold
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            anchors.leftMargin: 16
        }

        ActionButton {
            id: collapseButton
            label: "Close"
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            anchors.rightMargin: 12
            onActivated: root.toggleExpanded()
        }
    }

    Column {
        visible: root.expanded
        spacing: 10
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: headerArea.bottom
        anchors.margins: 16
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