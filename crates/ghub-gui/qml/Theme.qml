pragma Singleton
import QtQuick

// Tokens do mock aprovado (Gruvbox escuro). Fontes: IBM Plex Sans (UI) e
// JetBrains Mono (numeros); sem elas, cai na fonte padrao do sistema / monospace.
QtObject {
    readonly property color bg: "#282828"
    readonly property color bgHard: "#1d2021"
    readonly property color card: "#32302f"
    readonly property color line: "#3c3836"
    readonly property color raised: "#3c3836"
    readonly property color hi: "#504945"
    readonly property color border: "#665c54"
    readonly property color fg: "#ebdbb2"
    readonly property color fg2: "#bdae93"
    readonly property color fgDim: "#a89984"
    readonly property color muted: "#928374"
    readonly property color faint: "#7c6f64"
    readonly property color accent: "#fe8019"
    readonly property color yellow: "#fabd2f"
    readonly property color yellowDark: "#b57614"
    readonly property color good: "#b8bb26"
    readonly property color goodDark: "#79740e"
    readonly property color aqua: "#8ec07c"
    readonly property color aquaDark: "#427b58"
    readonly property color bad: "#fb4934"
    readonly property color warn: "#fabd2f"
    readonly property color warnBg: "#4a3a12"

    readonly property var _families: {
        try { return Qt.fontFamilies() } catch (e) { return [] }
    }
    readonly property string ui: _families.indexOf("IBM Plex Sans") >= 0
                                 ? "IBM Plex Sans" : Qt.application.font.family
    readonly property string mono: _families.indexOf("JetBrains Mono") >= 0
                                   ? "JetBrains Mono"
                                   : (_families.indexOf("JetBrainsMono Nerd Font") >= 0
                                      ? "JetBrainsMono Nerd Font" : "monospace")

    readonly property int sidebarWidth: 236
    readonly property int topBarHeight: 64
    readonly property int pad: 28
    readonly property int gap: 20
    readonly property int cardRadius: 14
}
