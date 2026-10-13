import QtQuick
import org.ghublinux.gui

QtObject {
    id: root
    readonly property string scenario: {
        const a = Qt.application.arguments
        const i = a.indexOf("--smoke")
        return i >= 0 && i + 1 < a.length ? a[i + 1] : ""
    }
    property var win: AppWindow { visible: true }
    property int tries: 0

    function check() {
        const b = root.win.backend
        switch (root.scenario) {
        case "theme":
            return Theme.bg.toString() === "#282828"
        case "running":
            return b.daemonState === "running" && b.deviceName === "G703 LS"
                && b.dpiText === "1200 DPI" && b.batteryText === "91%"
        case "stopped":
            return b.daemonState === "stopped" && !b.hasDevice
        case "sleeping":
            return b.hasDevice && !b.connected
        case "edit":
            // otimista: a propriedade muda na hora, sem esperar o daemon
            if (!b.hasDevice) return false
            return b.setDpiLevel(0, 1000) === "" && b.dpiLevelsJson.startsWith("[1000,")
                && b.setDpiLevel(0, 7) !== "" && b.validateDpi(7) !== "" && b.validateDpi(1000) === ""
        }
        return false
    }

    property Timer poll: Timer {
        interval: 100
        repeat: true
        running: root.scenario !== "fail"
        onTriggered: {
            root.tries++
            let ok = false
            try { ok = root.check() } catch (e) { ok = false }
            if (ok)
                Qt.exit(0)
            else if (root.tries >= 40)
                Qt.exit(7)
        }
    }
    Component.onCompleted: { if (root.scenario === "fail") Qt.exit(7) }
}
