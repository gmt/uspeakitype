import QtQuick 2.15
import QtQuick.Window 2.15

// This display is a passive layer surface. Only the separate Settings window
// accepts keyboard focus; showing the transcript must preserve the typing target.
Window {
    id: root
    width: 900; height: 330
    visible: false
    flags: Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.WindowDoesNotAcceptFocus
    color: "transparent"
    title: "usit — Dictation"
    property var state: usitBridge.state
    onClosing: close => {
        if (usitTray.hideToTray()) close.accepted = false
        else { usitBridge.send("quit", null); Qt.quit() }
    }
    function selfTest() {
        return transcript.committedText === "snapshot committed" &&
               transcript.partialText === "snapshot partial" && viewport.spectrum.length === 4
    }
    Rectangle {
        opacity: root.state.opacity === undefined ? 1 : Math.max(0.1, Number(root.state.opacity))
        anchors.fill: parent; radius: 22
        color: Qt.rgba(0.078, 0.047, 0.039, 0.88)
        border.width: 1; border.color: "#594032"
        clip: true
        Rectangle { width: 400; height: 400; radius: 200; color: "#2e4128"; opacity: 0.13; x: parent.width * 0.55; y: -180 }
        Rectangle { width: 330; height: 330; radius: 165; color: "#8c3b28"; opacity: 0.1; x: -120; y: 100 }
        Text {
            id: titleText
            x: 20; y: 14
            text: "usit"; color: "#e8deca"; font.pixelSize: 24; font.weight: Font.DemiBold
        }
        Text {
            anchors.left: titleText.right; anchors.leftMargin: 20; anchors.verticalCenter: titleText.verticalCenter
            text: root.state.push_to_talk ?
                      (root.state.ptt_held ? "● Dictating" : "Hold " + (root.state.ptt_shortcut || "your hold key") + " to dictate") :
                      root.state.paused ? "Paused" : "Listening"
            color: root.state.ptt_held ? "#c5d99a" : "#dcd2b8"; font.pixelSize: 14
        }
        Row {
            anchors.right: parent.right; anchors.rightMargin: 16; y: 14; spacing: 8
            ActionButton {
                objectName: "openSettings"
                label: "Controls"
                activeFocusOnTab: false
                helpText: "Open the separate Settings window."
                onActivated: usitTray.showSettings()
            }
            ActionButton {
                label: "Hide"
                activeFocusOnTab: false
                onActivated: if (!usitTray.hideToTray()) root.close()
            }
        }
        Text {
            id: status
            anchors.left: parent.left; anchors.right: parent.right
            anchors.leftMargin: 20; anchors.rightMargin: 20; y: 53
            text: root.state.ptt_error || root.state.error ||
                  (root.state.download_progress !== undefined && root.state.download_progress !== null ?
                   "Downloading model: " + Math.round(root.state.download_progress * 100) + "%" :
                   root.state.helper_summary || "Connecting to speech engine…")
            textFormat: Text.PlainText
            color: root.state.ptt_error || root.state.error ? "#ff9785" : "#c2b8a3"
            font.pixelSize: 12; elide: Text.ElideRight
        }
        SpectrogramViewport {
            id: viewport
            anchors.left: parent.left; anchors.right: parent.right; anchors.top: status.bottom
            anchors.bottom: transcript.top; anchors.margins: 16
            waterfallMode: root.state.viz_mode === "waterfall"
            spectrum: root.state.spectrum || []
            colorScheme: root.state.color_scheme || "flame"
        }
        TranscriptPanel {
            id: transcript
            anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
            anchors.margins: 16; height: 96
            committedText: root.state.committed || ""
            partialText: root.state.partial || ""
        }
    }
}
