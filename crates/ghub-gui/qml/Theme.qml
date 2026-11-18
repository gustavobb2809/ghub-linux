pragma Singleton
import QtQuick

// Tokens da GUI. As cores vêm, nesta ordem:
//   "rice":   o tema publicado pelo rice em ~/.local/state/rice-theme/colors.json (ao vivo, via ThemeSource);
//   "system": a paleta Qt do sistema (qt6ct, KDE);
//   "ghub":   o Gruvbox do ghub, quando não há nenhum dos dois.
// A AppWindow preenche `fallback` e `rice`; sem ela (smoke "theme"), vale o Gruvbox.
// Cores que viram texto ou contorno passam por `legible()`, então nenhuma paleta deixa legenda ou
// destaque abaixo do contraste mínimo.
QtObject {
    id: t

    property string fallback: "ghub"
    property var rice: null
    function setRice(json) {
        if (!json) { t.rice = null; return }
        try { t.rice = JSON.parse(json) } catch (e) { t.rice = null }
    }
    readonly property string source: t.rice ? "rice" : t.fallback
    readonly property SystemPalette sys: SystemPalette { colorGroup: SystemPalette.Active }

    // ---- utilitários de cor ----
    function mix(a, b, f) {
        return Qt.rgba(a.r + (b.r - a.r) * f, a.g + (b.g - a.g) * f, a.b + (b.b - a.b) * f, 1)
    }
    function lum(c) {
        const f = x => x <= 0.03928 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4)
        return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
    }
    function contrast(a, b) {
        const x = t.lum(a), y = t.lum(b)
        return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05)
    }
    // aproxima `c` de `towards` até ter `min` de contraste sobre `over`
    function legible(c, over, towards, min) {
        let r = c
        for (let i = 1; i <= 10 && t.contrast(r, over) < min; i++) r = t.mix(c, towards, i / 10)
        return r
    }
    function alpha(c, a) { return Qt.rgba(c.r, c.g, c.b, a) }

    // ---- superfícies ----
    readonly property color bg: source === "rice" ? rice.bg : source === "system" ? sys.window : "#282828"
    readonly property bool light: lum(bg) > 0.4
    readonly property color bgHard: source === "rice" ? Qt.darker(rice.bg, 1.3)
                                  : source === "system" ? Qt.darker(sys.window, light ? 1.05 : 1.25) : "#1d2021"
    readonly property color card: source === "rice" ? rice.surface : source === "system" ? sys.base : "#32302f"
    readonly property color raised: source === "rice" ? rice.overlay : source === "system" ? sys.button : "#3c3836"
    readonly property color line: source === "rice" ? rice.line : source === "system" ? mix(fg, card, 0.88) : "#3c3836"
    readonly property color hi: source === "rice" ? rice.lineStrong : source === "system" ? mix(fg, card, 0.78) : "#504945"
    // contorno de controle (toggle desligado, trilho, rádio): 3:1 sobre o card
    readonly property color border: legible(source === "rice" ? rice.gray : source === "system" ? mix(fg, card, 0.6) : "#665c54",
                                            card, fg, 3.0)

    // ---- texto ----
    readonly property color fg: source === "rice" ? rice.fg : source === "system" ? sys.windowText : "#ebdbb2"
    readonly property color muted: legible(source === "rice" ? rice.muted : source === "system" ? mix(fg, card, 0.4) : "#a89984",
                                           card, fg, 4.5)
    readonly property color fgDim: muted
    readonly property color fg2: mix(fg, muted, 0.5)
    // só para traços e ícones decorativos (não passa contraste de texto pequeno)
    readonly property color faint: source === "rice" ? rice.gray : source === "system" ? mix(fg, card, 0.55) : "#7c6f64"

    // ---- destaque: seleção, ação primária, foco, logo ----
    readonly property color _accentRaw: source === "rice" ? rice.accent : source === "system" ? sys.accent : "#fe8019"
    readonly property color accent: legible(_accentRaw, card, fg, 3.0)
    readonly property color accentText: legible(accent, bg, fg, 4.5)    // destaque usado como texto
    readonly property color onAccent: source === "rice" ? rice.onAccent
                                    : (contrast(bgHard, accent) >= contrast(fg, accent) ? bgHard : fg)
    readonly property color accentDark: mix(accent, bg, 0.5)
    readonly property color selection: alpha(accent, 0.16)               // fundo de item selecionado
    readonly property color focusRing: accentText

    // ---- semânticas (fixas no Gruvbox fora do rice; versão escura em paletas claras) ----
    readonly property color yellow: source === "rice" ? rice.yellow : (light ? "#b57614" : "#fabd2f")
    readonly property color warn: yellow
    readonly property color good: source === "rice" ? rice.green : (light ? "#79740e" : "#b8bb26")
    readonly property color bad: source === "rice" ? rice.red : (light ? "#9d0006" : "#fb4934")
    readonly property color aqua: source === "rice" ? rice.aqua : (light ? "#427b58" : "#8ec07c")
    readonly property color badText: legible(bad, card, fg, 4.5)
    readonly property color yellowDark: mix(yellow, bg, 0.5)
    readonly property color goodDark: mix(good, bg, 0.5)
    readonly property color aquaDark: mix(aqua, bg, 0.5)
    readonly property color warnBg: mix(warn, bg, 0.82)

    // cores do LED: o destaque do sistema primeiro, depois a paleta do tema
    readonly property var ledSwatches: source === "rice"
        ? [rice.accent, rice.orange || "#e07a2f", rice.yellow, rice.green, rice.aqua, rice.blue || "#7fa3a8", rice.purple || "#b8738a", rice.red, rice.fg]
        : [accent.toString(), "#fe8019", "#fabd2f", "#b8bb26", "#8ec07c", "#83a598", "#d3869b", "#fb4934", "#ebdbb2"]

    // ---- fontes: as do sistema (qt6ct/KDE); mono só para números e medidas ----
    readonly property var _families: {
        try { return Qt.fontFamilies() } catch (e) { return [] }
    }
    readonly property string ui: Qt.application.font.family
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
