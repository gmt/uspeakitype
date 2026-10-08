import QtQuick 2.15

Rectangle {
    id: root
    property string label: ""
    property string helpText: ""
    property bool value: false
    signal activated()
    signal helpRequested(string description)
    activeFocusOnTab: true
    height: 32
    radius: 16
    color: value ? "#2e382e" : "#342728"
    opacity: enabled ? 1 : 0.45
    border.width: activeFocus ? 2 : 1
    border.color: activeFocus ? "#d5ad67" : value ? "#3d4a3d" : "#4a383a"
    onActiveFocusChanged: if (activeFocus) helpRequested(helpText)
    Keys.onSpacePressed: activated()
    Keys.onReturnPressed: activated()
    Accessible.role: Accessible.CheckBox
    Accessible.name: label
    Accessible.description: helpText
    Accessible.checkable: true
    Accessible.checked: value
    Accessible.onToggleAction: activated()
    Text {
        text: root.label
        color: "#dcd2b8"
        font.pixelSize: 13
        font.weight: Font.Medium
        anchors.left: parent.left
        anchors.leftMargin: 12
        anchors.verticalCenter: parent.verticalCenter
    }
    Rectangle {
        width: 34; height: 20; radius: 10
        color: root.value ? "#5c7a45" : "#594a44"
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        Rectangle {
            width: 14; height: 14; radius: 7; y: 3
            x: root.value ? parent.width - width - 3 : 3
            color: "#e8deca"
            Behavior on x { NumberAnimation { duration: 120 } }
        }
    }
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        onEntered: root.helpRequested(root.helpText)
        onClicked: { root.forceActiveFocus(); root.activated() }
        cursorShape: Qt.PointingHandCursor
    }
}
