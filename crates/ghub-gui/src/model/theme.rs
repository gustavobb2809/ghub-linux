//! De onde vem o tema da GUI. Logica pura; o QObject `ThemeSource` (backend.rs) so a usa.
//!
//! Ordem: as cores publicadas pelo rice em `$XDG_STATE_HOME/rice-theme/colors.json` (ao vivo),
//! depois a paleta Qt do sistema (qt6ct, KDE) e, sem nenhuma das duas, o Gruvbox do ghub.

use std::path::PathBuf;

/// Chaves obrigatorias do `colors.json` do rice (todas `#rrggbb`).
pub const REQUIRED: [&str; 14] = [
    "bg", "surface", "overlay", "line", "lineStrong", "fg", "muted", "gray", "accent", "onAccent", "red",
    "green", "yellow", "aqua",
];
/// Chaves opcionais aproveitadas quando presentes.
pub const OPTIONAL: [&str; 3] = ["blue", "purple", "orange"];

/// O que fazer com o tema, escolhido por `GHUB_GUI_THEME` ou pelo modo de execucao.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    /// Le o rice; sem ele, a paleta do sistema (se houver tema Qt) ou o Gruvbox.
    Auto,
    /// Como `Auto`, mas forcado (inclusive no smoke, para capturas).
    Rice,
    /// Ignora o rice e usa a paleta Qt do sistema.
    System,
    /// Ignora tudo e usa o Gruvbox do ghub.
    Ghub,
}

/// `GHUB_GUI_THEME` manda; sem ela, o smoke usa o Gruvbox (testes nao dependem da maquina).
pub fn mode_from(env: Option<&str>, smoke: bool) -> ThemeMode {
    match env.map(str::trim) {
        Some("rice") => ThemeMode::Rice,
        Some("system") => ThemeMode::System,
        Some("ghub") => ThemeMode::Ghub,
        _ if smoke => ThemeMode::Ghub,
        _ => ThemeMode::Auto,
    }
}

impl ThemeMode {
    /// O arquivo do rice deve ser lido (e vigiado)?
    pub fn reads_rice(self) -> bool {
        matches!(self, ThemeMode::Auto | ThemeMode::Rice)
    }

    /// Tema usado quando nao ha cores do rice: "system" ou "ghub".
    pub fn fallback(self, platform_themed: bool) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Ghub => "ghub",
            ThemeMode::Auto | ThemeMode::Rice => {
                if platform_themed {
                    "system"
                } else {
                    "ghub"
                }
            }
        }
    }
}

/// Ha um tema Qt configurado no sistema (qt5ct/qt6ct, KDE, GNOME via adwaita-qt)?
/// Sem ele a paleta padrao do Qt e um cinza claro generico, pior que o Gruvbox do ghub.
pub fn platform_themed(qpa_theme: Option<&str>, desktop: Option<&str>) -> bool {
    if qpa_theme.is_some_and(|v| !v.trim().is_empty()) {
        return true;
    }
    desktop.is_some_and(|d| d.split(':').any(|p| p.eq_ignore_ascii_case("KDE")))
}

/// `$GHUB_GUI_THEME_FILE`, senao `$XDG_STATE_HOME/rice-theme/colors.json`, senao `~/.local/state/...`.
pub fn colors_path(override_file: Option<&str>, xdg_state: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    if let Some(f) = override_file.filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(f));
    }
    let base = match xdg_state.filter(|v| !v.is_empty()) {
        Some(x) => PathBuf::from(x),
        None => PathBuf::from(home.filter(|v| !v.is_empty())?).join(".local/state"),
    };
    Some(base.join("rice-theme").join("colors.json"))
}

