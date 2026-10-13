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

    // ---- capturas (cenario "shots"): `--shots <dir> --fake <cenario>` ----
    readonly property string shotsDir: {
        const a = Qt.application.arguments
        const i = a.indexOf("--shots")
        return i >= 0 && i + 1 < a.length ? a[i + 1] : ""
    }
    readonly property string fakeName: {
        const a = Qt.application.arguments
        const i = a.indexOf("--fake")
        return i >= 0 && i + 1 < a.length ? a[i + 1] : "x"
    }
    property var plan: []
    property int step: 0
    property int shotPhase: 0
    property int saved: 0
    property int shotTicks: 0
    readonly property var pageNames: ["visao-geral", "sensibilidade", "configuracoes", "iluminacao"]

    function buildPlan() {
        const w = root.win, p = []
        for (const sz of [[1280, 800], [900, 600]]) {
            if (w.disconnectedVisible) { p.push({ sz: sz, page: 0, menu: false, name: "desconectado" }); continue }
            for (let i = 0; i < 4; i++) p.push({ sz: sz, page: i, menu: false, name: root.pageNames[i] })
            p.push({ sz: sz, page: 0, menu: true, name: "menu-perfil" })
        }
        return p
    }

    function shotTick() {
        const b = root.win.backend
        if (root.shotsDir === "") { Qt.exit(7); return }
        if (root.plan.length === 0) {
            if (b.daemonState === "connecting" || (b.daemonState === "running" && !b.hasDevice)) return
            root.plan = root.buildPlan()
            return
        }
        if (root.step >= root.plan.length) { Qt.exit(root.saved === root.plan.length ? 0 : 7); return }
        const e = root.plan[root.step]
        const w = root.win
        if (root.shotPhase === 0) {
            w.width = e.sz[0]; w.height = e.sz[1]
            w.goTo(e.page)
            if (e.menu) w.openProfileMenu(); else w.closeProfileMenu()
            root.shotPhase = 1
            return
        }
        if (root.shotPhase === 1) {
            root.shotPhase = 2
            const file = root.shotsDir + "/" + root.fakeName + "-" + e.name + "-" + e.sz[0] + "x" + e.sz[1] + ".png"
            const ok = (e.menu ? w.menuItem() : w.shellItem()).grabToImage(function (r) {
                if (r.saveToFile(file)) root.saved++
                root.step++
                root.shotPhase = 0
            })
            if (!ok) { Qt.exit(7) }
        }
    }

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
            if (w.pageCount !== 4 || w.disconnectedVisible) return false
            for (let i = 0; i < 4; i++) {
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
        case "layout": {
            // geometria no tamanho minimo (900x600): nada encolhido nem fora da janela
            const w = root.win
            w.width = 900; w.height = 600
            if (!w.pagesVisible) return false
            const r = w.layoutReport()
            if (r.sidebar !== 236 || r.top !== 64 || r.tabs < 40 || r.main !== 900 - 236) return false
            // marca da barra lateral: emblema de 30 px + palavra, cabendo na barra
            if (r.logoH !== 30 || r.logoW < 90 || r.logoW > 236 - 14 - 15 - 8) return false
            w.goTo(1)
            return w.sensitivityMinTrack() >= 100 && w.sensitivityUsable()
        }
        case "logo":
            return root.win.pagesVisible && root.win.layoutReport().logoH === 30
        case "logo-wrong-size":
            // contraprova: a mesma checagem com altura errada tem de falhar
            return root.win.pagesVisible && root.win.layoutReport().logoH === 31
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
        case "lighting": {
            // aba habilitada, formulario reflete o LED do daemon e Aplicar grava
            const w = root.win
            if (!b.hasDevice || !b.ledSupported || w.pageCount !== 4) return false
            w.goTo(3)
            if (w.currentIndex !== 3 || !w.navTab(3).enabled) return false
            if (b.ledEffect !== "cycle" || w.lightingApplyEnabled()) return false
            w.lightingSelect("fixed")
            if (!w.lightingApplyEnabled()) return false
            w.lightingApply()
            return b.ledBusy === true || b.ledEffect === "fixed"
        }
        case "lighting-hidden":
            // sem suporte no mouse: a aba some
            return b.hasDevice && !b.ledSupported && !root.win.navTab(3).visible
        case "edit":
            // otimista: a propriedade muda na hora, sem esperar o daemon
            if (!b.hasDevice) return false
            return b.setDpiLevel(0, 1000) === "" && b.dpiLevelsJson.startsWith("[1000,")
                && b.setDpiLevel(0, 7) !== "" && b.validateDpi(7) !== "" && b.validateDpi(1000) === ""
        }
        return false
    }

    property Timer poll: Timer {
        interval: root.scenario === "shots" ? 300 : 100
        repeat: true
        running: root.scenario !== "fail"
        onTriggered: {
            if (root.scenario === "shots") {
                root.shotTicks++
                root.shotTick()
                if (root.shotTicks > 400) Qt.exit(7)
                return
            }
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
