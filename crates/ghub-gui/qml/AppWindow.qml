import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic
import org.ghublinux.gui

ApplicationWindow {
    id: win
    width: 1280; height: 800; minimumWidth: 900; minimumHeight: 600
    title: "ghub"
    visible: true
    color: Theme.bg
    font.family: Theme.uiFonts[0]

    property alias backend: backend
    property int currentIndex: 0
    readonly property int pageCount: 3
    readonly property bool disconnectedVisible: backend.daemonState === "stopped"
    readonly property bool connecting: backend.daemonState === "connecting"
    readonly property bool pagesVisible: !disconnectedVisible && !connecting

    function goTo(i) { if (i >= 0 && i < pageCount) currentIndex = i }
    function sensitivityLevelCount() { return sensitivity.levelCount }
    function navTab(i) { return [nav0, nav1, nav2][i] }
    function sensitivityFieldText(i) { return sensitivity.fieldText(i) }
    function sensitivityRowError() { return sensitivity.rowError }
    function sensitivityUsable() { return sensitivity.usable }
    function sensitivityCommits() { return sensitivity.commits }
    function sensitivityRelease(i) { sensitivity.simulateRelease(i) }
    function sensitivityEdit(i, t) { sensitivity.simulateEdit(i, t) }
    function overviewDpiText() { return overview.visible ? overview.dpiShown : "" }

    Backend {
        id: backend
        onActivateRequested: { if (win.visibility === Window.Minimized) win.showNormal(); win.show(); win.raise(); win.requestActivate() }
        Component.onCompleted: backend.start()
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.fillHeight: true
            Layout.preferredWidth: 200
            color: Theme.bgSoft
            ColumnLayout {
                anchors { fill: parent; margins: 12 }
                spacing: 4
                RowLayout {
                    Layout.bottomMargin: 16
                    spacing: 10
                    Rectangle { width: 32; height: 32; radius: 16; color: Theme.accent }
                    ColumnLayout {
                        spacing: 0
                        Text {
                            text: "ghub"; color: Theme.fg; font.pixelSize: 18; font.bold: true
                            font.family: Theme.uiFonts[0]
                        }
                        Text {
                            text: "LINUX"; color: Theme.fgDim; font.pixelSize: 10
                            font.family: Theme.monoFonts[0]
                        }
                    }
                }
                NavTab {
                    id: nav0
                    Layout.fillWidth: true
                    text: "Visão geral"
                    enabled: win.pagesVisible
                    checked: win.currentIndex === 0
                    onClicked: win.goTo(0)
                }
                NavTab {
                    id: nav1
                    Layout.fillWidth: true
                    text: "Sensibilidade"
                    enabled: win.pagesVisible
                    checked: win.currentIndex === 1
                    onClicked: win.goTo(1)
                }
                NavTab { Layout.fillWidth: true; text: "Iluminação"; enabled: false; badge: "v1.1" }
                NavTab { Layout.fillWidth: true; text: "Botões"; enabled: false; badge: "v1.2" }
                NavTab {
                    id: nav2
                    Layout.fillWidth: true
                    text: "Configurações"
                    enabled: win.pagesVisible
                    checked: win.currentIndex === 2
                    onClicked: win.goTo(2)
                }
                Item { Layout.fillHeight: true }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            RowLayout {
                visible: backend.showDeviceSelector
                Layout.fillWidth: true
                Layout.margins: Theme.gap
                spacing: 10
                Text {
                    text: "Dispositivo"
                    color: Theme.fgDim
                    font.family: Theme.uiFonts[0]
                    font.pixelSize: 13
                }
                ComboBox {
                    id: deviceBox
                    Layout.preferredWidth: 280
                    textRole: "name"
                    valueRole: "id"
                    model: JSON.parse(backend.devicesJson)
                    currentIndex: {
                        const l = model
                        for (let i = 0; i < l.length; i++)
                            if (l[i].id === backend.selectedDevice) return i
                        return -1
                    }
                    onActivated: i => backend.selectDevice(model[i].id)
                }
            }

            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true

                Disconnected {
                    anchors.fill: parent
                    backend: win.backend
                    visible: win.disconnectedVisible
                }
                Text {
                    anchors.centerIn: parent
                    visible: win.connecting
                    text: "Conectando…"
                    color: Theme.fgDim
                    font.family: Theme.uiFonts[0]
                    font.pixelSize: 18
                }
                StackLayout {
                    anchors.fill: parent
                    visible: win.pagesVisible
                    currentIndex: win.currentIndex
                    Overview { id: overview; backend: win.backend }
                    Sensitivity { id: sensitivity; backend: win.backend }
                    Settings { backend: win.backend }
                }

                Toast {
                    anchors { bottom: parent.bottom; horizontalCenter: parent.horizontalCenter; bottomMargin: Theme.gap }
                    text: backend.lastError
                    onDismissed: backend.clearError()
                }
            }
        }
    }
}