fn is_hex(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Valida o `colors.json` e devolve so as chaves conhecidas, em minusculas.
/// `None` se faltar alguma obrigatoria ou se algum valor nao for `#rrggbb`: o QML nunca ve lixo.
pub fn parse_colors(text: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let obj = v.as_object()?;
    let mut out = serde_json::Map::new();
    for k in REQUIRED {
        let s = obj.get(k)?.as_str()?;
        if !is_hex(s) {
            return None;
        }
        out.insert(k.to_string(), serde_json::Value::String(s.to_ascii_lowercase()));
    }
    for k in OPTIONAL {
        if let Some(s) = obj.get(k).and_then(|v| v.as_str()).filter(|s| is_hex(s)) {
            out.insert(k.to_string(), serde_json::Value::String(s.to_ascii_lowercase()));
        }
    }
    Some(serde_json::Value::Object(out).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O arquivo real publicado pelo rice (18/11/2026).
    const RICE: &str = r##"{
      "bg": "#111111", "surface": "#181818", "overlay": "#1f1f1f",
      "line": "#2a2a2a", "lineStrong": "#3b3b3b",
      "fg": "#dccbad", "muted": "#b09d8e", "gray": "#85766b",
      "accent": "#748c95", "onAccent": "#111111",
      "red": "#cc4a3e", "green": "#9aa02e", "yellow": "#d9a74a", "blue": "#7fa3a8",
      "purple": "#b8738a", "aqua": "#7aa38a", "orange": "#e07a2f"
    }"##;

    #[test]
    fn parses_the_real_rice_file() {
        let out: serde_json::Value = serde_json::from_str(&parse_colors(RICE).unwrap()).unwrap();
        assert_eq!(out["accent"], "#748c95");
        assert_eq!(out["onAccent"], "#111111");
        assert_eq!(out["blue"], "#7fa3a8");
        assert_eq!(out.as_object().unwrap().len(), 17);
    }

    #[test]
    fn rejects_missing_keys_bad_values_and_garbage() {
        assert_eq!(parse_colors(&RICE.replace("\"accent\": \"#748c95\",", "")), None);
        assert_eq!(parse_colors(&RICE.replace("#748c95", "red")), None);
        assert_eq!(parse_colors(&RICE.replace("#748c95", "#748c9")), None);
        assert_eq!(parse_colors("não é json"), None);
        assert_eq!(parse_colors("[]"), None);
    }

    #[test]
    fn keeps_only_known_keys_in_lowercase() {
        let text = RICE.replace("#748c95", "#748C95").replace("\"orange\"", "\"x\": 1, \"orange\"");
        let out: serde_json::Value = serde_json::from_str(&parse_colors(&text).unwrap()).unwrap();
        assert_eq!(out["accent"], "#748c95");
        assert!(out.get("x").is_none());
    }

    #[test]
    fn invalid_optional_keys_are_dropped_not_fatal() {
        let out: serde_json::Value =
            serde_json::from_str(&parse_colors(&RICE.replace("#e07a2f", "laranja")).unwrap()).unwrap();
        assert!(out.get("orange").is_none());
        assert_eq!(out["red"], "#cc4a3e");
    }

    #[test]
    fn path_prefers_override_then_xdg_then_home() {
        assert_eq!(colors_path(Some("/t/c.json"), Some("/x"), Some("/h")), Some(PathBuf::from("/t/c.json")));
        assert_eq!(colors_path(None, Some("/x"), Some("/h")), Some(PathBuf::from("/x/rice-theme/colors.json")));
        assert_eq!(
            colors_path(Some(""), Some(""), Some("/h")),
            Some(PathBuf::from("/h/.local/state/rice-theme/colors.json"))
        );
        assert_eq!(colors_path(None, None, None), None);
    }

    #[test]
    fn mode_comes_from_env_and_smoke_defaults_to_ghub() {
        assert_eq!(mode_from(None, false), ThemeMode::Auto);
        assert_eq!(mode_from(None, true), ThemeMode::Ghub);
        assert_eq!(mode_from(Some("rice"), true), ThemeMode::Rice);
        assert_eq!(mode_from(Some("system"), false), ThemeMode::System);
        assert_eq!(mode_from(Some(" ghub "), false), ThemeMode::Ghub);
        assert_eq!(mode_from(Some("outro"), false), ThemeMode::Auto);
    }

    #[test]
    fn fallback_follows_the_platform_theme() {
        assert_eq!(ThemeMode::Auto.fallback(true), "system");
        assert_eq!(ThemeMode::Auto.fallback(false), "ghub");
        assert_eq!(ThemeMode::System.fallback(false), "system");
        assert_eq!(ThemeMode::Ghub.fallback(true), "ghub");
        assert!(ThemeMode::Rice.reads_rice() && ThemeMode::Auto.reads_rice());
        assert!(!ThemeMode::System.reads_rice() && !ThemeMode::Ghub.reads_rice());
    }

    #[test]
    fn platform_theme_detection() {
        assert!(platform_themed(Some("qt5ct:qt6ct"), None));
        assert!(platform_themed(None, Some("KDE")));
        assert!(platform_themed(None, Some("Hyprland:KDE")));
        assert!(!platform_themed(Some(""), Some("Hyprland")));
        assert!(!platform_themed(None, None));
    }
}
