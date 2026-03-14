import QtQuick 2.15
import QtQuick.Window 2.15

Window {
    id: root
    width: 1240
    height: 780
    visible: true
    color: "#090707"
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
            GradientStop { position: 0.0; color: "#080607" }
            GradientStop { position: 0.38; color: "#0f0a0d" }
            GradientStop { position: 0.72; color: "#130f12" }
            GradientStop { position: 1.0; color: "#090707" }
        }
    }

    Rectangle {
        width: 840
        height: 230
        radius: height / 2
        color: "#5b4760"
        opacity: 0.12
        x: parent.width * 0.42
        y: 38
        rotation: -8
    }

    Rectangle {
        width: 760
        height: 210
        radius: height / 2
        color: "#71352d"
        opacity: 0.14
        x: -170
        y: parent.height - 250
        rotation: 11
    }

    Rectangle {
        width: 620
        height: 110
        radius: height / 2
        color: "#d39d4c"
        opacity: 0.08
        x: parent.width * 0.26
        y: parent.height * 0.22
        rotation: -4
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
                radius: 30
                color: "#110d0f"
                border.width: 1
                border.color: "#4a382f"
                clip: true

                Rectangle {
                    anchors.fill: parent
                    radius: parent.radius
                    gradient: Gradient {
                        GradientStop { position: 0.0; color: "#24181a" }
                        GradientStop { position: 0.45; color: "#151012" }
                        GradientStop { position: 1.0; color: "#0f0b0c" }
                    }
                    opacity: 0.96
                }

                Rectangle {
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    width: parent.width * 0.2
                    radius: width / 2
                    color: "#d39d4c"
                    opacity: 0.08
                    x: parent.width * 0.36
                }

                Text {
                    text: "usit"
                    color: "#f5eee5"
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
                        tint: root.paused ? "#6b3f38" : "#b07c3f"
                    }

                    StatusChip {
                        label: root.listening ? "Listening" : "Idle"
                        tint: root.listening ? "#8d4f34" : "#564550"
                    }

                    StatusChip {
                        label: root.injectionEnabled ? "Injecting" : "Display Only"
                        tint: root.injectionEnabled ? "#7b865f" : "#4d4038"
                    }
                }

                Text {
                    text: "Moonshine Voice bridge candidate"
                    color: "#b7a59c"
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
                radius: 48
                color: "#100b0d"
                border.width: 1
                border.color: "#4a3a33"
                clip: true

                Rectangle {
                    anchors.fill: parent
                    radius: parent.radius
                    gradient: Gradient {
                        GradientStop { position: 0.0; color: "#231619" }
                        GradientStop { position: 0.35; color: "#120d0f" }
                        GradientStop { position: 0.78; color: "#0d090a" }
                        GradientStop { position: 1.0; color: "#181115" }
                    }
                    opacity: 0.98
                }

                Rectangle {
                    width: parent.width * 0.18
                    height: parent.height + 120
                    radius: width / 2
                    color: "#d39d4c"
                    opacity: 0.06
                    x: parent.width * 0.56
                    y: -60
                    rotation: 6
                }

                Rectangle {
                    width: parent.width * 0.14
                    height: parent.height + 160
                    radius: width / 2
                    color: "#6f4f6c"
                    opacity: 0.1
                    x: parent.width * 0.7
                    y: -78
                    rotation: -4
                }

                Rectangle {
                    width: parent.width * 0.12
                    height: parent.height + 140
                    radius: width / 2
                    color: "#7a332d"
                    opacity: 0.08
                    x: parent.width * 0.18
                    y: -70
                    rotation: -7
                }

                Text {
                    text: "Spectrogram Surface"
                    color: "#f3ebe1"
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
            width: root.controlDrawerOpen ? 304 : 90
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
