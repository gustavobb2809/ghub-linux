//! Modelo da bandeja: transforma o estado do daemon numa descrição do menu.

use ghub_ipc::{DeviceState, DeviceSummary, STATE_CHARGING, STATE_FULL};

use crate::manager::LOW_BATTERY_PERCENT;
use crate::tray_icon::IconState;

pub const APP_NAME: &str = "G HUB Linux";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Autostart {
    Enabled,
    Disabled,
    /// O serviço do systemd não está instalado (ou o `systemctl` não responde).
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayInput {
    pub devices: Vec<DeviceSummary>,
    /// Estado do dispositivo principal (veja `pick_primary`), se houver.
    pub primary: Option<DeviceState>,
    pub profiles: Vec<String>,
    pub active_profile: String,
    pub autostart: Autostart,
}

impl TrayInput {
    pub fn empty() -> Self {
        TrayInput {
            devices: Vec::new(),
            primary: None,
            profiles: Vec::new(),
            active_profile: String::new(),
            autostart: Autostart::Unavailable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// Linha informativa (desabilitada).
    Info(String),
    Separator,
    Dpi { labels: Vec<String>, selected: usize, enabled: bool },
    Profiles { labels: Vec<String>, selected: usize },
    Autostart { checked: bool, enabled: bool },
    OpenGui,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayView {
    pub icon: IconState,
    pub title: String,
    pub tooltip_title: String,
    pub tooltip_text: String,
    pub entries: Vec<Entry>,
}

/// O dispositivo que a bandeja mostra: o primeiro conectado, senão o primeiro da lista.
pub fn pick_primary(devices: &[DeviceSummary]) -> Option<&DeviceSummary> {
    devices.iter().find(|d| d.connected).or_else(|| devices.first())
}

fn battery_text(percent: i32, state: &str) -> String {
    if percent < 0 {
        return "bateria desconhecida".to_string();
    }
    let suffix = match state {
        STATE_CHARGING => " (carregando)",
        STATE_FULL => " (carga completa)",
        _ => "",
    };
    format!("{percent} %{suffix}")
}

fn icon_state(device: Option<&DeviceState>) -> IconState {
    match device {
        None => IconState::Disconnected,
        Some(d) if !d.connected => IconState::Disconnected,
        Some(d) if d.battery_state == STATE_CHARGING => IconState::Charging,
        Some(d)
            if d.battery_percent >= 0
                && d.battery_percent <= i32::from(LOW_BATTERY_PERCENT)
                && d.battery_state != STATE_FULL =>
        {
            IconState::LowBattery
        }
        Some(_) => IconState::Normal,
    }
}

/// Barra de bateria em texto (10 blocos), como a barra fina do mock.
fn battery_bar(percent: i32) -> String {
    let filled = ((percent.clamp(0, 100) + 5) / 10) as usize;
    format!("{}{}", "▰".repeat(filled), "▱".repeat(10 - filled))
}

/// Segunda linha do cabeçalho: "<bateria> · <estado> · perfil <ativo>", omitindo o que falta.
fn status_line(d: &DeviceState, profile: &str) -> String {
    let mut parts = Vec::new();
    if d.battery_percent >= 0 {
        parts.push(format!("{} %", d.battery_percent));
    }
    parts.push(
        if !d.connected {
            "inativo ou dormindo"
        } else if d.battery_state == STATE_CHARGING {
            "carregando"
        } else if d.battery_state == STATE_FULL {
            "carga completa"
        } else {
            "conectado"
        }
        .to_string(),
    );
    if !profile.is_empty() {
        parts.push(format!("perfil {profile}"));
    }
    parts.join(" · ")
}

pub fn build_view(input: &TrayInput) -> TrayView {
    let device = input.primary.as_ref();
    let icon = icon_state(device);
    let mut entries = Vec::new();

    let (tooltip_title, tooltip_text) = match device {
        None => {
            entries.push(Entry::Info("Nenhum dispositivo encontrado".into()));
            (APP_NAME.to_string(), "Nenhum dispositivo encontrado".to_string())
        }
        Some(d) => {
            let battery = battery_text(d.battery_percent, &d.battery_state);
            let profile = if input.active_profile.is_empty() {
                d.profile.as_str()
            } else {
                input.active_profile.as_str()
            };
            entries.push(Entry::Info(d.name.clone()));
            entries.push(Entry::Info(status_line(d, profile)));
            if d.battery_percent >= 0 {
                entries.push(Entry::Info(battery_bar(d.battery_percent)));
            }
            let text = if d.connected {
                format!("{battery} · {} DPI", d.dpi_current)
            } else {
                format!("Inativo ou dormindo · {battery}")
            };
            if !d.dpi_levels.is_empty() {
                entries.push(Entry::Separator);
                entries.push(Entry::Dpi {
                    labels: d.dpi_levels.iter().map(|v| format!("{v} DPI")).collect(),
                    selected: (d.active_level as usize).min(d.dpi_levels.len() - 1),
                    enabled: d.connected,
                });
            }
            (d.name.clone(), text)
        }
    };

    if !input.profiles.is_empty() {
        // Sem DPI, o separador do cabeçalho ainda não foi posto; nunca dois seguidos.
        if entries.last() != Some(&Entry::Separator)
            && !matches!(entries.last(), Some(Entry::Dpi { .. }))
        {
            entries.push(Entry::Separator);
        }
        let selected = input
            .profiles
            .iter()
            .position(|p| *p == input.active_profile)
            .unwrap_or(0);
        entries.push(Entry::Profiles { labels: input.profiles.clone(), selected });
    }

    entries.push(Entry::Separator);
    entries.push(Entry::Autostart {
        checked: input.autostart == Autostart::Enabled,
        enabled: input.autostart != Autostart::Unavailable,
    });
    entries.push(Entry::OpenGui);
    entries.push(Entry::Quit);

    TrayView {
        icon,
        title: APP_NAME.to_string(),
        tooltip_title,
        tooltip_text,
        entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(connected: bool, percent: i32, battery_state: &str) -> DeviceState {
        DeviceState {
            id: "046dc539-1".into(),
            name: "G703".into(),
            connected,
            dpi_current: if connected { 1200 } else { 0 },
            dpi_levels: vec![800, 1200, 1600],
            active_level: 1,
            dpi_min: 100,
            dpi_max: 25600,
            dpi_step: 50,
            dpi_list: vec![],
            polling_hz: if connected { 1000 } else { 0 },
            polling_supported: vec![125, 250, 500, 1000],
            battery_percent: percent,
            battery_state: battery_state.into(),
            profile: "Padrão".into(),
        }
    }

    fn summary(id: &str, connected: bool) -> DeviceSummary {
        DeviceSummary {
            id: id.into(),
            name: id.into(),
            connected,
            battery_percent: 50,
            battery_state: "discharging".into(),
        }
    }

    fn input(primary: Option<DeviceState>) -> TrayInput {
        TrayInput {
            devices: vec![],
            primary,
            profiles: vec!["Padrão".into(), "Trabalho".into()],
            active_profile: "Trabalho".into(),
            autostart: Autostart::Disabled,
        }
    }

    #[test]
    fn connected_device_builds_the_full_menu() {
        let v = build_view(&input(Some(state(true, 91, "discharging"))));
        assert_eq!(v.icon, IconState::Normal);
        assert_eq!(v.title, APP_NAME);
        assert_eq!(v.tooltip_title, "G703");
        assert_eq!(v.tooltip_text, "91 % · 1200 DPI");
        assert_eq!(
            v.entries,
            vec![
                Entry::Info("G703".into()),
                Entry::Info("91 % · conectado · perfil Trabalho".into()),
                Entry::Info("▰▰▰▰▰▰▰▰▰▱".into()),
                Entry::Separator,
                Entry::Dpi {
                    labels: vec!["800 DPI".into(), "1200 DPI".into(), "1600 DPI".into()],
                    selected: 1,
                    enabled: true
                },
                Entry::Profiles {
                    labels: vec!["Padrão".into(), "Trabalho".into()],
                    selected: 1
                },
                Entry::Separator,
                Entry::Autostart { checked: false, enabled: true },
                Entry::OpenGui,
                Entry::Quit,
            ]
        );
    }

    #[test]
    fn header_status_line_variants() {
        let line = |p, st: &str, profile: &str| {
            let mut i = input(Some(state(true, p, st)));
            i.active_profile = profile.into();
            match &build_view(&i).entries[1] {
                Entry::Info(t) => t.clone(),
                e => panic!("{e:?}"),
            }
        };
        assert_eq!(line(91, STATE_CHARGING, "Padrão"), "91 % · carregando · perfil Padrão");
        assert_eq!(line(100, STATE_FULL, "Padrão"), "100 % · carga completa · perfil Padrão");
        // bateria desconhecida: sem trecho de % e sem barra
        let mut i = input(Some(state(true, -1, "unknown")));
        i.active_profile = String::new();
        let v = build_view(&i);
        // perfil vazio cai no perfil do dispositivo
        assert_eq!(v.entries[1], Entry::Info("conectado · perfil Padrão".into()));
        assert_eq!(v.entries[2], Entry::Separator);
        let mut s = state(true, -1, "unknown");
        s.profile = String::new();
        let v = build_view(&input(Some(s)));
        assert_eq!(v.entries[1], Entry::Info("conectado · perfil Trabalho".into()));
    }

    #[test]
    fn battery_bar_rounds_to_ten_blocks() {
        assert_eq!(battery_bar(0), "▱▱▱▱▱▱▱▱▱▱");
        assert_eq!(battery_bar(82), "▰▰▰▰▰▰▰▰▱▱");
        assert_eq!(battery_bar(100), "▰▰▰▰▰▰▰▰▰▰");
    }

    #[test]
    fn menu_never_has_adjacent_or_edge_separators() {
        for profiles in [true, false] {
            for levels in [true, false] {
                for dev in [true, false] {
                    let mut s = state(true, 50, "discharging");
                    if !levels {
                        s.dpi_levels.clear();
                    }
                    let mut i = input(dev.then_some(s));
                    if !profiles {
                        i.profiles.clear();
                    }
                    let e = build_view(&i).entries;
                    assert_ne!(e.first(), Some(&Entry::Separator));
                    for pair in e.windows(2) {
                        assert!(!(pair[0] == Entry::Separator && pair[1] == Entry::Separator));
                    }
                }
            }
        }
    }

    #[test]
    fn icon_follows_the_battery() {
        let icon = |p, s| build_view(&input(Some(state(true, p, s)))).icon;
        assert_eq!(icon(15, "discharging"), IconState::LowBattery);
        assert_eq!(icon(16, "discharging"), IconState::Normal);
        assert_eq!(icon(5, "unknown"), IconState::LowBattery);
        assert_eq!(icon(5, STATE_CHARGING), IconState::Charging);
        assert_eq!(icon(5, STATE_FULL), IconState::Normal);
        assert_eq!(icon(-1, "unknown"), IconState::Normal);
        assert_eq!(icon(80, STATE_CHARGING), IconState::Charging);
    }

    #[test]
    fn charging_text_and_unknown_battery_text() {
        let v = build_view(&input(Some(state(true, 80, STATE_CHARGING))));
        assert_eq!(v.tooltip_text, "80 % (carregando) · 1200 DPI");
        let v = build_view(&input(Some(state(true, -1, "unknown"))));
        assert_eq!(v.tooltip_text, "bateria desconhecida · 1200 DPI");
        let v = build_view(&input(Some(state(true, 100, STATE_FULL))));
        assert_eq!(v.tooltip_text, "100 % (carga completa) · 1200 DPI");
    }

    #[test]
    fn disconnected_device_keeps_the_last_battery_and_disables_dpi() {
        let v = build_view(&input(Some(state(false, 89, "unknown"))));
        assert_eq!(v.icon, IconState::Disconnected);
        assert_eq!(v.tooltip_text, "Inativo ou dormindo · 89 %");
        assert_eq!(v.entries[0], Entry::Info("G703".into()));
        assert_eq!(
            v.entries[1],
            Entry::Info("89 % · inativo ou dormindo · perfil Trabalho".into())
        );
        assert_eq!(v.entries[2], Entry::Info("▰▰▰▰▰▰▰▰▰▱".into()));
        assert!(v.entries.iter().any(
            |e| matches!(e, Entry::Dpi { enabled: false, .. })
        ));
    }

    #[test]
    fn disconnected_with_low_battery_is_still_disconnected() {
        let v = build_view(&input(Some(state(false, 3, "unknown"))));
        assert_eq!(v.icon, IconState::Disconnected);
    }

    #[test]
    fn no_device_shows_a_clear_message_and_no_dpi_entry() {
        let v = build_view(&input(None));
        assert_eq!(v.icon, IconState::Disconnected);
        assert_eq!(v.tooltip_title, APP_NAME);
        assert_eq!(v.tooltip_text, "Nenhum dispositivo encontrado");
        assert_eq!(v.entries[0], Entry::Info("Nenhum dispositivo encontrado".into()));
        assert!(!v.entries.iter().any(|e| matches!(e, Entry::Dpi { .. })));
        assert!(v.entries.contains(&Entry::OpenGui));
        assert!(v.entries.contains(&Entry::Quit));
    }

    #[test]
    fn profile_selection_and_absence() {
        let mut i = input(None);
        i.active_profile = "não existe".into();
        let v = build_view(&i);
        assert!(v.entries.iter().any(
            |e| matches!(e, Entry::Profiles { selected: 0, .. })
        ));
        i.profiles.clear();
        let v = build_view(&i);
        assert!(!v.entries.iter().any(|e| matches!(e, Entry::Profiles { .. })));
        // nunca dois separadores seguidos
        for pair in v.entries.windows(2) {
            assert!(!(pair[0] == Entry::Separator && pair[1] == Entry::Separator));
        }
    }

    #[test]
    fn autostart_states() {
        let entry = |a| {
            let mut i = input(None);
            i.autostart = a;
            build_view(&i)
                .entries
                .into_iter()
                .find(|e| matches!(e, Entry::Autostart { .. }))
                .unwrap()
        };
        assert_eq!(entry(Autostart::Enabled), Entry::Autostart { checked: true, enabled: true });
        assert_eq!(entry(Autostart::Disabled), Entry::Autostart { checked: false, enabled: true });
        assert_eq!(entry(Autostart::Unavailable), Entry::Autostart { checked: false, enabled: false });
    }

    #[test]
    fn active_level_out_of_range_is_clamped_and_empty_levels_hide_the_entry() {
        let mut s = state(true, 50, "discharging");
        s.active_level = 9;
        let v = build_view(&input(Some(s.clone())));
        assert!(v.entries.iter().any(
            |e| matches!(e, Entry::Dpi { selected: 2, .. })
        ));
        s.dpi_levels.clear();
        let v = build_view(&input(Some(s)));
        assert!(!v.entries.iter().any(|e| matches!(e, Entry::Dpi { .. })));
    }

    #[test]
    fn primary_prefers_a_connected_device() {
        assert_eq!(pick_primary(&[]), None);
        let ds = vec![summary("a", false), summary("b", true)];
        assert_eq!(pick_primary(&ds).unwrap().id, "b");
        let ds = vec![summary("a", false), summary("c", false)];
        assert_eq!(pick_primary(&ds).unwrap().id, "a");
    }
}
