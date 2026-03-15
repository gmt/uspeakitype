import QtQuick 2.15
import QtQuick.Window 2.15

Window {
    id: root
    width: 1040
    height: 340
    visible: true
    color: "#0b1216"
    title: "usit Qt Quick Overlay Concept"

    property bool controlDrawerOpen: true
    property bool listening: true
    property bool injectionEnabled: true
    property bool paused: false
    property bool showWaterfall: true
    property real fakeLevel: 0.62

    Timer {
        interval: 110
        running: true
        repeat: true
        onTriggered: root.fakeLevel = 0.22 + Math.random() * 0.68
    }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: "#0b1216" }
            GradientStop { position: 0.45; color: "#0f1b21" }
            GradientStop { position: 1.0; color: "#141718" }
        }
    }

    Rectangle {
        width: 460
        height: 460
        radius: width / 2
        color: "#204e52"
        opacity: 0.16
        x: parent.width * 0.58
        y: -150
    }

    Rectangle {
        width: 390
        height: 390
        radius: width / 2
        color: "#d36b35"
        opacity: 0.12
        x: -100
        y: parent.height - 260
    }

    Item {
        id: content
        anchors.fill: parent
        anchors.margins: 16

        Item {
            id: leftColumn
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            anchors.right: drawer.left
            anchors.rightMargin: 12

            Rectangle {
                id: header
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                height: 52
                radius: 18
                color: "#152129"
                border.width: 1
                border.color: "#27414d"

                Text {
                    id: titleText
                    text: "usit"
                    color: "#f4f5ef"
                    font.pixelSize: 24
                    font.weight: Font.DemiBold
                    anchors.left: parent.left
                    anchors.leftMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                }

                Row {
                    anchors.left: titleText.right
                    anchors.leftMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8

                    StatusChip {
                        label: root.listening ? "Listening" : "Idle"
                        tint: root.listening ? "#a25a2a" : "#39444a"
                    }

                    StatusChip {
                        label: root.injectionEnabled ? "Injecting" : "Display Only"
                        tint: root.injectionEnabled ? "#3d6f50" : "#4a4340"
                    }
                }

                Rectangle {
                    id: recordButton
                    width: 36
                    height: 36
                    radius: 18
                    color: root.paused ? "#1e262b" : "#8c3b3b"
                    border.width: 2
                    border.color: root.paused ? "#334550" : "#b04b4b"
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.verticalCenter: parent.verticalCenter

                    Rectangle {
                        width: 12
                        height: 12
                        radius: root.paused ? 6 : 3
                        color: "#f4f5ef"
                        anchors.centerIn: parent

                        Behavior on radius { NumberAnimation { duration: 150; easing.type: Easing.OutCubic } }
                        Behavior on color { ColorAnimation { duration: 150 } }
                    }

                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.paused = !root.paused
                        hoverEnabled: true
                        onEntered: recordButton.scale = 1.05
                        onExited: recordButton.scale = 1.0
                    }

                    Behavior on scale { NumberAnimation { duration: 100 } }
                    Behavior on color { ColorAnimation { duration: 150 } }
                    Behavior on border.color { ColorAnimation { duration: 150 } }
                }

                Text {
                    text: "Moonshine Voice bridge candidate"
                    color: "#9fb3b7"
                    font.pixelSize: 13
                    anchors.right: parent.right
                    anchors.rightMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                }
            }

            Rectangle {
                id: shell
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: header.bottom
                anchors.topMargin: 12
                anchors.bottom: parent.bottom
                radius: 18
                color: "#11181d"
                border.width: 1
                border.color: "#22343d"

                Text {
                    id: shellTitle
                    text: "Spectrogram Surface"
                    color: "#edf1ea"
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                    anchors.left: parent.left
                    anchors.leftMargin: 16
                    anchors.top: parent.top
                    anchors.topMargin: 12
                }

                Row {
                    id: shellActions
                    anchors.right: parent.right
                    anchors.rightMargin: 16
                    anchors.verticalCenter: shellTitle.verticalCenter
                    spacing: 8

                    ActionButton {
                        label: root.showWaterfall ? "Waterfall" : "Bars"
                        onActivated: root.showWaterfall = !root.showWaterfall
                    }

                    ActionButton {
                        label: root.controlDrawerOpen ? "Dock Controls" : "Open Controls"
                        onActivated: root.controlDrawerOpen = !root.controlDrawerOpen
                    }
                }

                SpectrogramViewport {
                    id: viewport
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: shellTitle.bottom
                    anchors.topMargin: 12
                    anchors.bottom: transcript.top
                    anchors.leftMargin: 12
                    anchors.rightMargin: 12
                    anchors.bottomMargin: 12
                    waterfallMode: root.showWaterfall
                    energyLevel: root.fakeLevel
                }

                TranscriptPanel {
                    id: transcript
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.margins: 12
                    height: 84
                    committedText: "the shell should feel deliberate while the renderer stays free to be specialized"
                    partialText: "and the viewport seam ought to be obvious without looking temporary"
                }
            }
        }

        ControlDrawer {
            id: drawer
            width: root.controlDrawerOpen ? 260 : 84
            anchors.top: parent.top
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            expanded: root.controlDrawerOpen
            paused: root.paused
            listening: root.listening
            injectionEnabled: root.injectionEnabled
            waterfallMode: root.showWaterfall

            onTogglePaused: root.paused = !root.paused
            onToggleListening: root.listening = !root.listening
            onToggleInjection: root.injectionEnabled = !root.injectionEnabled
            onToggleWaterfall: root.showWaterfall = !root.showWaterfall
            onToggleExpanded: root.controlDrawerOpen = !root.controlDrawerOpen
        }
    }
}