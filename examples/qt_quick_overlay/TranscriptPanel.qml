import QtQuick 2.15

Rectangle {
    id: root
    property string committedText: ""
    property string partialText: ""
    radius: 16; color: "#1c1613"; border.width: 1; border.color: "#42322a"
    Flickable {
        id: scroll
        anchors.fill: parent; anchors.margins: 16
        contentHeight: transcript.height
        clip: true
        function showLatest() { contentY = Math.max(0, contentHeight - height) }
        onContentHeightChanged: showLatest()
        Column {
            id: transcript
            width: scroll.width; spacing: 4
            Text {
                width: parent.width
                text: root.committedText; textFormat: Text.PlainText
                color: "#e8deca"; font.pixelSize: 16; font.weight: Font.Medium
                wrapMode: Text.WordWrap
            }
            Text {
                width: parent.width
                text: root.partialText; textFormat: Text.PlainText
                color: "#c2b8a3"; font.pixelSize: 16
                wrapMode: Text.WordWrap
            }
        }
    }
}
