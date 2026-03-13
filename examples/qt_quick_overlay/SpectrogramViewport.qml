import QtQuick 2.15

Rectangle {
    id: root
    property bool waterfallMode: false
    property real energyLevel: 0.5

    radius: 24
    color: "#091015"
    border.width: 1
    border.color: "#1f313a"
    clip: true

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: "#081015" }
            GradientStop { position: 0.55; color: "#0d1c23" }
            GradientStop { position: 1.0; color: "#121516" }
        }
    }

    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: parent.height * 0.33
        gradient: Gradient {
            GradientStop { position: 0.0; color: "#00000000" }
            GradientStop { position: 1.0; color: "#280d0907" }
        }
    }

    Item {
        anchors.fill: parent
        anchors.margins: 18

        Repeater {
            model: root.waterfallMode ? 44 : 56

            Rectangle {
                required property int index

                width: root.waterfallMode ? parent.width : Math.max(6, (parent.width - 18) / 56)
                height: root.waterfallMode
                    ? Math.max(3, parent.height / 44)
                    : Math.max(24, parent.height * (0.12 + ((index % 9) / 12)) * (0.35 + root.energyLevel))
                radius: root.waterfallMode ? 3 : 6
                x: root.waterfallMode ? 0 : index * (width + 4)
                y: root.waterfallMode ? index * (height + 4) : parent.height - height - (index % 3) * 4
                color: root.waterfallMode
                    ? Qt.hsla(0.08 + (index / 70), 0.78, 0.45 + (root.energyLevel * 0.16), 0.92)
                    : Qt.hsla(0.10 + (index / 100), 0.84, 0.46 + (root.energyLevel * 0.15), 0.96)
                opacity: root.waterfallMode ? 0.38 + ((index % 7) / 12) : 0.85
            }
        }
    }

    Text {
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.margins: 18
        text: "Placeholder renderer seam: this Item could host the existing Rust/WGPU spectrogram surface."
        color: "#8fa3a8"
        font.pixelSize: 14
    }
}
