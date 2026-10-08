import QtQuick 2.15

Item {
    id: root
    property var choices: []
    property int selectedId: -1
    property string activeLabel: "Loading…"
    property bool expanded: false
    readonly property int highlightedIndex: options.currentIndex
    signal selected(int index)
    signal helpRequested(string description)
    height: expanded ? 246 : 30
    function open() {
        expanded = true
        let index = choices.findIndex(choice => choice.id === selectedId)
        options.currentIndex = Math.max(0, index)
        options.forceActiveFocus()
        options.positionViewAtIndex(options.currentIndex, ListView.Contain)
    }
    function choose(index) {
        if (index < 0 || index >= choices.length) return
        selected(index)
        expanded = false
        trigger.forceActiveFocus()
    }
    ActionButton {
        id: trigger
        width: parent.width
        label: "Model: " + root.activeLabel + (root.expanded ? " ▴" : " ▾")
        helpText: "Choose a speech model directly. Arrow keys browse; Return selects. Browsing does not download models."
        enabled: root.choices.length > 0
        onActivated: {
            if (root.expanded) root.expanded = false
            else root.open()
        }
        onHelpRequested: description => root.helpRequested(description)
        Keys.onDownPressed: root.open()
    }
    Rectangle {
        y: 38; width: parent.width; height: 208
        visible: root.expanded
        color: "#241c18"; radius: 10
        border.color: options.activeFocus ? "#d5ad67" : "#4a3831"
        ListView {
            id: options
            objectName: "modelChoices"
            anchors.fill: parent; anchors.margins: 5
            model: root.choices
            clip: true
            currentIndex: -1
            keyNavigationEnabled: true
            boundsBehavior: Flickable.StopAtBounds
            Keys.onReturnPressed: root.choose(currentIndex)
            Keys.onSpacePressed: root.choose(currentIndex)
            Keys.onEscapePressed: { root.expanded = false; trigger.forceActiveFocus() }
            Keys.onPressed: event => {
                if (event.key === Qt.Key_Home) { currentIndex = 0; event.accepted = true }
                if (event.key === Qt.Key_End) { currentIndex = count - 1; event.accepted = true }
            }
            onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)
            delegate: Rectangle {
                required property var modelData
                required property int index
                width: options.width; height: 38; radius: 6
                color: ListView.isCurrentItem ? "#3d4731" : mouse.containsMouse ? "#342820" : "transparent"
                Text {
                    anchors.fill: parent; anchors.margins: 8
                    text: (modelData.id === root.selectedId ? "✓ " : "") + modelData.label
                    textFormat: Text.PlainText
                    verticalAlignment: Text.AlignVCenter
                    color: "#e8deca"; font.pixelSize: 12; elide: Text.ElideRight
                }
                MouseArea {
                    id: mouse
                    anchors.fill: parent; hoverEnabled: true
                    onClicked: root.choose(index)
                    onEntered: root.helpRequested(modelData.label + ". Select to activate; a missing model will download first.")
                }
                Accessible.role: Accessible.ListItem
                Accessible.name: modelData.label
                Accessible.onPressAction: root.choose(index)
            }
        }
    }
}
