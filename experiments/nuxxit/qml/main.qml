import QtQuick 2.15
import QtQuick.Window 2.15

import org.usit.nuxxit 1.0

Window {
    id: root
    width: 780
    height: 360
    visible: true
    title: "nuxxit"
    color: "#100d0a"

    LevelBridge {
        id: bridge
    }

    Timer {
        interval: 33
        repeat: true
        running: true
        triggeredOnStart: true
        onTriggered: bridge.tick()
    }

    Rectangle {
        anchors.fill: parent
        anchors.margins: 20
        radius: 18
        color: "#1b1510"
        border.color: "#4b3929"
        border.width: 1

        Column {
            anchors.fill: parent
            anchors.margins: 20
            spacing: 16

            Text {
                text: "nuxxit"
                color: "#f3e4d2"
                font.pixelSize: 30
                font.bold: true
            }

            Text {
                text: bridge.model_label
                color: "#d0bda4"
                font.pixelSize: 16
            }

            Rectangle {
                width: parent.width
                height: 160
                radius: 16
                color: "#231a13"
                border.color: "#5a4330"

                Rectangle {
                    width: 52
                    radius: 12
                    color: bridge.waterfallish ? "#b07b38" : "#cc8f43"
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 14
                    anchors.left: parent.left
                    anchors.leftMargin: 24
                    height: Math.max(14, (parent.height - 28) * bridge.level)
                }

                Rectangle {
                    width: 52
                    radius: 12
                    color: "#ead3a1"
                    opacity: 0.75
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 14
                    anchors.left: parent.left
                    anchors.leftMargin: 104
                    height: Math.max(14, (parent.height - 28) * bridge.peak)
                }
            }

            Text {
                text: bridge.status
                color: "#f0ddc5"
                font.pixelSize: 18
            }

            Text {
                text: "Single-process Rust/CXX-Qt sketch. QML drives tick(); Rust owns the state object."
                color: "#a79079"
                wrapMode: Text.WordWrap
                font.pixelSize: 14
            }
        }
    }
}
