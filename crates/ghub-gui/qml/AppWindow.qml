import QtQuick
import QtQuick.Controls.Basic
import org.ghublinux.gui

ApplicationWindow {
    id: win
    width: 1280; height: 800; minimumWidth: 900; minimumHeight: 600
    title: "ghub"
    color: Theme.bg
    property alias backend: backend
    Backend { id: backend; Component.onCompleted: backend.start() }
    Label { anchors.centerIn: parent; text: backend.deviceName; color: Theme.fg }
}
