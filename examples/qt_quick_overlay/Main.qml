import QtQuick 2.15
import QtQuick.Window 2.15

Window {
    id: root
    width: 1100; height: 590
    minimumWidth: controlDrawerOpen ? 1020 : 720
    minimumHeight: 440
    visible: true
    flags: Qt.Tool | Qt.WindowStaysOnTopHint | Qt.WindowTitleHint | Qt.WindowCloseButtonHint | Qt.WindowMinMaxButtonsHint
    color: "#140c0a"
    title: "usit — Dictation"
    property var state: usitBridge.state
    property bool controlDrawerOpen: true
    property bool paused: !!state.paused
    property bool showWaterfall: state.viz_mode === "waterfall"
    opacity: state.opacity === undefined ? 1 : Math.max(0.1, Number(state.opacity))
    onClosing: close => {
        if (usitTray.hideToTray()) close.accepted = false
        else { usitBridge.send("quit", null); Qt.quit() }
    }
    function command(type, value) { usitBridge.send(type, value === undefined ? null : value) }
    // Invoked only by the host's explicit --self-test option, against a supplied snapshot.
    function selfTest() {
        if (transcript.committedText !== "snapshot committed" || transcript.partialText !== "snapshot partial") return false
        if (!paused || viewport.spectrum.length !== 4 || !drawer.state.auto_gain_enabled) return false
        pauseControl.activated()
        injectionControl.activated()
        vizControl.activated()
        drawer.command("toggle_agc", null)
        drawer.command("toggle_auto_save", null)
        drawer.command("cycle_device", null)
        drawer.command("cycle_model", null)
        drawer.command("set_gain", 1.25)
        drawer.command("set_opacity", 0.8)
        drawer.command("cycle_color", null)
        drawer.command("save", null)
        return true
    }
    Shortcut { sequence: "P"; onActivated: root.command("toggle_pause") }
    Shortcut { sequence: "I"; onActivated: root.command("toggle_injection") }
    Shortcut { sequence: "V"; onActivated: root.command("toggle_viz") }
    Shortcut { sequence: "C"; onActivated: root.controlDrawerOpen = !root.controlDrawerOpen }
    Shortcut { sequence: "Ctrl+S"; onActivated: root.command("save") }
    Shortcut { sequence: "Ctrl+Q"; onActivated: { root.command("quit"); Qt.quit() } }
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0; color: "#140c0a" }
            GradientStop { position: 0.45; color: "#1a120d" }
            GradientStop { position: 1; color: "#1c1511" }
        }
    }
    Rectangle { width: 460; height: 460; radius: 230; color: "#2e4128"; opacity: 0.16; x: parent.width * 0.58; y: -150 }
    Rectangle { width: 390; height: 390; radius: 195; color: "#8c3b28"; opacity: 0.12; x: -100; y: parent.height - 260 }
    Item {
        anchors.fill: parent; anchors.margins: 16
        Item {
            id: leftColumn
            anchors.left: parent.left; anchors.top: parent.top; anchors.bottom: parent.bottom
            anchors.right: root.controlDrawerOpen ? drawer.left : parent.right
            anchors.rightMargin: root.controlDrawerOpen ? 16 : 0
            Rectangle {
                id: header
                anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
                height: 48; radius: 24; color: "#1c1511"; border.width: 1; border.color: "#3d2d25"
                Text {
                    id: titleText
                    text: "usit"; color: "#e8deca"; font.pixelSize: 24; font.weight: Font.DemiBold
                    anchors.left: parent.left; anchors.leftMargin: 16; anchors.verticalCenter: parent.verticalCenter
                }
                Row {
                    anchors.left: titleText.right; anchors.leftMargin: 20; anchors.verticalCenter: parent.verticalCenter
                    spacing: 10
                    DrawerSwitch {
                        id: pauseControl
                        width: 136; label: root.paused ? "Paused" : "Listening"; value: !root.paused
                        helpText: "Pause or resume recognition (P)."
                        onActivated: root.command("toggle_pause")
                        onHelpRequested: description => drawer.description = description
                    }
                    DrawerSwitch {
                        id: injectionControl
                        width: 136; label: "Injecting"; value: !!root.state.injection_enabled
                        enabled: !!root.state.transcription_available
                        helpText: "Type committed phrases into your focused application (I). Requires an active speech model."
                        onActivated: root.command("toggle_injection")
                        onHelpRequested: description => drawer.description = description
                    }
                    DrawerSwitch {
                        id: vizControl
                        width: 136; label: root.showWaterfall ? "Waterfall" : "Bars"; value: root.showWaterfall
                        helpText: "Switch between spectrum bars and a scrolling frequency history (V)."
                        onActivated: root.command("toggle_viz")
                        onHelpRequested: description => drawer.description = description
                    }
                    ActionButton {
                        label: "Controls"; width: 88
                        helpText: "Show or hide microphone, model and appearance settings (C)."
                        onActivated: root.controlDrawerOpen = !root.controlDrawerOpen
                        onHelpRequested: description => drawer.description = description
                    }
                }
            }
            Text {
                id: status
                anchors.top: header.bottom; anchors.topMargin: 12
                anchors.left: parent.left; anchors.right: parent.right; anchors.leftMargin: 12
                color: root.state.error ? "#ff9785" : "#c2b8a3"
                text: root.state.error ? "ERROR: " + root.state.error :
                      root.state.download_progress !== undefined && root.state.download_progress !== null ?
                      "Downloading model: " + Math.round(root.state.download_progress * 100) + "%" :
                      (root.state.helper_summary || "Connecting to speech engine…")
                font.pixelSize: 13; wrapMode: Text.WordWrap
            }
            Rectangle {
                id: shell
                anchors.left: parent.left; anchors.right: parent.right
                anchors.top: status.bottom; anchors.topMargin: 12; anchors.bottom: parent.bottom
                radius: 18; color: "#18120f"; border.width: 1; border.color: "#362721"
                SpectrogramViewport {
                    id: viewport
                    anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
                    anchors.bottom: transcript.top; anchors.margins: 16
                    waterfallMode: root.showWaterfall
                    spectrum: root.state.spectrum || []
                    colorScheme: root.state.color_scheme || "fire"
                }
                TranscriptPanel {
                    id: transcript
                    anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
                    anchors.margins: 16
                    height: Math.min(150, shell.height * 0.4)
                    committedText: root.state.committed || ""
                    partialText: root.state.partial || ""
                }
            }
        }
        ControlDrawer {
            id: drawer
            width: 290; visible: root.controlDrawerOpen
            anchors.top: parent.top; anchors.right: parent.right; anchors.bottom: parent.bottom
            state: root.state
            onCommand: (type, value) => root.command(type, value)
            onCloseRequested: root.controlDrawerOpen = false
        }
    }
}
