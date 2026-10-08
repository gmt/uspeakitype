import QtQuick 2.15

Rectangle {
    id: root
    property string label: ""
    property string helpText: ""
    signal activated()
    signal helpRequested(string description)
    activeFocusOnTab: true
    width: buttonLabel.implicitWidth + 24
    height: 30
    radius: 12
    opacity: enabled ? 1 : 0.45
    color: mouse.containsMouse ? "#3b2e26" : "#2b211d"
    border.width: activeFocus ? 2 : 1
    border.color: activeFocus ? "#d5ad67" : "#4a3831"
    onActiveFocusChanged: if (activeFocus) helpRequested(helpText)
    Keys.onSpacePressed: activated()
    Keys.onReturnPressed: activated()
    Accessible.role: Accessible.Button
    Accessible.name: label
    Accessible.description: helpText
    Accessible.onPressAction: activated()
    Text {
        id: buttonLabel
        anchors.centerIn: parent
        text: root.label
        color: "#f0eadf"
        font.pixelSize: 12
        font.weight: Font.Medium
        width: Math.min(implicitWidth, root.width - 20)
        elide: Text.ElideRight
    }
    MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        onEntered: root.helpRequested(root.helpText)
        onClicked: { root.forceActiveFocus(); root.activated() }
        cursorShape: Qt.PointingHandCursor
    }
}
