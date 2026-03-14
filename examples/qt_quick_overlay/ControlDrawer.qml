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

    radius: 40
    color: "#100b0d"
    border.width: 1
    border.color: "#4a3a33"
    clip: true

    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: 126
        gradient: Gradient {
            GradientStop { position: 0.0; color: "#24181a" }
            GradientStop { position: 1.0; color: "#130d0f" }
        }
    }

    Rectangle {
        width: parent.width * 0.42
        height: parent.height + 120
        radius: width / 2
        color: "#6f4f6c"
        opacity: 0.08
        x: parent.width * 0.34
        y: -60
        rotation: 6
    }

    Text {
        id: title
        visible: root.expanded
        text: "Control Dock"
        color: "#f4ece2"
        font.pixelSize: 22
        font.weight: Font.DemiBold
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.margins: 16
    }

    ActionButton {
        id: collapseButton
        label: root.expanded ? "Collapse" : "Controls"
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.margins: 16
        anchors.leftMargin: 16
        anchors.rightMargin: 16
        onActivated: root.toggleExpanded()
    }

    Text {
        visible: root.expanded
        text: "Qt Quick can handle the shell, drawer, and transcript chrome while the spectrogram remains a dedicated surface."
        color: "#b6a39a"
        font.pixelSize: 15
        wrapMode: Text.WordWrap
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: collapseButton.bottom
        anchors.margins: 16
        anchors.topMargin: 14
    }

    Column {
        visible: root.expanded
        spacing: 12
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.leftMargin: 16
        anchors.rightMargin: 16
        anchors.topMargin: 142

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

    Rectangle {
        visible: root.expanded
        height: 1
        color: "#43332e"
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.topMargin: 390
        anchors.leftMargin: 16
        anchors.rightMargin: 16
    }

    Text {
        visible: root.expanded
        text: "Immediate Wins"
        color: "#f4ece2"
        font.pixelSize: 18
        font.weight: Font.Medium
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.leftMargin: 16
        anchors.topMargin: 408
    }

    Text {
        visible: root.expanded
        text: "- proper layout primitives\n- polished drawer and sheet behavior\n- stronger focus and hover affordances\n- less custom panel geometry and hit-testing"
        color: "#b6a39a"
        font.pixelSize: 14
        wrapMode: Text.WordWrap
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.leftMargin: 16
        anchors.rightMargin: 16
        anchors.topMargin: 438
    }

    Rectangle {
        visible: root.expanded
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 16
        height: 88
        radius: 26
        color: "#191113"
        border.width: 1
        border.color: "#6f4f6c"

        Text {
            text: "Bridge Risk"
            color: "#f0ceb8"
            font.pixelSize: 15
            font.weight: Font.DemiBold
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.margins: 14
        }

        Text {
            text: "This only solves container chrome. Input injection, Wayland embedding, and renderer ownership still need a real bridge."
            color: "#d3c5bf"
            font.pixelSize: 13
            wrapMode: Text.WordWrap
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            anchors.margins: 14
            anchors.topMargin: 34
        }
    }
}
