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
    font.family: Theme.ui

    property alias backend: backend
    property int currentIndex: 0
    readonly property int pageCount: 3
    readonly property bool disconnectedVisible: backend.daemonState === "stopped"
    readonly property bool connecting: backend.daemonState === "connecting"
    readonly property bool pagesVisible: !disconnectedVisible && !connecting
    readonly property var profileList: JSON.parse(backend.profilesJson)
    readonly property var deviceList: backend.showDeviceSelector
        ? JSON.parse(backend.devicesJson)
        : (backend.hasDevice ? [{ id: backend.selectedDevice, name: backend.deviceName }] : [])
    readonly property bool asleep: backend.hasDevice && !backend.connected
    readonly property bool onSettings: currentIndex === 2
    readonly property string sleepText: "Mouse desconectado ou dormindo — mova o mouse"

    function goTo(i) { if (i >= 0 && i < pageCount) currentIndex = i }
    function sensitivityLevelCount() { return sensitivity.levelCount }
    function navTab(i) { return [nav0, nav1, nav2][i] }
    function layoutReport() { return layout.report() }
    function sensitivityMinTrack() { return sensitivity.minTrackWidth() }
    function openProfileMenu() { if (pill.visible) profileMenu.open() }
    function shellItem() { return shell }
    function menuItem() { return profileMenu.contentItem }
    function closeProfileMenu() { profileMenu.close() }
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

    Rectangle {
        id: shell
        anchors.fill: parent
        color: Theme.bg
    RowLayout {
        id: layout
        anchors.fill: parent
        spacing: 0

        function report() {
            // diagnostico de geometria usado pelo smoke "layout"
            const r = {
                sidebar: sidebar.width, top: topBar.visible ? topBar.height : -1,
                tabs: tabBar.visible ? tabBar.height : -1,
                main: mainCol.width, content: content.height,
                logoW: brand.width, logoH: brand.height
            }
            return r
        }

        // ---------------- Barra lateral ----------------
        Rectangle {
            id: sidebar
            Layout.fillHeight: true
            Layout.preferredWidth: Theme.sidebarWidth
            Layout.minimumWidth: Theme.sidebarWidth
            Layout.maximumWidth: Theme.sidebarWidth
            color: Theme.bgHard
            Rectangle { anchors { right: parent.right; top: parent.top; bottom: parent.bottom } width: 1; color: Theme.line }
            ColumnLayout {
                anchors { fill: parent; leftMargin: 14; rightMargin: 15; topMargin: 20; bottomMargin: 20 }
                spacing: 6
                Logo {
                    id: brand
                    objectName: "logo"
                    Layout.leftMargin: 8
                    Layout.bottomMargin: 18
                    symbolSize: 30
                }
                Text {
                    Layout.leftMargin: 8
                    Layout.topMargin: 6
                    Layout.bottomMargin: 6
                    text: "Dispositivos"
                    color: Theme.faint
                    font.family: Theme.ui
                    font.pixelSize: 11
                    font.letterSpacing: 1.2
                    font.capitalization: Font.AllUppercase
                }
                Text {
                    visible: win.deviceList.length === 0
                    Layout.leftMargin: 10
                    text: "Nenhum dispositivo"
                    color: Theme.muted
                    font.family: Theme.ui
                    font.pixelSize: 13
                }
                Repeater {
                    model: win.deviceList
                    delegate: SideEntry {
                        id: dev
                        required property var modelData
                        readonly property bool selected: modelData.id === backend.selectedDevice
                        Layout.fillWidth: true
                        text: modelData.name
                        subtitle: selected ? (backend.connected ? backend.batteryText : "dormindo") : ""
                        checked: selected && !win.onSettings
                        enabled: !win.disconnectedVisible
                        glyph1: "M12 3c-3.5 0-6 2.6-6 6v6c0 3.4 2.5 6 6 6s6-2.6 6-6V9c0-3.4-2.5-6-6-6z"
                        glyph2: "M12 3v7"
                        glyphColor: selected ? Theme.yellow : Theme.faint
                        onClicked: { if (!selected) backend.selectDevice(modelData.id); if (win.onSettings) win.goTo(0) }
                    }
                }
                Item { Layout.fillHeight: true }
                SideEntry {
                    id: nav2
                    Layout.fillWidth: true
                    text: "Configurações"
                    enabled: win.pagesVisible
                    checked: win.currentIndex === 2
                    showDot: true
                    dotColor: backend.daemonState === "running" ? Theme.good
                              : (backend.daemonState === "stopped" ? Theme.bad : Theme.muted)
                    subtitle: backend.daemonState === "running" ? "ghubd ativo"
                              : (backend.daemonState === "stopped" ? "ghubd parado" : "conectando…")
                    glyph1: "M9 12a3 3 0 1 0 6 0a3 3 0 1 0 -6 0z"
                    glyph2: "M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M18.4 5.6l-2.1 2.1M7.7 16.3l-2.1 2.1"
                    glyphColor: Theme.fgDim
                    onClicked: win.goTo(2)
                }
            }
        }

        // ---------------- Area principal ----------------
        ColumnLayout {
            id: mainCol
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumWidth: 0
            spacing: 0

            Rectangle {
                id: topBar
                visible: win.pagesVisible
                Layout.fillWidth: true
                Layout.preferredHeight: Theme.topBarHeight
                color: "transparent"
                Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Theme.line }
                RowLayout {
                    anchors { fill: parent; leftMargin: Theme.pad; rightMargin: Theme.pad; bottomMargin: 1 }
                    spacing: 16
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Text {
                            Layout.fillWidth: true
                            text: win.onSettings ? "Configurações"
                                  : (backend.hasDevice ? backend.deviceName : "Nenhum dispositivo")
                            elide: Text.ElideRight
                            color: Theme.fg
                            font.family: Theme.ui
                            font.pixelSize: 18
                            font.weight: Font.DemiBold
                        }
                        Text {
                            visible: !win.onSettings && backend.hasDevice
                            Layout.fillWidth: true
                            text: win.asleep ? win.sleepText : "Conectado"
                            elide: Text.ElideRight
                            color: win.asleep ? Theme.warn : Theme.muted
                            font.family: Theme.ui
                            font.pixelSize: 12
                        }
                    }
                    Rectangle {
                        id: pill
                        visible: !win.onSettings && backend.hasDevice
                        radius: height / 2
                        color: Theme.raised
                        border.color: Theme.hi
                        border.width: 1
                        implicitWidth: pillRow.implicitWidth + 28 + 2
                        implicitHeight: pillRow.implicitHeight + 16 + 2
                        RowLayout {
                            id: pillRow
                            anchors.centerIn: parent
                            spacing: 8
                            Text { text: "Perfil"; color: Theme.muted; font.family: Theme.ui; font.pixelSize: 13 }
                            Text { text: backend.activeProfile; color: Theme.fg; font.family: Theme.ui; font.pixelSize: 13 }
                            Glyph { d1: "M5 9.5l7 7 7-7"; color: Theme.fgDim; size: 12 }
                        }
                        MouseArea {
                            id: pillArea
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: profileMenu.opened ? profileMenu.close() : profileMenu.open()
                        }
                        Popup {
                            id: profileMenu
                            parent: pill
                            y: pill.height + 6
                            x: pill.width - width
                            width: Math.max(220, pill.width)
                            padding: 6
                            closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
                            background: Rectangle { color: Theme.card; radius: 12; border.color: Theme.hi; border.width: 1 }
                            contentItem: ColumnLayout {
                                spacing: 2
                                Repeater {
                                    model: win.profileList
                                    delegate: Rectangle {
                                        id: mi
                                        required property string modelData
                                        readonly property bool active: modelData === backend.activeProfile
                                        Layout.fillWidth: true
                                        implicitHeight: 36
                                        radius: 8
                                        color: active ? Theme.raised : (miArea.containsMouse ? Qt.rgba(1, 1, 1, 0.05) : "transparent")
                                        RowLayout {
                                            anchors { fill: parent; leftMargin: 10; rightMargin: 10 }
                                            spacing: 10
                                            Rectangle { width: 8; height: 8; radius: 4; color: mi.active ? Theme.yellow : Theme.hi }
                                            Text {
                                                Layout.fillWidth: true
                                                text: mi.modelData
                                                elide: Text.ElideRight
                                                color: mi.active ? Theme.fg : Theme.fg2
                                                font.family: Theme.ui
                                                font.pixelSize: 13
                                            }
                                            Text { visible: mi.active; text: "ativo"; color: Theme.muted; font.family: Theme.ui; font.pixelSize: 12 }
                                        }
                                        MouseArea {
                                            id: miArea
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            onClicked: { backend.activateProfile(mi.modelData); profileMenu.close() }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Rectangle {
                id: tabBar
                visible: win.pagesVisible && !win.onSettings
                Layout.fillWidth: true
                Layout.preferredHeight: tabRow.implicitHeight + 1
                color: "transparent"
                Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Theme.line }
                Row {
                    id: tabRow
                    x: 24
                    spacing: 4
                    NavTab {
                        id: nav0
                        text: "Visão geral"
                        enabled: win.pagesVisible
                        checked: win.currentIndex === 0
                        onClicked: win.goTo(0)
                    }
                    NavTab {
                        id: nav1
                        text: "Sensibilidade"
                        enabled: win.pagesVisible
                        checked: win.currentIndex === 1
                        onClicked: win.goTo(1)
                    }
                    NavTab { text: "Iluminação"; enabled: false; badge: "v1.1" }
                    NavTab { text: "Botões"; enabled: false; badge: "v1.2" }
                }
            }

            Item {
                id: content
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
                    font.family: Theme.ui
                    font.pixelSize: 18
                }
                StackLayout {
                    anchors.fill: parent
                    visible: win.pagesVisible
                    currentIndex: win.currentIndex
                    Overview { id: overview; backend: win.backend }
                    Sensitivity { id: sensitivity; backend: win.backend }
                    Settings { id: settings; backend: win.backend }
                }

                Toast {
                    anchors { bottom: parent.bottom; horizontalCenter: parent.horizontalCenter; bottomMargin: Theme.pad }
                    text: backend.lastError
                    onDismissed: backend.clearError()
                }
            }
        }
    }
    }
}
