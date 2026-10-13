use ghub_ipc::{DeviceState, DeviceSummary};
use super::dpi::{DpiRange, Levels};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DaemonStatus { #[default] Connecting, Running, Stopped }
impl DaemonStatus {
    pub fn as_str(self) -> &'static str {
        match self { Self::Connecting => "connecting", Self::Running => "running", Self::Stopped => "stopped" }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceChoice { pub id: String, pub name: String }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceView {
    pub id: String, pub name: String, pub connected: bool,
    pub dpi_current: u32, pub dpi_levels: Vec<u32>, pub active_level: u32,
    pub range: DpiRange, pub polling_hz: u32, pub polling_options: Vec<u32>,
    pub battery_percent: i32, pub battery_state: String,
}

impl DeviceView {
    pub fn from_state(s: &DeviceState) -> Self {
        DeviceView {
            id: s.id.clone(), name: s.name.clone(), connected: s.connected,
            dpi_current: s.dpi_current, dpi_levels: s.dpi_levels.clone(), active_level: s.active_level,
            range: DpiRange::from_state(s), polling_hz: s.polling_hz,
            polling_options: s.polling_supported.clone(),
            battery_percent: s.battery_percent, battery_state: s.battery_state.clone(),
        }
    }
    pub fn levels(&self) -> Levels { Levels { levels: self.dpi_levels.clone(), active: self.active_level } }
    pub fn with_levels(mut self, l: Levels) -> Self { self.dpi_levels = l.levels; self.active_level = l.active; self }
    /// O DPI lido no mouse difere do nível ativo do perfil (ex.: botão de DPI).
    pub fn profile_differs(&self) -> bool {
        self.connected && self.dpi_current != 0
            && self.dpi_levels.get(self.active_level as usize).is_some_and(|p| *p != self.dpi_current)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ViewState {
    pub status: DaemonStatus,
    pub device: Option<DeviceView>,
    /// Dispositivos conectados (o seletor só aparece com 2 ou mais).
    pub choices: Vec<DeviceChoice>,
    pub profiles: Vec<String>,
    pub active_profile: String,
    pub autostart: Option<bool>,
    pub read_error: Option<String>,
}

/// Escolhe o dispositivo exibido: o selecionado (se ainda existe), senão o primeiro
/// conectado, senão o primeiro conhecido (exibido esmaecido).
pub fn pick_device<'a>(list: &'a [DeviceSummary], selected: Option<&str>) -> Option<&'a DeviceSummary> {
    selected.and_then(|id| list.iter().find(|d| d.id == id))
        .or_else(|| list.iter().find(|d| d.connected))
        .or_else(|| list.first())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st() -> DeviceState {
        DeviceState {
            id: "m1".into(), name: "Mouse".into(), connected: true, dpi_current: 1600,
            dpi_levels: vec![800, 1200, 3200], active_level: 1, dpi_min: 100, dpi_max: 25600, dpi_step: 50,
            dpi_list: vec![], polling_hz: 1000, polling_supported: vec![125, 500, 1000],
            battery_percent: 80, battery_state: "discharging".into(), profile: "default".into(),
        }
    }
    fn sum(id: &str, connected: bool) -> DeviceSummary {
        DeviceSummary { id: id.into(), name: id.into(), connected, battery_percent: 50, battery_state: String::new() }
    }

    #[test]
    fn from_state_copies_fields() {
        let s = st();
        let v = DeviceView::from_state(&s);
        assert_eq!(v.id, "m1");
        assert_eq!(v.name, "Mouse");
        assert!(v.connected);
        assert_eq!(v.dpi_current, 1600);
        assert_eq!(v.dpi_levels, vec![800, 1200, 3200]);
        assert_eq!(v.active_level, 1);
        assert_eq!(v.range, DpiRange::from_state(&s));
        assert_eq!(v.polling_hz, 1000);
        assert_eq!(v.polling_options, vec![125, 500, 1000]);
        assert_eq!(v.battery_percent, 80);
        assert_eq!(v.battery_state, "discharging");
    }
    #[test]
    fn profile_differs_cases() {
        let v = DeviceView::from_state(&st());
        assert!(v.profile_differs()); // 1600 vs 1200
        let mut z = v.clone(); z.dpi_current = 0;
        assert!(!z.profile_differs());
        let mut d = v.clone(); d.connected = false;
        assert!(!d.profile_differs());
        let mut o = v.clone(); o.active_level = 9;
        assert!(!o.profile_differs());
        let mut e = v.clone(); e.dpi_current = 1200;
        assert!(!e.profile_differs());
    }
    #[test]
    fn pick_device_preference() {
        let list = vec![sum("a", false), sum("b", true), sum("c", true)];
        assert_eq!(pick_device(&list, Some("c")).unwrap().id, "c");
        assert_eq!(pick_device(&list, Some("zzz")).unwrap().id, "b");
        assert_eq!(pick_device(&list, None).unwrap().id, "b");
        let ghosts = vec![sum("a", false), sum("d", false)];
        assert_eq!(pick_device(&ghosts, None).unwrap().id, "a");
        assert!(pick_device(&[], None).is_none());
    }
    #[test]
    fn with_levels_swaps_levels_and_active() {
        let v = DeviceView::from_state(&st()).with_levels(Levels { levels: vec![400, 500], active: 0 });
        assert_eq!(v.dpi_levels, vec![400, 500]);
        assert_eq!(v.active_level, 0);
        assert_eq!(v.levels(), Levels { levels: vec![400, 500], active: 0 });
    }
    #[test]
    fn default_view_state_is_connecting_and_empty() {
        let v = ViewState::default();
        assert_eq!(v.status, DaemonStatus::Connecting);
        assert!(v.device.is_none() && v.choices.is_empty());
    }
}
