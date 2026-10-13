use super::{ApiError, DaemonApi};
use ghub_ipc::{DeviceState, DeviceSummary, LedSetting, STATE_DISCHARGING};
use std::sync::Mutex;

pub struct FakeInner {
    pub running: bool,
    pub devices: Vec<DeviceState>,
    pub profiles: Vec<String>,
    pub active_profile: String,
    pub autostart: Option<bool>,
    pub calls: Vec<String>,
    pub fail_next: Option<ApiError>,
    pub start_ok: bool,
    pub led: LedSetting,
    pub led_effects: Vec<String>,
    /// Falha só a próxima gravação de LED (não atrapalha as leituras do `refresh`).
    pub fail_next_write: Option<ApiError>,
}
pub struct FakeDaemon { pub inner: Mutex<FakeInner> }

pub fn sample_state() -> DeviceState {
    DeviceState {
        id: "dev-1".into(), name: "G703 LS".into(), connected: true,
        dpi_current: 1200, dpi_levels: vec![800, 1200, 1600], active_level: 1,
        dpi_min: 100, dpi_max: 25600, dpi_step: 50, dpi_list: vec![],
        polling_hz: 1000, polling_supported: vec![125, 250, 500, 1000],
        battery_percent: 91, battery_state: STATE_DISCHARGING.into(), profile: "Padrão".into(),
    }
}

impl FakeDaemon {
    pub fn new() -> Self {
        FakeDaemon { inner: Mutex::new(FakeInner {
            running: true, devices: vec![sample_state()], profiles: vec!["Padrão".into()],
            active_profile: "Padrão".into(), autostart: Some(false), calls: vec![],
            fail_next: None, start_ok: true,
            led: LedSetting { effect: "cycle".into(), color: String::new(), period_ms: 8000, intensity: 0 },
            led_effects: ["off", "fixed", "cycle", "breathing"].map(String::from).to_vec(),
            fail_next_write: None,
        }) }
    }
    pub fn scenario(name: &str) -> Self {
        let d = Self::new();
        {
            let mut i = d.inner.lock().unwrap();
            match name {
                "stopped" => i.running = false,
                "no-led" => i.led_effects.clear(),
                "sleeping" => { let s = &mut i.devices[0]; s.connected = false; s.dpi_current = 0; }
                _ => {}
            }
        }
        d
    }
    pub fn calls(&self) -> Vec<String> { self.inner.lock().unwrap().calls.clone() }

    fn record(&self, call: String) -> Result<std::sync::MutexGuard<'_, FakeInner>, ApiError> {
        let mut i = self.inner.lock().unwrap();
        i.calls.push(call);
        if !i.running { return Err(ApiError::Unavailable); }
        if let Some(e) = i.fail_next.take() { return Err(e); }
        Ok(i)
    }
}
impl Default for FakeDaemon { fn default() -> Self { Self::new() } }

fn unknown() -> ApiError { ApiError::Failed("dispositivo desconhecido".into()) }

