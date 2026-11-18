import QtQuick
import org.ghublinux.gui

Rectangle {
    id: toast
    property string text: ""
    signal dismissed()
    visible: text !== ""
    Accessible.role: Accessible.AlertMessage
    Accessible.name: toast.text
    color: Theme.card
    radius: 10
    border.color: Theme.badText
    border.width: 1
    implicitHeight: msg.implicitHeight + 24
    implicitWidth: Math.min(560, msg.implicitWidth + 40)
    Text {
        id: msg
        anchors { fill: parent; margins: 12 }
        text: toast.text
        color: Theme.fg
        wrapMode: Text.WordWrap
        font.family: Theme.ui
        font.pixelSize: 13
    }
    MouseArea { anchors.fill: parent; onClicked: toast.dismissed() }
    onTextChanged: if (visible) hideTimer.restart()
    Timer {
        id: hideTimer
        interval: 5000
        running: toast.visible
        repeat: false
        onTriggered: toast.dismissed()
    }
}
