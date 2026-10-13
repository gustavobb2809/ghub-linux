//! Dublês compartilhados pelos testes do gerente e do serviço D-Bus.

use std::sync::{Arc, Mutex, MutexGuard};

use ghub_ipc::{DeviceState, DeviceSummary};
use hidpp::features::dpi::DpiSupport;
use hidpp::features::onboard::{Led, LedKind};
use hidpp::HidppError;

use crate::backend::{Backend, Found};
use crate::battery_sysfs::BatteryInfo;
use crate::link::{Launcher, ManagerLink, ServiceControl};
use crate::manager::ManagerError;
use crate::mouse::{Capabilities, Mouse, MouseResult};
use crate::notify::{Notification, Notifier};
use crate::tray_model::Autostart;

pub struct FakeState {
    pub online: bool,
    pub dpi: u16,
    pub polling: u16,
    pub caps: Capabilities,
    pub dpi_sets: Vec<u16>,
    pub polling_sets: Vec<u16>,
    pub led: Led,
    pub led_sets: Vec<Led>,
    /// Recarregar o perfil onboard (set_led) devolve o DPI a este valor (simula o reset real).
    pub dpi_after_led: Option<u16>,
    /// Com o mouse online, leituras falham.
    pub fail_reads: bool,
    /// Com o mouse online, escritas falham (e não são registradas).
    pub fail_writes: bool,
    /// Com o mouse online, `ping` falha.
    pub fail_ping: bool,
    /// Com o mouse online, `capabilities` falha (dispositivo sem DPI/polling utilizáveis).
    pub fail_caps: bool,
}

#[derive(Clone)]
pub struct FakeMouse(pub Arc<Mutex<FakeState>>);

impl FakeMouse {
    pub fn new(dpi: u16, polling: u16) -> Self {
        FakeMouse(Arc::new(Mutex::new(FakeState {
            online: true,
            dpi,
            polling,
            caps: Capabilities {
                dpi: DpiSupport::Range { min: 100, max: 25600, step: 50 },
                polling_hz: vec![125, 250, 500, 1000],
                led: LedKind::ALL.to_vec(),
            },
            dpi_sets: Vec::new(),
            polling_sets: Vec::new(),
            led: Led::Cycle { period_ms: 8000, intensity: 0 },
            led_sets: Vec::new(),
            dpi_after_led: None,
            fail_reads: false,
            fail_writes: false,
            fail_ping: false,
            fail_caps: false,
        })))
    }

    pub fn state(&self) -> MutexGuard<'_, FakeState> {
        self.0.lock().unwrap()
    }
}

impl Mouse for FakeMouse {
    fn capabilities(&mut self) -> MouseResult<Capabilities> {
        let s = self.state();
        if s.online && s.fail_caps {
            return Err(HidppError::Timeout);
        }
        Ok(s.caps.clone())
    }

    fn read_dpi(&mut self) -> MouseResult<u16> {
        let s = self.state();
        if s.online && !s.fail_reads { Ok(s.dpi) } else { Err(HidppError::Timeout) }
    }

    fn read_polling_hz(&mut self) -> MouseResult<u16> {
        let s = self.state();
        if s.online && !s.fail_reads { Ok(s.polling) } else { Err(HidppError::Timeout) }
    }

    fn set_dpi(&mut self, dpi: u16) -> MouseResult<u16> {
        let mut s = self.state();
        if !s.online || s.fail_writes {
            return Err(HidppError::Timeout);
        }
        s.dpi = dpi;
        s.dpi_sets.push(dpi);
        Ok(dpi)
    }

    fn set_polling_hz(&mut self, hz: u16) -> MouseResult<()> {
        let mut s = self.state();
        if !s.online || s.fail_writes {
            return Err(HidppError::Timeout);
        }
        s.polling = hz;
        s.polling_sets.push(hz);
        Ok(())
    }

    fn led(&mut self) -> MouseResult<Led> {
        let s = self.state();
        if s.online && !s.fail_reads { Ok(s.led) } else { Err(HidppError::Timeout) }
    }

    fn set_led(&mut self, led: &Led) -> MouseResult<()> {
        let mut s = self.state();
        if !s.online || s.fail_writes {
            return Err(HidppError::Timeout);
        }
        s.led = *led;
        s.led_sets.push(*led);
        if let Some(dpi) = s.dpi_after_led {
            s.dpi = dpi;
        }
        Ok(())
    }

    fn ping(&mut self) -> MouseResult<()> {
        let s = self.state();
        if s.online && !s.fail_ping { Ok(()) } else { Err(HidppError::Timeout) }
    }
}

pub struct FakeBackend {
    pub mice: Vec<(String, String, FakeMouse)>,
    pub battery: Arc<Mutex<Option<BatteryInfo>>>,
    /// Lista `skip` recebida na última varredura.
    pub last_skip: Vec<String>,
}