impl DaemonApi for FakeDaemon {
    fn daemon_running(&self) -> bool { self.inner.lock().unwrap().running }
    fn start_daemon(&self) -> Result<(), String> {
        let mut i = self.inner.lock().unwrap();
        if i.start_ok { i.running = true; Ok(()) } else { Err("falhou".into()) }
    }
    fn devices(&self) -> Result<Vec<DeviceSummary>, ApiError> {
        let i = self.record("devices".into())?;
        Ok(i.devices.iter().map(|d| DeviceSummary {
            id: d.id.clone(), name: d.name.clone(), connected: d.connected,
            battery_percent: d.battery_percent, battery_state: d.battery_state.clone(),
        }).collect())
    }
    fn get_device_state(&self, id: &str) -> Result<DeviceState, ApiError> {
        let i = self.record(format!("get_device_state {id}"))?;
        i.devices.iter().find(|d| d.id == id).cloned().ok_or_else(unknown)
    }
    fn set_dpi_levels(&self, id: &str, levels: Vec<u32>, active: u32) -> Result<(), ApiError> {
        let mut i = self.record(format!("set_dpi_levels {id} {levels:?} {active}"))?;
        let d = i.devices.iter_mut().find(|d| d.id == id).ok_or_else(unknown)?;
        if d.connected { if let Some(v) = levels.get(active as usize) { d.dpi_current = *v; } }
        d.dpi_levels = levels;
        d.active_level = active;
        Ok(())
    }
    fn set_active_dpi_level(&self, id: &str, active: u32) -> Result<(), ApiError> {
        let mut i = self.record(format!("set_active_dpi_level {id} {active}"))?;
        let d = i.devices.iter_mut().find(|d| d.id == id).ok_or_else(unknown)?;
        d.active_level = active;
        if d.connected { if let Some(v) = d.dpi_levels.get(active as usize) { d.dpi_current = *v; } }
        Ok(())
    }
    fn set_polling_rate(&self, id: &str, hz: u32) -> Result<(), ApiError> {
        let mut i = self.record(format!("set_polling_rate {id} {hz}"))?;
        let d = i.devices.iter_mut().find(|d| d.id == id).ok_or_else(unknown)?;
        d.polling_hz = hz;
        Ok(())
    }
    // As leituras de LED não entram em `calls`: não alteram as listas exatas dos testes antigos.
    fn led_effects(&self, _id: &str) -> Result<Vec<String>, ApiError> {
        let i = self.inner.lock().unwrap();
        if !i.running { return Err(ApiError::Unavailable); }
        Ok(i.led_effects.clone())
    }
    fn get_led(&self, _id: &str) -> Result<LedSetting, ApiError> {
        let i = self.inner.lock().unwrap();
        if !i.running { return Err(ApiError::Unavailable); }
        Ok(i.led.clone())
    }
    fn set_led(&self, id: &str, s: &LedSetting) -> Result<(), ApiError> {
        let mut i = self.inner.lock().unwrap();
        i.calls.push(format!("set_led {id} {} {} {} {}", s.effect, s.color, s.period_ms, s.intensity));
        if !i.running { return Err(ApiError::Unavailable); }
        if let Some(e) = i.fail_next_write.take() { return Err(e); }
        i.led = s.clone();
        Ok(())
    }
    fn list_profiles(&self) -> Result<Vec<String>, ApiError> {
        Ok(self.record("list_profiles".into())?.profiles.clone())
    }
    fn get_active_profile(&self) -> Result<String, ApiError> {
        Ok(self.record("get_active_profile".into())?.active_profile.clone())
    }
    fn activate_profile(&self, name: &str) -> Result<(), ApiError> {
        let mut i = self.record(format!("activate_profile {name}"))?;
        if !i.profiles.iter().any(|p| p == name) { return Err(ApiError::Failed("perfil desconhecido".into())); }
        i.active_profile = name.into();
        Ok(())
    }
    fn create_profile(&self, name: &str) -> Result<(), ApiError> {
        let mut i = self.record(format!("create_profile {name}"))?;
        if i.profiles.iter().any(|p| p == name) { return Err(ApiError::Failed("já existe".into())); }
        i.profiles.push(name.into());
        Ok(())
    }
    fn delete_profile(&self, name: &str) -> Result<(), ApiError> {
        let mut i = self.record(format!("delete_profile {name}"))?;
        i.profiles.retain(|p| p != name);
        if i.active_profile == name { i.active_profile = i.profiles.first().cloned().unwrap_or_default(); }
        Ok(())
    }
    fn rescan(&self) -> Result<(), ApiError> { self.record("rescan".into()).map(|_| ()) }
    fn autostart(&self) -> Option<bool> { self.inner.lock().unwrap().autostart }
    fn set_autostart(&self, enable: bool) -> Result<(), String> {
        let mut i = self.inner.lock().unwrap();
        i.calls.push(format!("set_autostart {enable}"));
        i.autostart = Some(enable);
        Ok(())
    }
}
