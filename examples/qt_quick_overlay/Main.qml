import QtQuick 2.15
import QtQuick.Window 2.15

Window {
    id: root
    width: 1180
    height: 760
    visible: true
    color: "#0b1216"
    title: "usit Qt Quick Overlay Concept"

    property bool controlDrawerOpen: true
    property bool listening: true
    property bool injectionEnabled: true
    property bool paused: false
    property bool showWaterfall: false
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
        width: 420
        height: 420
        radius: width / 2
        color: "#1f4b4d"
        opacity: 0.18
        x: parent.width * 0.63
        y: -120
    }

    Rectangle {
        width: 360
        height: 360
        radius: width / 2
        color: "#d36b35"
        opacity: 0.10
        x: -80
        y: parent.height - 240
    }

    Item {
        id: content
        anchors.fill: parent
        anchors.margins: 24

        Item {
            id: leftColumn
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            anchors.right: drawer.left
            anchors.rightMargin: 20

            Rectangle {
                id: header
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                height: 64
                radius: 22
                color: "#152129"
                border.width: 1
                border.color: "#27414d"

                Text {
                    text: "usit"
                    color: "#f4f5ef"
                    font.pixelSize: 28
                    font.weight: Font.DemiBold
                    anchors.left: parent.left
                    anchors.leftMargin: 18
                    anchors.verticalCenter: parent.verticalCenter
                }

                Row {
                    anchors.left: parent.left
                    anchors.leftMargin: 94
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 10

                    StatusChip {
                        label: root.paused ? "Paused" : "Live"
                        tint: root.paused ? "#7b5050" : "#2e6f68"
                    }

                    StatusChip {
                        label: root.listening ? "Listening" : "Idle"
                        tint: root.listening ? "#a25a2a" : "#39444a"
                    }

                    StatusChip {
                        label: root.injectionEnabled ? "Injecting" : "Display Only"
                        tint: root.injectionEnabled ? "#3d6f50" : "#4a4340"
                    }
                }

                Text {
                    text: "Moonshine Voice bridge candidate"
                    color: "#9fb3b7"
                    font.pixelSize: 15
                    anchors.right: parent.right
                    anchors.rightMargin: 18
                    anchors.verticalCenter: parent.verticalCenter
                }
            }

            Rectangle {
                id: shell
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: header.bottom
                anchors.topMargin: 16
                anchors.bottom: parent.bottom
                radius: 28
                color: "#11181d"
                border.width: 1
                border.color: "#22343d"

                Text {
                    text: "Spectrogram Surface"
                    color: "#edf1ea"
                    font.pixelSize: 24
                    font.weight: Font.DemiBold
                    anchors.left: parent.left
                    anchors.leftMargin: 18
                    anchors.top: parent.top
                    anchors.topMargin: 18
                }

                Row {
                    anchors.right: parent.right
                    anchors.rightMargin: 18
                    anchors.top: parent.top
                    anchors.topMargin: 16
                    spacing: 10

                    ActionButton {
                        label: root.showWaterfall ? "Waterfall" : "Bars"
                        onActivated: root.showWaterfall = !root.showWaterfall
                    }

                    ActionButton {
                        label: root.controlDrawerOpen ? "Hide Controls" : "Show Controls"
                        onActivated: root.controlDrawerOpen = !root.controlDrawerOpen
                    }
                }

                SpectrogramViewport {
                    id: viewport
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.topMargin: 64
                    anchors.bottom: transcript.top
                    anchors.leftMargin: 18
                    anchors.rightMargin: 18
                    anchors.bottomMargin: 16
                    waterfallMode: root.showWaterfall
                    energyLevel: root.fakeLevel
                }

                TranscriptPanel {
                    id: transcript
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.margins: 18
                    height: 122
                    committedText: "the control panel is fighting the renderer less when it is clearly a separate thing"
                    partialText: "and that seems promising"
                    footerText: "Requested: moonshine-voice   Active: moonshine-base   Download: idle"
                }
            }
        }

        ControlDrawer {
            id: drawer
            width: root.controlDrawerOpen ? 292 : 84
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
