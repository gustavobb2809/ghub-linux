import QtQuick
import QtQuick.Window
import org.ghublinux.gui

Window {
    id: root
    visible: true
    width: 400
    height: 300
    color: Theme.bg
    readonly property string scenario: {
        const a = Qt.application.arguments
        const i = a.indexOf("--smoke")
        return i >= 0 && i + 1 < a.length ? a[i + 1] : ""
    }
    Timer {
        interval: 200
        running: true
        onTriggered: {
            let code = 7
            try {
                if (root.scenario === "theme")
                    code = Theme.bg.toString() === "#282828" ? 0 : 7
            } catch (e) {
                code = 7
            }
            Qt.exit(code)
        }
    }
}
