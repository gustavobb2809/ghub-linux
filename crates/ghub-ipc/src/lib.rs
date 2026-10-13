//! Contrato D-Bus entre o daemon `ghubd` e seus clientes (GUI, bandeja, CLI).

use serde::{Deserialize, Serialize};
use zbus::proxy;
use zvariant::Type;

pub const BUS_NAME: &str = "org.ghublinux.Daemon";
pub const OBJECT_PATH: &str = "/org/ghublinux/Daemon";
pub const INTERFACE: &str = "org.ghublinux.Daemon1";

/// Valor de `battery_percent` quando a bateria é desconhecida.
pub const BATTERY_UNKNOWN: i32 = -1;

pub const STATE_UNKNOWN: &str = "unknown";
pub const STATE_DISCHARGING: &str = "discharging";
pub const STATE_CHARGING: &str = "charging";
pub const STATE_FULL: &str = "full";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DeviceSummary {
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub battery_percent: i32,
    pub battery_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DeviceState {
    pub id: String,
    pub name: String,
    pub connected: bool,
    /// DPI lido do dispositivo agora; 0 se desconhecido (offline).
    pub dpi_current: u32,
    pub dpi_levels: Vec<u32>,
    pub active_level: u32,
    pub dpi_min: u32,
    pub dpi_max: u32,
    /// 0 quando o dispositivo anuncia uma lista explícita (ver `dpi_list`).
    pub dpi_step: u32,
    pub dpi_list: Vec<u32>,
    pub polling_hz: u32,
    pub polling_supported: Vec<u32>,
    pub battery_percent: i32,
    pub battery_state: String,
    pub profile: String,
}

/// LED do perfil onboard ativo. `effect`: off | fixed | cycle | breathing; `color`: `#RRGGBB` ou vazio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct LedSetting {
    pub effect: String,
    pub color: String,
    pub period_ms: u32,
    pub intensity: u32,
}

#[proxy(
    interface = "org.ghublinux.Daemon1",
    default_service = "org.ghublinux.Daemon",
    default_path = "/org/ghublinux/Daemon"
)]
pub trait Daemon {
    fn devices(&self) -> zbus::Result<Vec<DeviceSummary>>;
    fn get_device_state(&self, id: &str) -> zbus::Result<DeviceState>;
    fn set_dpi_levels(&self, id: &str, levels: Vec<u32>, active: u32) -> zbus::Result<()>;
    fn set_active_dpi_level(&self, id: &str, active: u32) -> zbus::Result<()>;
    fn set_polling_rate(&self, id: &str, hz: u32) -> zbus::Result<()>;
    /// Efeitos de LED suportados pelo dispositivo; vazio quando não há suporte.
    fn led_effects(&self, id: &str) -> zbus::Result<Vec<String>>;
    fn get_led(&self, id: &str) -> zbus::Result<LedSetting>;
    fn set_led(&self, id: &str, effect: &str, color: &str, period_ms: u32, intensity: u32) -> zbus::Result<()>;
    fn list_profiles(&self) -> zbus::Result<Vec<String>>;
    fn get_active_profile(&self) -> zbus::Result<String>;
    fn activate_profile(&self, name: &str) -> zbus::Result<()>;
    fn create_profile(&self, name: &str) -> zbus::Result<()>;
    fn delete_profile(&self, name: &str) -> zbus::Result<()>;
    fn rescan(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn devices_changed(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn device_changed(&self, id: String) -> zbus::Result<()>;
    #[zbus(signal)]
    fn profile_changed(&self, name: String) -> zbus::Result<()>;
    #[zbus(signal)]
    fn battery_low(&self, id: String, percent: u32) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use zvariant::serialized::Context;
    use zvariant::{to_bytes, LE};

    fn roundtrip<T>(value: &T) -> T
    where
        T: serde::Serialize + serde::de::DeserializeOwned + Type,
    {
        let bytes = to_bytes(Context::new_dbus(LE, 0), value).unwrap();
        let (back, _): (T, usize) = bytes.deserialize().unwrap();
        back
    }

    #[test]
    fn summary_signature_is_stable() {
        assert_eq!(DeviceSummary::SIGNATURE.to_string(), "(ssbis)");
    }

    #[test]
    fn summary_roundtrips() {
        let v = DeviceSummary {
            id: "046dc539-1".into(),
            name: "G703 LS".into(),
            connected: true,
            battery_percent: BATTERY_UNKNOWN,
            battery_state: STATE_UNKNOWN.into(),
        };
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn state_roundtrips() {
        let v = DeviceState {
            id: "046dc539-1".into(),
            name: "G703 LS".into(),
            connected: true,
            dpi_current: 1200,
            dpi_levels: vec![400, 800, 1600],
            active_level: 2,
            dpi_min: 100,
            dpi_max: 25600,
            dpi_step: 50,
            dpi_list: vec![],
            polling_hz: 1000,
            polling_supported: vec![125, 250, 500, 1000],
            battery_percent: 91,
            battery_state: STATE_DISCHARGING.into(),
            profile: "Padrão".into(),
        };
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn led_setting_signature_is_stable() {
        assert_eq!(LedSetting::SIGNATURE.to_string(), "(ssuu)");
    }

    #[test]
    fn led_setting_roundtrips() {
        let v = LedSetting { effect: "fixed".into(), color: "#FE8019".into(), period_ms: 0, intensity: 0 };
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn constants_match_the_spec() {
        assert_eq!(BUS_NAME, "org.ghublinux.Daemon");
        assert_eq!(OBJECT_PATH, "/org/ghublinux/Daemon");
        assert_eq!(INTERFACE, "org.ghublinux.Daemon1");
    }
}
