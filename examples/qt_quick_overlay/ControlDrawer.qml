import QtQuick 2.15
import QtQuick.Window 2.15

Rectangle {
    id: root
    property var state: ({})
    property string description: "Tab to a control for help. Space activates; arrow keys adjust sliders."
    signal command(string type, var value)
    signal closeRequested()
    Connections {
        target: root.Window.window
        function onActiveFocusItemChanged() {
            let item = root.Window.window.activeFocusItem
            while (item && item.parent !== controls) item = item.parent
            if (!item) return
            if (item.y < scroll.contentY) scroll.contentY = item.y
            if (item.y + item.height > scroll.contentY + scroll.height)
                scroll.contentY = item.y + item.height - scroll.height
        }
    }
    radius: 18
    color: "#1a1411"
    border.width: 1
    border.color: "#362721"
    clip: true
    Text {
        id: heading
        x: 16; y: 16
        text: "Control Panel"
        color: "#e2d8bd"; font.pixelSize: 18; font.weight: Font.DemiBold
    }
    ActionButton {
        label: "Close"; anchors.right: parent.right; anchors.rightMargin: 12; y: 12
        helpText: "Hide the control panel. Press C to open it again."
        onActivated: root.closeRequested()
        onHelpRequested: description => root.description = description
    }
    Flickable {
        id: scroll
        anchors.left: parent.left; anchors.right: parent.right
        anchors.top: heading.bottom; anchors.topMargin: 18
        anchors.bottom: help.top; anchors.bottomMargin: 12
        anchors.leftMargin: 16; anchors.rightMargin: 16
        contentHeight: controls.height
        clip: true
        Column {
            id: controls
            width: scroll.width
            spacing: 10
            DrawerSwitch {
                id: agc; objectName: "agcControl"
                width: parent.width; label: "Automatic gain"; value: !!root.state.auto_gain_enabled
                helpText: "Adjust microphone gain automatically. Manual gain is disabled while this is enabled."
                onActivated: root.command("toggle_agc", null)
                onHelpRequested: description => root.description = description
            }
            ValueSlider {
                id: gain; objectName: "gainControl"
                width: parent.width; maximum: 2; value: Number(root.state.gain || 0)
                enabled: !root.state.auto_gain_enabled
                label: "Software gain: " + value.toFixed(2) + "×" + (enabled ? "" : " (automatic)")
                helpText: "Adjust the microphone level before recognition. Left and right arrows change gain."
                onEdited: newValue => root.command("set_gain", newValue)
                onHelpRequested: description => root.description = description
            }
            ActionButton {
                width: parent.width; label: "Input: " + (root.state.source_label || "Default")
                helpText: "Choose the next input device. Device changes take effect on the next launch."
                onActivated: root.command("cycle_device", null)
                onHelpRequested: description => root.description = description
            }
            Text { text: "Input selection applies next launch"; color: "#b4a58b"; font.pixelSize: 11 }
            ModelSelector {
                objectName: "modelSelector"
                width: parent.width
                choices: root.state.models || []
                selectedId: root.state.selected_model === undefined ? -1 : root.state.selected_model
                activeLabel: root.state.model_label || "Loading…"
                onSelected: index => root.command("select_model", index)
                onHelpRequested: description => root.description = description
            }
            ActionButton {
                visible: root.state.download_progress !== undefined && root.state.download_progress !== null
                width: parent.width; label: "Cancel model download"
                helpText: "Cancel the pending model download. Keep using the active speech model."
                onActivated: root.command("cancel_model", null)
                onHelpRequested: description => root.description = description
            }
            ActionButton {
                width: parent.width; label: "Colors: " + (root.state.color_scheme || "Default")
                helpText: "Cycle the spectrum color palette."
                onActivated: root.command("cycle_color", null)
                onHelpRequested: description => root.description = description
            }
            ValueSlider {
                width: parent.width; minimum: 0.1; maximum: 1
                value: root.state.opacity === undefined ? 1 : Number(root.state.opacity)
                label: "Window opacity: " + Math.round(value * 100) + "%"
                helpText: "Adjust the window opacity. Left and right arrows change the level."
                onEdited: newValue => root.command("set_opacity", newValue)
                onHelpRequested: description => root.description = description
            }
            DrawerSwitch {
                width: parent.width; label: "Auto-save"; value: !!root.state.auto_save
                helpText: "Save explicit setting changes automatically. Automatic gain adjustments are never saved."
                onActivated: root.command("toggle_auto_save", null)
                onHelpRequested: description => root.description = description
            }
            ActionButton {
                width: parent.width; label: "Save settings"
                helpText: "Save your current settings for the next launch."
                onActivated: root.command("save", null)
                onHelpRequested: description => root.description = description
            }
        }
    }
    Text {
        id: help
        anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
        anchors.margins: 16
        height: 58
        text: root.description
        wrapMode: Text.WordWrap
        color: "#c2b8a3"; font.pixelSize: 12
    }
}