impl FakeBackend {
    /// Um backend com um único mouse `dev-1` ("G703 LS") e um handle para a bateria.
    pub fn with_mouse(mouse: FakeMouse) -> (Self, Arc<Mutex<Option<BatteryInfo>>>) {
        let battery = Arc::new(Mutex::new(None));
        (
            FakeBackend {
                mice: vec![("dev-1".to_string(), "G703 LS".to_string(), mouse)],
                battery: battery.clone(),
                last_skip: Vec::new(),
            },
            battery,
        )
    }
}

impl Backend for FakeBackend {
    fn scan(&mut self, skip: &[String]) -> Vec<Found> {
        self.last_skip = skip.to_vec();
        self.mice
            .iter()
            .filter(|(id, _, m)| m.state().online && !skip.contains(id))
            .map(|(id, name, m)| Found {
                id: id.clone(),
                name: name.clone(),
                mouse: Box::new(m.clone()),
            })
            .collect()
    }

    fn battery(&mut self, _id: &str) -> Option<BatteryInfo> {
        self.battery.lock().unwrap().clone()
    }
}

pub fn sample_summary(id: &str, connected: bool) -> DeviceSummary {
    DeviceSummary {
        id: id.into(),
        name: format!("Mouse {id}"),
        connected,
        battery_percent: 80,
        battery_state: "discharging".into(),
    }
}

pub fn sample_state(id: &str, connected: bool) -> DeviceState {
    DeviceState {
        id: id.into(),
        name: format!("Mouse {id}"),
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
        battery_percent: 80,
        battery_state: "discharging".into(),
        profile: "Padrão".into(),
    }
}

pub struct FakeLink {
    pub devices: Mutex<Vec<DeviceSummary>>,
    pub state: Mutex<Option<DeviceState>>,
    pub profiles: Mutex<Vec<String>>,
    pub active: Mutex<String>,
    pub level_result: Mutex<Result<(), ManagerError>>,
    pub profile_result: Mutex<Result<(), ManagerError>>,
    pub calls: Mutex<Vec<String>>,
}

impl Default for FakeLink {
    fn default() -> Self {
        FakeLink {
            devices: Mutex::new(Vec::new()),
            state: Mutex::new(None),
            profiles: Mutex::new(Vec::new()),
            active: Mutex::new(String::new()),
            level_result: Mutex::new(Ok(())),
            profile_result: Mutex::new(Ok(())),
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl FakeLink {
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl ManagerLink for FakeLink {
    fn devices(&self) -> Vec<DeviceSummary> {
        self.devices.lock().unwrap().clone()
    }
    fn state(&self, _id: &str) -> Option<DeviceState> {
        self.state.lock().unwrap().clone()
    }
    fn profiles(&self) -> Vec<String> {
        self.profiles.lock().unwrap().clone()
    }
    fn active_profile(&self) -> String {
        self.active.lock().unwrap().clone()
    }
    fn set_active_level(&self, id: &str, level: usize) -> Result<(), ManagerError> {
        self.calls.lock().unwrap().push(format!("set_active_level:{id}:{level}"));
        self.level_result.lock().unwrap().clone()
    }
    fn activate_profile(&self, name: &str) -> Result<(), ManagerError> {
        self.calls.lock().unwrap().push(format!("activate_profile:{name}"));
        self.profile_result.lock().unwrap().clone()
    }
    fn shutdown(&self) {
        self.calls.lock().unwrap().push("shutdown".into());
    }
}

pub struct FakeService {
    pub autostart: Mutex<Autostart>,
    pub set_result: Mutex<Result<(), String>>,
    pub stop_result: Mutex<Result<bool, String>>,
    pub calls: Mutex<Vec<String>>,
}

impl Default for FakeService {
    fn default() -> Self {
        FakeService {
            autostart: Mutex::new(Autostart::Disabled),
            set_result: Mutex::new(Ok(())),
            stop_result: Mutex::new(Ok(true)),
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl FakeService {
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl ServiceControl for FakeService {
    fn autostart(&self) -> Autostart {
        *self.autostart.lock().unwrap()
    }
    fn set_autostart(&self, enable: bool) -> Result<(), String> {
        self.calls.lock().unwrap().push(format!("set_autostart:{enable}"));
        self.set_result.lock().unwrap().clone()
    }
    fn stop(&self) -> Result<bool, String> {
        self.calls.lock().unwrap().push("stop".into());
        self.stop_result.lock().unwrap().clone()
    }
}

pub struct FakeLauncher {
    pub result: Mutex<Result<(), String>>,
    pub opened: Mutex<u32>,
}

impl Default for FakeLauncher {
    fn default() -> Self {
        FakeLauncher { result: Mutex::new(Ok(())), opened: Mutex::new(0) }
    }
}

impl Launcher for FakeLauncher {
    fn open_gui(&self) -> Result<(), String> {
        *self.opened.lock().unwrap() += 1;
        self.result.lock().unwrap().clone()
    }
}

#[derive(Default)]
pub struct FakeNotifier {
    pub sent: Mutex<Vec<Notification>>,
}

impl FakeNotifier {
    pub fn sent(&self) -> Vec<Notification> {
        self.sent.lock().unwrap().clone()
    }
}

impl Notifier for FakeNotifier {
    fn notify(&self, n: &Notification) {
        self.sent.lock().unwrap().push(n.clone());
    }
}
