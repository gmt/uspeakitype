import QtQuick 2.15
import QtQuick.Window 2.15

Window {
    id: root
    width: 1080
    height: 320
    visible: true
    color: "#140c0a"
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
            GradientStop { position: 0.0; color: "#140c0a" }
            GradientStop { position: 0.45; color: "#1a120d" }
            GradientStop { position: 1.0; color: "#1c1511" }
        }
    }

    Rectangle {
        width: 460
        height: 460
        radius: width / 2
        color: "#2e4128"
        opacity: 0.16
        x: parent.width * 0.58
        y: -150
    }

    Rectangle {
        width: 390
        height: 390
        radius: width / 2
        color: "#8c3b28"
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
            anchors.right: root.controlDrawerOpen ? drawer.left : parent.right
            anchors.rightMargin: root.controlDrawerOpen ? 16 : 0

            Rectangle {
                id: header
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                height: 56
                radius: 18
                color: "#1c1511"
                border.width: 1
                border.color: "#3d2d25"

                Text {
                    id: titleText
                    text: "usit"
                    color: "#f4f0e6"
                    font.pixelSize: 24
                    font.weight: Font.DemiBold
                    anchors.left: parent.left
                    anchors.leftMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                }

                Row {
                    id: headerLeft
                    anchors.left: titleText.right
                    anchors.leftMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 12

                    DrawerSwitch {
                        width: 136
                        label: "Listening"
                        value: root.listening
                        onActivated: root.listening = !root.listening
                    }

                    DrawerSwitch {
                        width: 136
                        label: "Injecting"
                        value: root.injectionEnabled
                        onActivated: root.injectionEnabled = !root.injectionEnabled
                    }
                }

                Item {
                    anchors.left: headerLeft.right
                    anchors.right: headerActions.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom

                    Rectangle {
                        id: recordButton
                        width: 40
                        height: 40
                        radius: 20
                        color: root.paused ? "#2b201c" : "#8c3b28"
                        border.width: 2
                        border.color: root.paused ? "#4a352d" : "#b04b36"
                        anchors.centerIn: parent

                        Rectangle {
                            width: 14
                            height: 14
                            radius: root.paused ? 7 : 3
                            color: "#f4efe6"
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
                }

                Row {
                    id: headerActions
                    anchors.right: parent.right
                    anchors.rightMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 12

                    DrawerSwitch {
                        width: 136
                        label: root.showWaterfall ? "Waterfall" : "Bars"
                        value: root.showWaterfall
                        onActivated: root.showWaterfall = !root.showWaterfall
                    }

                    DrawerSwitch {
                        width: 136
                        label: "Control"
                        value: root.controlDrawerOpen
                        onActivated: root.controlDrawerOpen = !root.controlDrawerOpen
                    }
                }
            }

            Rectangle {
                id: shell
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: header.bottom
                anchors.topMargin: 16
                anchors.bottom: parent.bottom
                radius: 18
                color: "#18120f"
                border.width: 1
                border.color: "#362721"

                SpectrogramViewport {
                    id: viewport
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.topMargin: 16
                    anchors.bottom: transcript.top
                    anchors.leftMargin: 16
                    anchors.rightMargin: 16
                    anchors.bottomMargin: 16
                    waterfallMode: root.showWaterfall
                    energyLevel: root.fakeLevel
                }

                TranscriptPanel {
                    id: transcript
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.margins: 16
                    height: 84
                    committedText: "the shell should feel deliberate while the renderer stays free to be specialized"
                    partialText: "and the viewport seam ought to be obvious without looking temporary"
                }
            }
        }

        ControlDrawer {
            id: drawer
            width: 260
            visible: root.controlDrawerOpen
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