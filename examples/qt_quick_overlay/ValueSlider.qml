import QtQuick 2.15

Item {
    id: root
    property string label: ""
    property string helpText: ""
    property real value: 0
    property real minimum: 0
    property real maximum: 1
    property real step: 0.05
    signal edited(real newValue)
    signal helpRequested(string description)
    activeFocusOnTab: enabled
    height: 45
    opacity: enabled ? 1 : 0.45
    function adjust(amount) { edited(Math.max(minimum, Math.min(maximum, value + amount))) }
    onActiveFocusChanged: if (activeFocus) helpRequested(helpText)
    Keys.onLeftPressed: adjust(-step)
    Keys.onRightPressed: adjust(step)
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Home) { edited(minimum); event.accepted = true }
        if (event.key === Qt.Key_End) { edited(maximum); event.accepted = true }
    }
    Accessible.role: Accessible.Slider
    Accessible.name: label
    Accessible.description: helpText
    Accessible.onIncreaseAction: adjust(step)
    Accessible.onDecreaseAction: adjust(-step)
    Text { text: root.label; color: "#dcd2b8"; font.pixelSize: 12 }
    Rectangle {
        id: track
        anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
        anchors.bottomMargin: 8
        height: 7; radius: 3
        color: "#4a3831"
        border.color: root.activeFocus ? "#d5ad67" : "#4a3831"
        Rectangle {
            width: parent.width * Math.max(0, Math.min(1, (root.value - root.minimum) / (root.maximum - root.minimum)))
            height: parent.height; radius: 3; color: "#8a9a60"
        }
    }
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        onEntered: root.helpRequested(root.helpText)
        function choose(x) { root.edited(root.minimum + Math.max(0, Math.min(1, x / width)) * (root.maximum - root.minimum)) }
        onPressed: mouse => { root.forceActiveFocus(); choose(mouse.x) }
        onPositionChanged: mouse => { if (pressed) choose(mouse.x) }
    }
}
