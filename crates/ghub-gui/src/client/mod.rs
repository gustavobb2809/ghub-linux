use ghub_ipc::{DeviceState, DeviceSummary, LedSetting};
use std::fmt;

pub mod controller;
#[cfg(any(test, feature = "fake-daemon"))]
pub mod fake;
pub mod real;
pub mod single_instance;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError { Unavailable, Failed(String) }

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Unavailable => write!(f, "O serviço ghubd não está rodando."),
            ApiError::Failed(m) => write!(f, "{m}"),
        }
    }
}

pub trait DaemonApi: Send + Sync + 'static {
    fn daemon_running(&self) -> bool;
    fn start_daemon(&self) -> Result<(), String>;
    fn devices(&self) -> Result<Vec<DeviceSummary>, ApiError>;
    fn get_device_state(&self, id: &str) -> Result<DeviceState, ApiError>;
    fn set_dpi_levels(&self, id: &str, levels: Vec<u32>, active: u32) -> Result<(), ApiError>;
    fn set_active_dpi_level(&self, id: &str, active: u32) -> Result<(), ApiError>;
    fn set_polling_rate(&self, id: &str, hz: u32) -> Result<(), ApiError>;
    fn led_effects(&self, id: &str) -> Result<Vec<String>, ApiError>;
    fn get_led(&self, id: &str) -> Result<LedSetting, ApiError>;
    fn set_led(&self, id: &str, setting: &LedSetting) -> Result<(), ApiError>;
    fn list_profiles(&self) -> Result<Vec<String>, ApiError>;
    fn get_active_profile(&self) -> Result<String, ApiError>;
    fn activate_profile(&self, name: &str) -> Result<(), ApiError>;
    fn create_profile(&self, name: &str) -> Result<(), ApiError>;
    fn delete_profile(&self, name: &str) -> Result<(), ApiError>;
    fn rescan(&self) -> Result<(), ApiError>;
    fn autostart(&self) -> Option<bool>;
    fn set_autostart(&self, enable: bool) -> Result<(), String>;
}
