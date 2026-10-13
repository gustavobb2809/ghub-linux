pragma Singleton
import QtQuick

QtObject {
    readonly property color bg: "#282828"
    readonly property color bgSoft: "#32302f"
    readonly property color card: "#3c3836"
    readonly property color cardHi: "#504945"
    readonly property color border: "#665c54"
    readonly property color fg: "#ebdbb2"
    readonly property color fgDim: "#a89984"
    readonly property color accent: "#fe8019"
    readonly property color good: "#b8bb26"
    readonly property color warn: "#fabd2f"
    readonly property color bad: "#fb4934"
    readonly property var uiFonts: ["Inter", "Noto Sans", "sans-serif"]
    readonly property var monoFonts: ["JetBrains Mono", "monospace"]
    readonly property int radius: 10
    readonly property int gap: 16
}
