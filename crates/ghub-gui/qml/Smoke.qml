import QtQuick
import org.ghublinux.gui

QtObject {
    id: root
    readonly property string scenario: {
        const a = Qt.application.arguments
        const i = a.indexOf("--smoke")
        return i >= 0 && i + 1 < a.length ? a[i + 1] : ""
    }
    property var win: AppWindow { }
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
        case "pages": {
            const w = root.win
            if (w.pageCount !== 3 || w.disconnectedVisible) return false
            for (let i = 0; i < 3; i++) {
                w.goTo(i)
                if (w.currentIndex !== i) return false
            }
            w.goTo(0)
            return w.sensitivityLevelCount() === 3 && b.dpiText === "1200 DPI"
                && w.overviewDpiText() === b.dpiText
        }
        case "disconnected":
            return root.win.disconnectedVisible && root.win.currentIndex === 0
        case "dimmed":
            // mouse dormindo: controles desabilitados e nunca "0 DPI"
            return b.hasDevice && !b.connected && !root.win.sensitivityUsable()
                && b.dpiText === "lendo…" && root.win.overviewDpiText() === "lendo…"
        case "visible":
            return root.win.visible === true
        case "hidden":
            return root.win.visible === false
        case "badtext": {
            // texto nao puramente numerico ou acima de 2^31-1 e rejeitado, sem escrever
            const w = root.win
            if (!b.hasDevice || !w.sensitivityUsable()) return false
            w.sensitivityEdit(0, "800abc")
            if (w.sensitivityRowError() === "" || w.sensitivityFieldText(0) !== "800") return false
            w.sensitivityEdit(0, "4294968496")
            return w.sensitivityRowError() !== "" && w.sensitivityFieldText(0) !== "1200"
                && w.sensitivityCommits() === 0 && b.dpiLevelsJson.startsWith("[800,")
        }
        case "nav": {
            // clicar na aba ativa nao a desmarca (nem a torna "checkable")
            const w = root.win
            w.goTo(1)
            const t = w.navTab(1)
            if (!t.enabled || t.checkable || !t.checked) return false
            t.clicked()
            return t.checked && w.currentIndex === 1 && !w.navTab(0).checked
        }
        case "rebind": {
            // edicao rejeitada nao pode quebrar o binding do campo; blur sem mudanca nao escreve
            const w = root.win
            if (!b.hasDevice || !w.sensitivityUsable()) return false
            w.sensitivityEdit(0, "7")
            if (w.sensitivityFieldText(0) !== "800") return false
            w.sensitivityEdit(1, "1200")
            if (w.sensitivityCommits() !== 0) return false
            return b.setDpiLevel(0, 1000) === "" && w.sensitivityFieldText(0) === "1000"
        }
        case "release": {
            // soltar o slider e terminar a edicao do campo chamam commitDpi
            const w = root.win
            if (!b.hasDevice || !w.sensitivityUsable()) return false
            w.sensitivityRelease(0)
            if (w.sensitivityCommits() !== 1) return false
            w.sensitivityEdit(0, "1000")
            return w.sensitivityCommits() === 2 && b.dpiLevelsJson.startsWith("[1000,")
        }
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
