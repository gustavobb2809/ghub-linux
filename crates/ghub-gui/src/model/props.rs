use super::format::*;
use super::state::ViewState;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Props {
    pub daemon_state: String,
    pub has_device: bool, pub device_name: String, pub connected: bool,
    pub battery_percent: i32, pub battery_text: String, pub battery_low: bool,
    pub dpi_current: i32, pub dpi_text: String,
    pub dpi_levels_json: String, pub active_level: i32,
    pub dpi_min: i32, pub dpi_max: i32, pub dpi_step: i32,
    pub polling_hz: i32, pub polling_text: String, pub polling_options_json: String,
    pub profiles_json: String, pub active_profile: String,
    pub profile_differs: bool, pub profile_diff_text: String,
    pub autostart_known: bool, pub autostart_enabled: bool,
    pub devices_json: String, pub show_device_selector: bool, pub selected_device: String,
}

impl From<&ViewState> for Props {
    fn from(v: &ViewState) -> Self {
        let mut p = Props {
            daemon_state: v.status.as_str().into(),
            profiles_json: json_strings(&v.profiles),
            active_profile: v.active_profile.clone(),
            autostart_known: v.autostart.is_some(),
            autostart_enabled: v.autostart.unwrap_or(false),
            devices_json: serde_json::to_string(
                &v.choices.iter().map(|c| serde_json::json!({"id": c.id, "name": c.name})).collect::<Vec<_>>(),
            ).unwrap_or_else(|_| "[]".into()),
            show_device_selector: v.choices.len() >= 2,
            dpi_levels_json: "[]".into(), polling_options_json: "[]".into(),
            battery_percent: -1, battery_text: battery_text(-1, ""), dpi_text: dpi_text(0),
            polling_text: polling_text(0),
            ..Props::default()
        };
        if let Some(d) = &v.device {
            p.has_device = true;
            p.device_name = d.name.clone();
            p.selected_device = d.id.clone();
            p.connected = d.connected;
            p.battery_percent = d.battery_percent;
            p.battery_text = battery_text(d.battery_percent, &d.battery_state);
            p.battery_low = battery_is_low(d.battery_percent);
            p.dpi_current = d.dpi_current as i32;
            p.dpi_text = dpi_text(d.dpi_current);
            p.dpi_levels_json = json_u32s(&d.dpi_levels);
            p.active_level = d.active_level as i32;
            p.dpi_min = d.range.min as i32; p.dpi_max = d.range.max as i32; p.dpi_step = d.range.step as i32;
            p.polling_hz = d.polling_hz as i32;
            p.polling_text = polling_text(d.polling_hz);
            p.polling_options_json = json_u32s(&d.polling_options);
            p.profile_differs = d.profile_differs();
            if p.profile_differs {
                let want = d.dpi_levels.get(d.active_level as usize).copied().unwrap_or(0);
                p.profile_diff_text = format!("O mouse está em {} DPI, mas o perfil pede {} DPI.", d.dpi_current, want);
            }
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::state::{DaemonStatus, DeviceChoice, DeviceView};
    use crate::model::dpi::DpiRange;

    fn dev(connected: bool, dpi_current: u32) -> DeviceView {
        DeviceView {
            id: "m1".into(), name: "Mouse".into(), connected, dpi_current,
            dpi_levels: vec![800, 1200], active_level: 1,
            range: DpiRange { min: 100, max: 25600, step: 50, list: vec![] },
            polling_hz: 1000, polling_options: vec![500, 1000],
            battery_percent: 80, battery_state: "discharging".into(),
            led: None,
        }
    }
    fn with(d: DeviceView) -> ViewState { ViewState { device: Some(d), ..ViewState::default() } }

    #[test]
    fn default_state_props() {
        let p = Props::from(&ViewState::default());
        assert_eq!(p.daemon_state, "connecting");
        assert!(!p.has_device);
        assert_eq!(p.dpi_text, "lendo…");
        assert_eq!(p.dpi_levels_json, "[]");
        assert_eq!(p.polling_options_json, "[]");
        assert_eq!(p.devices_json, "[]");
    }
    #[test]
    fn connected_with_unknown_dpi_never_shows_zero() {
        let p = Props::from(&with(dev(true, 0)));
        assert_eq!(p.dpi_text, "lendo…");
        assert!(!p.dpi_text.contains('0'));
        assert!(!p.profile_differs);
        assert!(p.profile_diff_text.is_empty());
    }
    #[test]
    fn sleeping_device_is_present_but_not_connected() {
        let p = Props::from(&with(dev(false, 0)));
        assert!(p.has_device && !p.connected);
    }
    #[test]
    fn selector_needs_two_choices() {
        let c = |i: &str| DeviceChoice { id: i.into(), name: i.into() };
        let one = ViewState { choices: vec![c("a")], ..ViewState::default() };
        assert!(!Props::from(&one).show_device_selector);
        let two = ViewState { choices: vec![c("a"), c("b")], ..ViewState::default() };
        let p = Props::from(&two);
        assert!(p.show_device_selector);
        assert!(p.devices_json.contains("\"id\":\"a\""));
    }
    #[test]
    fn diff_text_mentions_both_dpis() {
        let p = Props::from(&with(dev(true, 1600)));
        assert!(p.profile_differs);
        assert!(p.profile_diff_text.contains("1600") && p.profile_diff_text.contains("1200"));
        assert_eq!(p.dpi_text, "1600 DPI");
        assert_eq!(p.dpi_min, 100);
    }
    #[test]
    fn stopped_status() {
        let v = ViewState { status: DaemonStatus::Stopped, autostart: Some(true), ..ViewState::default() };
        let p = Props::from(&v);
        assert_eq!(p.daemon_state, "stopped");
        assert!(p.autostart_known && p.autostart_enabled);
    }
}
