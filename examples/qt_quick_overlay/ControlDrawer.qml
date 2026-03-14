import QtQuick 2.15

Rectangle {
    id: root
    property bool expanded: true
    property bool paused: false
    property bool listening: true
    property bool injectionEnabled: true
    property bool waterfallMode: false
    property bool softAgcEnabled: true
    property bool hardAgcEnabled: false
    property bool autoSave: true
    property real softwareGain: 0.72
    property real hardwareGain: 0.46
    property string selectedSource: "Scarlett 2i2 USB"
    property string selectedSourceDetail: "Focusrite analogue input 1"
    property string activeModel: "Moonshine Voice"
    property string requestedModel: "Moonshine Voice"
    property string focusDescription: "Source selector chooses the capture device or PipeWire node we ask for. In Plasma-first mode this should eventually feel like an input helper, not like a fake hardware keyboard."

    signal togglePaused()
    signal toggleListening()
    signal toggleInjection()
    signal toggleWaterfall()
    signal toggleExpanded()
    signal toggleSoftAgc()
    signal toggleHardAgc()
    signal toggleAutoSave()
    signal bumpSource()
    signal bumpModel()
    signal adjustSoftwareGain(real nextValue)
    signal adjustHardwareGain(real nextValue)

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
        text: "A plausible Plasma-facing shell should expose the real speech controls first, then make the renderer seam feel intentional."
        color: "#b6a39a"
        font.pixelSize: 15
        wrapMode: Text.WordWrap
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: collapseButton.bottom
        anchors.margins: 16
        anchors.topMargin: 14
    }

    Flickable {
        visible: root.expanded
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: collapseButton.bottom
        anchors.bottom: parent.bottom
        anchors.leftMargin: 16
        anchors.rightMargin: 16
        anchors.topMargin: 102
        anchors.bottomMargin: 14
        contentWidth: width
        contentHeight: drawerColumn.height
        clip: true

        Column {
            id: drawerColumn
            width: root.width - 32
            spacing: 12

            DrawerSection {
                width: parent.width
                title: "Input Routing"
                subtitle: "Control the designated capture source before anything learns from user corrections."

                DrawerField {
                    width: parent.width
                    label: "Requested Source"
                    value: root.selectedSource
                    detail: root.selectedSourceDetail
                    accent: "#d39d4c"
                    onActivated: root.bumpSource()
                }

                DrawerSwitch {
                    width: parent.width
                    label: "Listening"
                    value: root.listening
                    onActivated: root.toggleListening()
                }

                DrawerSwitch {
                    width: parent.width
                    label: "Paused"
                    value: root.paused
                    onActivated: root.togglePaused()
                }
            }

            DrawerSection {
                width: parent.width
                title: "Gain & AGC"
                subtitle: "Separate manual trim from algorithm ownership so users never feel the system fighting their hands."

                DrawerSlider {
                    width: parent.width
                    label: "Software Gain"
                    detail: root.softAgcEnabled ? "Soft AGC is active; this becomes an aggressiveness-style tuning control." : "Manual digital trim before transcription."
                    value: root.softwareGain
                    accent: "#af7b3f"
                    onValueEdited: root.adjustSoftwareGain(nextValue)
                }

                DrawerSlider {
                    width: parent.width
                    label: "Hardware Gain"
                    detail: root.hardAgcEnabled ? "Hard AGC is active; expose behavior rather than pretending the hardware knob is still manual." : "Represents device-level preamp gain when the driver exposes it."
                    value: root.hardwareGain
                    accent: "#7a332d"
                    onValueEdited: root.adjustHardwareGain(nextValue)
                }

                DrawerSwitch {
                    width: parent.width
                    label: "Soft AGC"
                    value: root.softAgcEnabled
                    onActivated: root.toggleSoftAgc()
                }

                DrawerSwitch {
                    width: parent.width
                    label: "Hard AGC"
                    value: root.hardAgcEnabled
                    onActivated: root.toggleHardAgc()
                }
            }

            DrawerSection {
                width: parent.width
                title: "Model Driver"
                subtitle: "If we ever do RL on user corrections, the designated driver model needs to be explicit and session-stable."

                DrawerField {
                    width: parent.width
                    label: "Requested Model"
                    value: root.requestedModel
                    detail: "Active now: " + root.activeModel
                    accent: "#6f4f6c"
                    onActivated: root.bumpModel()
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

                DrawerSwitch {
                    width: parent.width
                    label: "Auto-save"
                    value: root.autoSave
                    onActivated: root.toggleAutoSave()
                }
            }

            DrawerSection {
                width: parent.width
                title: "Focused Control"
                subtitle: "A real control dock should always explain the currently hovered or focused frob."

                Text {
                    width: parent.width
                    text: root.focusDescription
                    color: "#d6c7c0"
                    font.pixelSize: 13
                    wrapMode: Text.WordWrap
                }
            }

            Rectangle {
                width: parent.width
                height: 102
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
                    text: "Qt Quick can make the dock feel native, but text injection semantics, Wayland embedding, and backend provenance still belong to the Rust side of the bridge."
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
    }
}
