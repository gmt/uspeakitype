import QtQuick 2.15
import QtQuick.Window 2.15

Window {
    id: root
    width: 1240
    height: 780
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
                        label: root.controlDrawerOpen ? "Dock Controls" : "Open Controls"
                        onActivated: root.controlDrawerOpen = !root.controlDrawerOpen
                    }
                }

                SpectrogramViewport {
                    id: viewport
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.topMargin: 62
                    anchors.bottom: transcript.top
                    anchors.leftMargin: 14
                    anchors.rightMargin: 14
                    anchors.bottomMargin: 14
                    waterfallMode: root.showWaterfall
                    energyLevel: root.fakeLevel
                }

                TranscriptPanel {
                    id: transcript
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.margins: 18
                    height: 132
                    committedText: "the shell should feel deliberate while the renderer stays free to be specialized"
                    partialText: "and the viewport seam ought to be obvious without looking temporary"
                    footerText: "Requested: moonshine-voice   Active: moonshine-base   Surface: Rust/WGPU seam   Download: idle"
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
